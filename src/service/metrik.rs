//! metrik.rs — Pengukuran latensi & hitungan jalur panas, di dalam proses.
//!
//! ── KENAPA BUKAN PROMETHEUS ───────────────────────────────────────────────
//! Endpoint `/metrics` hanya berguna bila ada Prometheus yang menariknya, dan
//! itu berarti satu proses lagi di mesin 8 GB yang sama — memakan jatah yang
//! justru sedang kita jaga. Histogram di sini seluruhnya atomik dan berukuran
//! tetap: dua belas `AtomicU64` per ukuran, tanpa alokasi, tanpa kunci, dan
//! tanpa satu pun byte yang tumbuh seiring lalu lintas.
//!
//! ── KENAPA PERSENTIL, BUKAN RATA-RATA ─────────────────────────────────────
//! Rata-rata menyembunyikan persis kejadian yang membuat orang mengeluh. Saat
//! situs ini jatuh kemarin, rata-rata latensi tetap tampak wajar karena
//! sebagian besar permintaan adalah aset statis yang cepat; yang rusak adalah
//! ekornya. p95 dan p99 memperlihatkannya, rata-rata tidak akan pernah.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::LazyLock;

/// Batas atas ember, dalam milidetik. Rapat di bawah 100 ms (tempat sebagian
/// besar permintaan sehat berada) dan renggang di atasnya — di atas satu detik
/// selisih antara 3 dan 4 detik tak mengubah tindakan siapa pun.
const BATAS_MS: [u64; 11] = [1, 2, 5, 10, 25, 50, 100, 250, 500, 1_000, 5_000];

/// Histogram berember tetap. Ember terakhir menampung segalanya di atas batas
/// tertinggi, sehingga tak ada pengamatan yang hilang.
#[derive(Default)]
pub struct Histogram {
    ember: [AtomicU64; BATAS_MS.len() + 1],
    jumlah: AtomicU64,
    total_ms: AtomicU64,
}

impl Histogram {
    pub const fn baru() -> Self {
        #[allow(clippy::declare_interior_mutable_const)]
        const NOL: AtomicU64 = AtomicU64::new(0);
        Self {
            ember: [NOL; BATAS_MS.len() + 1],
            jumlah: NOL,
            total_ms: NOL,
        }
    }

    pub fn catat(&self, ms: u64) {
        let i = BATAS_MS
            .iter()
            .position(|b| ms <= *b)
            .unwrap_or(BATAS_MS.len());
        // `Relaxed` cukup: yang dibutuhkan hanya atomisitas per penambah, bukan
        // urutan antar-penambah. Metrik yang menuntut urutan akan membebani
        // jalur panas demi ketepatan yang tak seorang pun baca.
        self.ember[i].fetch_add(1, Ordering::Relaxed);
        self.jumlah.fetch_add(1, Ordering::Relaxed);
        self.total_ms.fetch_add(ms, Ordering::Relaxed);
    }

    pub fn jumlah(&self) -> u64 {
        self.jumlah.load(Ordering::Relaxed)
    }

    /// Persentil, dalam milidetik.
    ///
    /// Mengembalikan BATAS ATAS ember tempat persentilnya jatuh — jadi bacanya
    /// "p95 paling banyak sekian ms", bukan angka pasti. Interpolasi di dalam
    /// ember akan mengarang ketepatan yang datanya memang tak punya.
    ///
    /// `None` bila belum ada pengamatan sama sekali — dibedakan dari nol, yang
    /// akan terbaca sebagai "sangat cepat" padahal artinya "belum tahu".
    pub fn persentil(&self, p: f64) -> Option<u64> {
        let total = self.jumlah();
        if total == 0 {
            return None;
        }
        let sasaran = (total as f64 * p).ceil() as u64;
        let mut kumulatif = 0u64;
        for (i, e) in self.ember.iter().enumerate() {
            kumulatif += e.load(Ordering::Relaxed);
            if kumulatif >= sasaran {
                return Some(BATAS_MS.get(i).copied().unwrap_or(u64::MAX));
            }
        }
        Some(u64::MAX)
    }

    pub fn rata_rata_ms(&self) -> Option<u64> {
        let n = self.jumlah();
        if n == 0 {
            return None;
        }
        Some(self.total_ms.load(Ordering::Relaxed) / n)
    }
}

/// Seluruh ukuran aplikasi. Global karena jalur yang diukur tersebar di
/// service, repository, dan WebSocket — menyalurkannya lewat parameter akan
/// menyentuh puluhan tanda tangan demi sesuatu yang murni pengamatan.
pub struct Metrik {
    /// Dari bingkai WS tiba sampai ACK dikirim — yang benar-benar dirasakan
    /// orang saat menekan kirim.
    pub chat_kirim: Histogram,
    /// Lama satu kueri basis data.
    pub db_kueri: Histogram,
    /// Lama `PUBLISH` ke Redis, termasuk percobaan ulangnya.
    pub redis_publish: Histogram,

    /// Pesan yang dibuang karena kanal penerimanya penuh — koneksi lambat.
    pub pesan_dibuang: AtomicU64,
    /// Berapa kali sesi digantikan koneksi baru. Angka yang melonjak berarti
    /// badai sambung-ulang, bukan pemakaian yang meningkat.
    pub sesi_diganti: AtomicU64,
}

impl Metrik {
    const fn baru() -> Self {
        Self {
            chat_kirim: Histogram::baru(),
            db_kueri: Histogram::baru(),
            redis_publish: Histogram::baru(),
            pesan_dibuang: AtomicU64::new(0),
            sesi_diganti: AtomicU64::new(0),
        }
    }
}

pub static METRIK: LazyLock<Metrik> = LazyLock::new(Metrik::baru);

/// Ukur satu blok lalu catat lamanya. Dipakai sebagai penjaga:
/// `let _u = ukur(&METRIK.db_kueri);`
pub struct Ukur<'a> {
    hist: &'a Histogram,
    mulai: std::time::Instant,
}

impl<'a> Ukur<'a> {
    pub fn baru(hist: &'a Histogram) -> Self {
        Self {
            hist,
            mulai: std::time::Instant::now(),
        }
    }
}

impl Drop for Ukur<'_> {
    fn drop(&mut self) {
        // Dicatat saat DROP, bukan lewat pemanggilan eksplisit: jalur yang
        // diukur punya banyak `?` dan `return` lebih awal, dan pencatatan
        // manual pasti terlewat justru di jalur GAGAL — yaitu jalur yang paling
        // ingin diketahui lamanya.
        self.hist.catat(self.mulai.elapsed().as_millis() as u64);
    }
}

pub fn ukur(hist: &Histogram) -> Ukur<'_> {
    Ukur::baru(hist)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kosong_belum_tahu_bukan_nol() {
        let h = Histogram::baru();
        // Nol akan terbaca "sangat cepat"; yang benar adalah "belum ada data".
        assert_eq!(h.persentil(0.95), None);
        assert_eq!(h.rata_rata_ms(), None);
        assert_eq!(h.jumlah(), 0);
    }

    #[test]
    fn persentil_mengikuti_sebaran() {
        let h = Histogram::baru();
        // 99 permintaan cepat, 1 sangat lambat — sebaran yang menjatuhkan situs.
        for _ in 0..99 {
            h.catat(3);
        }
        h.catat(4_000);

        assert_eq!(h.jumlah(), 100);
        assert_eq!(h.persentil(0.5), Some(5));
        assert_eq!(h.persentil(0.95), Some(5));
        // p99 masih menangkap yang cepat; p100 barulah yang lambat.
        assert_eq!(h.persentil(1.0), Some(5_000));
    }

    /// Inti dari kenapa rata-rata tak dipakai: satu ekor panjang tenggelam.
    #[test]
    fn rata_rata_menyembunyikan_ekor() {
        let h = Histogram::baru();
        for _ in 0..999 {
            h.catat(2);
        }
        h.catat(5_000);
        // Rata-rata terbaca sehat…
        assert!(h.rata_rata_ms().unwrap() < 10);
        // …padahal ada permintaan yang memakan lima detik.
        assert_eq!(h.persentil(1.0), Some(5_000));
    }

    #[test]
    fn di_atas_batas_tertinggi_tak_hilang() {
        let h = Histogram::baru();
        h.catat(60_000);
        assert_eq!(h.jumlah(), 1);
        // Ember terakhir tak berbatas atas.
        assert_eq!(h.persentil(0.5), Some(u64::MAX));
    }

    #[test]
    fn tepat_di_batas_masuk_ember_itu() {
        let h = Histogram::baru();
        h.catat(100);
        assert_eq!(h.persentil(1.0), Some(100));
    }

    #[test]
    fn penjaga_mencatat_saat_drop() {
        let h = Histogram::baru();
        {
            let _u = Ukur::baru(&h);
        }
        assert_eq!(h.jumlah(), 1);
    }
}
