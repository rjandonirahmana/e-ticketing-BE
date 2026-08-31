//! affinity.rs — Behavior tracking user (server-side) dengan buffer + batch flush.
//!
//! Masalah versi lama (`reco.rs` langsung INSERT): setiap page-view user login =
//! sampai 3 round-trip Postgres di jalur request. Pada ratusan ribu user/hari
//! itu ratusan ribu write kecil yang berebut koneksi pool dengan checkout.
//!
//! Desain baru:
//! - `record()` hanya menulis ke HashMap in-memory (nanodetik, tanpa await) —
//!   jalur request tidak pernah menunggu DB untuk telemetri.
//! - Satu task background mem-flush buffer tiap `FLUSH_INTERVAL` sebagai SATU
//!   statement UNNEST (N sinyal = 1 round-trip), di luar jalur request.
//! - Skor lama di-decay saat upsert (`DECAY_PER_DAY^hari`) sehingga minat lama
//!   memudar — konsisten dengan decay localStorage di sisi client.
//! - Sinyal berbobot: view=1, add-to-cart=3, purchase=5. Memilih/membeli produk
//!   jauh lebih kuat menandakan minat daripada sekadar membuka halaman.
//!
//! Data ini telemetri best-effort: bila DB sedang jenuh, buffer dibatasi
//! (`MAX_BUFFERED`) dan sinyal termuda dibuang — jangan pernah menukar stabilitas
//! server dengan data rekomendasi.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use deadpool_postgres::Pool;

/// Interval flush buffer → Postgres. 5 dtk cukup real-time untuk rekomendasi
/// (dibaca dengan cache 30–60 dtk) tanpa membebani DB.
const FLUSH_INTERVAL: Duration = Duration::from_secs(5);
/// Faktor decay skor per hari (0.977^30 ≈ 0.5 → half-life ±30 hari).
const DECAY_PER_DAY: f64 = 0.977;
/// Plafon entri buffer. Terlampaui (mis. DB down berjam-jam) → sinyal baru
/// dibuang. ~100k entri ≈ beberapa MB — aman untuk RAM 8 GB.
const MAX_BUFFERED: usize = 100_000;
/// Maksimum kategori yang dicatat per sinyal (samakan dengan versi lama).
const MAX_CATS_PER_SIGNAL: usize = 3;

/// Jenis perilaku user terhadap sebuah produk/product.
#[derive(Debug, Clone, Copy)]
pub enum AffinitySignal {
    /// Membuka halaman detail product.
    View,
    /// MENCARI sesuatu, lalu kategori hasilnya dicatat.
    ///
    /// Bobotnya di ATAS `View` dan di bawah `Cart`, dan itu disengaja:
    /// mengetik kata pencarian adalah niat yang dinyatakan sendiri oleh
    /// pengguna, sedangkan membuka satu produk bisa terjadi karena kebetulan
    /// tergulir atau salah ketuk. Tetapi ia belum memilih apa pun, jadi belum
    /// sekuat memasukkan barang ke keranjang.
    Search,
    /// Menambahkan tiket ke keranjang (memilih produk).
    Cart,
    /// Menyelesaikan pembelian.
    Purchase,
}

impl AffinitySignal {
    fn weight(self) -> f64 {
        match self {
            AffinitySignal::View => 1.0,
            AffinitySignal::Search => 2.0,
            AffinitySignal::Cart => 3.0,
            AffinitySignal::Purchase => 5.0,
        }
    }

    /// Parse dari string client (server fn). Tak dikenal → View (bobot terkecil),
    /// supaya client versi lama/berubah tidak bisa memompa skor.
    pub fn from_str_lossy(s: &str) -> Self {
        match s {
            "cart" => AffinitySignal::Cart,
            "search" => AffinitySignal::Search,
            "purchase" => AffinitySignal::Purchase,
            _ => AffinitySignal::View,
        }
    }
}

/// SQL upsert dengan decay waktu: skor lama meluruh dulu sesuai umur barunya,
/// baru ditambah bobot sinyal baru. Dipakai flush batch & purchase.
const UPSERT_DECAY: &str = "ON CONFLICT (user_id, category) DO UPDATE SET \
     score = user_affinity.score \
             * POWER($DECAY, GREATEST(EXTRACT(EPOCH FROM (NOW() - user_affinity.updated_at)), 0) / 86400.0) \
             + EXCLUDED.score, \
     updated_at = NOW()";

fn upsert_decay_sql() -> String {
    UPSERT_DECAY.replace("$DECAY", &DECAY_PER_DAY.to_string())
}

pub struct AffinityService {
    pool: Pool,
    /// (user_id_hex, category) → akumulasi bobot sejak flush terakhir.
    buf: Mutex<HashMap<(String, String), f64>>,
}

impl AffinityService {
    /// Buat service + spawn task flush background. Task berhenti sendiri bila
    /// service di-drop (pegangannya Weak).
    pub fn new(pool: Pool) -> Arc<Self> {
        let svc = Arc::new(Self {
            pool,
            buf: Mutex::new(HashMap::new()),
        });
        let weak = Arc::downgrade(&svc);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(FLUSH_INTERVAL);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tick.tick().await;
                let Some(svc) = weak.upgrade() else { break };
                svc.flush().await;
            }
        });
        svc
    }

    /// Catat sinyal minat. Murni operasi memori — aman dipanggil di jalur
    /// request sepanas apa pun.
    pub fn record(&self, user_id_hex: &str, categories: &[String], signal: AffinitySignal) {
        // Satu user_id non-hex akan menggagalkan decode() SELURUH batch UNNEST —
        // tolak di sini agar satu input rusak tak menghanguskan sinyal user lain.
        if user_id_hex.is_empty()
            || user_id_hex.len() % 2 != 0
            || !user_id_hex.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return;
        }
        let w = signal.weight();
        let mut buf = self.buf.lock().unwrap();
        for c in categories.iter().take(MAX_CATS_PER_SIGNAL) {
            let c = c.trim();
            if c.is_empty() {
                continue;
            }
            let key = (user_id_hex.to_string(), c.to_string());
            if buf.len() >= MAX_BUFFERED && !buf.contains_key(&key) {
                return; // buffer penuh (DB macet lama) → buang sinyal baru
            }
            *buf.entry(key).or_insert(0.0) += w;
        }
    }

    /// Sinyal purchase dari order yang baru dibuat: kategori product diambil lewat
    /// satu query join (cart_items → product_variants → products) dan langsung
    /// di-upsert. Dijalankan background — checkout tidak menunggu.
    pub fn record_purchase(self: &Arc<Self>, user_id_hex: String, order_id_hex: String) {
        let svc = self.clone();
        tokio::spawn(async move {
            let sql = format!(
                "INSERT INTO user_affinity (user_id, category, score, updated_at) \
                 SELECT decode($1,'hex'), cat.value, $3::float8, NOW() \
                 FROM orders o \
                 JOIN cart_items ci ON ci.cart_id = o.cart_id \
                 JOIN product_variants tv ON tv.id = ci.ticket_variant_id \
                 JOIN products e ON e.id = tv.event_id \
                 CROSS JOIN LATERAL jsonb_array_elements_text(e.category) AS cat(value) \
                 WHERE o.id = decode($2,'hex') \
                   AND jsonb_typeof(e.category) = 'array' \
                 GROUP BY cat.value \
                 {}",
                upsert_decay_sql()
            );
            let client = match svc.pool.get().await {
                Ok(c) => c,
                Err(e) => {
                    tracing::debug!(error = %e, "affinity purchase: pool busy — sinyal dilewati");
                    return;
                }
            };
            if let Err(e) = client
                .execute(&sql, &[&user_id_hex, &order_id_hex, &AffinitySignal::Purchase.weight()])
                .await
            {
                tracing::debug!(error = %e, "affinity purchase: upsert gagal (best-effort)");
            }
        });
    }

    /// Kuras buffer → satu statement UNNEST. Gagal = data hangus (best-effort);
    /// jangan retry/menahan buffer agar memori tak membengkak saat DB bermasalah.
    async fn flush(&self) {
        let drained: Vec<((String, String), f64)> = {
            let mut buf = self.buf.lock().unwrap();
            if buf.is_empty() {
                return;
            }
            buf.drain().collect()
        };

        let mut users = Vec::with_capacity(drained.len());
        let mut cats = Vec::with_capacity(drained.len());
        let mut scores = Vec::with_capacity(drained.len());
        for ((u, c), s) in &drained {
            users.push(u.as_str());
            cats.push(c.as_str());
            scores.push(*s);
        }

        let client = match self.pool.get().await {
            Ok(c) => c,
            Err(e) => {
                tracing::debug!(error = %e, dropped = drained.len(), "affinity flush: pool busy — batch dibuang");
                return;
            }
        };

        let sql = format!(
            "INSERT INTO user_affinity (user_id, category, score, updated_at) \
             SELECT decode(t.u,'hex'), t.c, t.s, NOW() \
             FROM UNNEST($1::text[], $2::text[], $3::float8[]) AS t(u, c, s) \
             {}",
            upsert_decay_sql()
        );
        match client.execute(&sql, &[&users, &cats, &scores]).await {
            Ok(n) => tracing::debug!(rows = n, "affinity flush ok"),
            Err(e) => {
                tracing::debug!(error = %e, dropped = drained.len(), "affinity flush gagal (best-effort)")
            }
        }
    }
}
