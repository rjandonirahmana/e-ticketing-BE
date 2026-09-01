use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use super::sfu::SfuCommand;

/// Identitas penonton yang sedang menonton siaran. Dikirim klien saat subscribe
/// dan ditampilkan ke merchant ("siapa saja yang join"). `photo_url` opsional —
/// model user belum punya foto, jadi UI memakai avatar inisial sebagai fallback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewerInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub photo_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomInfo {
    pub room_id: String,
    pub merchant_id: String,
    pub merchant_name: String,
    pub event_slug: Option<String>,
    pub viewer_count: usize,
    pub started_at: i64,
    /// Daftar penonton yang sedang bergabung.
    #[serde(default)]
    pub viewers: Vec<ViewerInfo>,
    /// Produk yang sedang DIJUAL dalam siaran ini, urut sesuai pilihan merchant.
    ///
    /// Hanya id-nya. Rinciannya (nama, harga, sampul) diambil penonton lewat
    /// `get_merchant_public_products` yang sudah ada — room sudah tahu
    /// `merchant_id`, jadi tak ada kueri baru yang perlu ditulis, dan daftar
    /// ini tak pernah menyimpan harga basi.
    #[serde(default)]
    pub product_ids: Vec<String>,
}

pub struct LiveRoom {
    pub room_id: String,
    pub merchant_id: String,
    pub merchant_name: String,
    pub event_slug: Option<String>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub cmd_tx: mpsc::Sender<SfuCommand>,
    /// connection_id → info penonton (satu user bisa punya beberapa koneksi/tab).
    subscribers: DashMap<String, ViewerInfo>,
    /// Produk yang dipilih merchant untuk dijual selama siaran ini.
    ///
    /// DI MEMORI, bukan di basis data, dan itu disengaja: pilihannya berlaku
    /// untuk SATU siaran dan mati bersamanya. Menyimpannya di Postgres berarti
    /// baris yang wajib dibersihkan saat siaran berakhir — termasuk saat
    /// berakhirnya tak wajar (tab ditutup, proses mati) — untuk data yang tak
    /// seorang pun perlukan sesudahnya. Room ini sendiri sudah fana dengan cara
    /// yang sama.
    ///
    /// Nilainya = urutan tampil, supaya susunan yang dipilih merchant terjaga.
    products: DashMap<String, i32>,
    /// user_id → jumlah koneksi hidup. Menjaga `viewer_count()` tetap O(1) &
    /// ter-dedupe tanpa alokasi HashSet tiap panggilan (dipanggil di jalur join
    /// panas). CATATAN: AtomicUsize naif TIDAK bisa dipakai di sini karena akan
    /// menghitung ganda user multi-tab; refcount per-user inilah yang benar.
    unique_viewers: DashMap<String, u32>,
}

impl LiveRoom {
    pub fn new(
        room_id: String,
        merchant_id: String,
        merchant_name: String,
        event_slug: Option<String>,
        cmd_tx: mpsc::Sender<SfuCommand>,
    ) -> Self {
        Self {
            room_id,
            merchant_id,
            merchant_name,
            event_slug,
            started_at: chrono::Utc::now(),
            cmd_tx,
            subscribers: DashMap::new(),
            products: DashMap::new(),
            unique_viewers: DashMap::new(),
        }
    }

    /// Ganti seluruh daftar produk yang dijual di siaran ini.
    ///
    /// Mengganti, bukan menambah: merchant mengirim daftar utuh dari layarnya,
    /// jadi menghapus satu produk di sana harus benar-benar menghapusnya di
    /// sini. Menggabung akan membuat produk yang sudah dicabut tetap tampil di
    /// keranjang penonton tanpa cara apa pun untuk membuangnya.
    pub fn set_products(&self, ids: &[String]) {
        self.products.clear();
        for (i, id) in ids.iter().enumerate() {
            self.products.insert(id.clone(), i as i32);
        }
    }

    /// Id produk siaran ini, urut sesuai pilihan merchant.
    pub fn product_ids(&self) -> Vec<String> {
        let mut v: Vec<(i32, String)> =
            self.products.iter().map(|e| (*e.value(), e.key().clone())).collect();
        v.sort_by_key(|(i, _)| *i);
        v.into_iter().map(|(_, id)| id).collect()
    }

    /// Jumlah penonton unik (dedupe by user id). Satu user dengan beberapa
    /// koneksi/tab dihitung sekali. O(1): baca panjang peta refcount per-user.
    pub fn viewer_count(&self) -> usize {
        self.unique_viewers.len()
    }

    pub fn add_subscriber(&self, id: &str, info: ViewerInfo) {
        // Pembuat live (merchant) tidak dihitung sebagai penonton.
        if info.id == self.merchant_id {
            return;
        }
        let user_id = info.id.clone();
        // Hanya bump refcount user bila ini koneksi BARU (bukan replace key sama).
        if self.subscribers.insert(id.to_string(), info).is_none() {
            *self.unique_viewers.entry(user_id).or_insert(0) += 1;
        }
    }

    pub fn remove_subscriber(&self, id: &str) {
        if let Some((_, info)) = self.subscribers.remove(id) {
            // Turunkan refcount user; hapus entri saat koneksi terakhir pergi.
            // Drop ref sebelum remove pada map yang sama → hindari deadlock DashMap.
            // `remove_if` menguji DAN menghapus di bawah kunci shard yang sama.
            // Versi sebelumnya melepas kunci (`drop(cnt)`) lalu menghapus tanpa
            // syarat: bila tepat di celah itu tab baru milik user yang sama
            // tersambung dan menaikkan hitungannya jadi 2, entrinya tetap
            // terhapus — penonton yang masih menonton hilang dari hitungan, dan
            // saat ia benar-benar pergi tak ada lagi entri untuk dikurangi.
            if self
                .unique_viewers
                .remove_if(&info.id, |_, cnt| *cnt <= 1)
                .is_none()
            {
                if let Some(mut cnt) = self.unique_viewers.get_mut(&info.id) {
                    *cnt -= 1;
                }
            }
        }
    }

    /// Daftar penonton unik (dedupe by user id).
    pub fn viewers(&self) -> Vec<ViewerInfo> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for e in self.subscribers.iter() {
            if seen.insert(e.value().id.clone()) {
                out.push(e.value().clone());
            }
        }
        out
    }

    pub fn info(&self) -> RoomInfo {
        RoomInfo {
            room_id: self.room_id.clone(),
            merchant_id: self.merchant_id.clone(),
            merchant_name: self.merchant_name.clone(),
            event_slug: self.event_slug.clone(),
            viewer_count: self.viewer_count(),
            started_at: self.started_at.timestamp_millis(),
            viewers: self.viewers(),
            product_ids: self.product_ids(),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn room() -> LiveRoom {
        // Penerima channel dibuang; tak ada perintah SFU yang dikirim di uji ini.
        let (tx, _rx) = mpsc::channel(1);
        LiveRoom::new("r1".into(), "merchant-1".into(), "Toko".into(), None, tx)
    }
    fn penonton(id: &str) -> ViewerInfo {
        ViewerInfo { id: id.into(), name: format!("User {id}"), photo_url: None }
    }

    /// Satu user dengan BEBERAPA tab dihitung SEKALI.
    #[test]
    fn multi_tab_dihitung_satu() {
        let r = room();
        r.add_subscriber("conn-a", penonton("u1"));
        r.add_subscriber("conn-b", penonton("u1"));
        assert_eq!(r.viewer_count(), 1);
        assert_eq!(r.viewers().len(), 1);
    }

    /// Menutup SATU tab tak boleh menghilangkan penonton yang masih menonton di
    /// tab lain. Ini regresi dari `remove_subscriber` yang dulu menghapus entri
    /// refcount tanpa syarat setelah melepas kuncinya.
    #[test]
    fn tutup_satu_tab_penonton_tetap_terhitung() {
        let r = room();
        r.add_subscriber("conn-a", penonton("u1"));
        r.add_subscriber("conn-b", penonton("u1"));
        r.remove_subscriber("conn-a");
        assert_eq!(r.viewer_count(), 1, "masih ada satu tab terbuka");
        r.remove_subscriber("conn-b");
        assert_eq!(r.viewer_count(), 0, "semua tab tertutup");
    }

    /// Merchant pemilik siaran bukan penonton.
    #[test]
    fn pemilik_siaran_tak_dihitung() {
        let r = room();
        r.add_subscriber("conn-owner", penonton("merchant-1"));
        assert_eq!(r.viewer_count(), 0);
    }

    /// Koneksi yang tak dikenal tak boleh membuat hitungan jadi negatif atau
    /// menghapus entri orang lain.
    #[test]
    fn hapus_koneksi_asing_tak_berefek() {
        let r = room();
        r.add_subscriber("conn-a", penonton("u1"));
        r.remove_subscriber("conn-entah");
        assert_eq!(r.viewer_count(), 1);
    }

    /// Beberapa user berbeda dihitung terpisah.
    #[test]
    fn user_berbeda_dihitung_terpisah() {
        let r = room();
        for i in 0..5 {
            r.add_subscriber(&format!("c{i}"), penonton(&format!("u{i}")));
        }
        assert_eq!(r.viewer_count(), 5);
    }
}

// ── Uji ───────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests_penonton {
    use super::*;

    fn ruang() -> LiveRoom {
        // `cmd_tx` tak pernah dipakai jalur yang diuji di sini — seluruhnya
        // pembukuan penonton di dalam peta, tanpa satu pun perintah ke SFU.
        let (tx, _rx) = mpsc::channel(1);
        LiveRoom::new(
            "live_m1".into(),
            "m1".into(),
            "Toko".into(),
            None,
            tx,
        )
    }

    fn orang(id: &str) -> ViewerInfo {
        ViewerInfo {
            id: id.into(),
            name: format!("Orang {id}"),
            photo_url: None,
        }
    }

    /// Merchant menonton siarannya sendiri di tab lain — itu bukan penonton.
    /// Menghitungnya berarti setiap siaran dimulai dari angka satu.
    #[test]
    fn merchant_tak_dihitung_sebagai_penonton() {
        let r = ruang();
        r.add_subscriber("conn-m", orang("m1"));
        assert_eq!(r.viewer_count(), 0);
        assert!(r.viewers().is_empty());
    }

    /// Satu orang membuka DUA tab: dua koneksi, tapi satu penonton.
    #[test]
    fn dua_tab_satu_orang_dihitung_sekali() {
        let r = ruang();
        r.add_subscriber("conn-a", orang("u1"));
        r.add_subscriber("conn-b", orang("u1"));
        // `viewer_count` MENGHITUNG ORANG, bukan koneksi — angka inilah yang
        // dipajang ke penonton lain, dan "12 menonton" yang sebenarnya enam
        // orang dengan dua tab adalah angka yang membohongi.
        assert_eq!(r.viewer_count(), 1);
        assert_eq!(r.viewers().len(), 1);
    }

    /// SKENARIO KONEKSI LEMAH: sambungan putus lalu tersambung lagi.
    ///
    /// Urutan yang sebenarnya terjadi di lapangan BUKAN "putus lalu sambung",
    /// melainkan "sambung lalu putus": koneksi baru sering berhasil sebelum
    /// yang lama sempat menutup. Bila pembukuannya salah menangani celah itu,
    /// orang yang MASIH menonton lenyap dari hitungan — dan saat ia benar-benar
    /// pergi, tak ada lagi entri untuk dikurangi, sehingga hitungannya rusak
    /// sampai siaran berakhir.
    #[test]
    fn sambung_dulu_baru_putus_tak_menghilangkan_penonton() {
        let r = ruang();
        r.add_subscriber("conn-lama", orang("u1"));
        r.add_subscriber("conn-baru", orang("u1")); // tab baru masuk
        r.remove_subscriber("conn-lama"); // tab lama baru menutup

        assert_eq!(r.viewers().len(), 1, "ia masih menonton");
        assert_eq!(r.viewer_count(), 1);
    }

    /// Dan setelah koneksi terakhirnya benar-benar pergi, ia hilang dari daftar.
    #[test]
    fn koneksi_terakhir_pergi_menghapus_penonton() {
        let r = ruang();
        r.add_subscriber("conn-a", orang("u1"));
        r.add_subscriber("conn-b", orang("u1"));
        r.remove_subscriber("conn-a");
        r.remove_subscriber("conn-b");
        assert!(r.viewers().is_empty());
        assert_eq!(r.viewer_count(), 0);
    }

    /// Jaringan yang buruk membuat pesan "keluar" terkirim dua kali.
    #[test]
    fn keluar_dua_kali_tak_membuat_hitungan_minus() {
        let r = ruang();
        r.add_subscriber("conn-a", orang("u1"));
        r.remove_subscriber("conn-a");
        r.remove_subscriber("conn-a");
        assert_eq!(r.viewer_count(), 0);
        assert!(r.viewers().is_empty());

        // Dan ia masih bisa kembali dengan hitungan yang benar.
        r.add_subscriber("conn-c", orang("u1"));
        assert_eq!(r.viewers().len(), 1);
    }

    #[test]
    fn keluar_dari_koneksi_tak_dikenal_aman() {
        let r = ruang();
        r.add_subscriber("conn-a", orang("u1"));
        r.remove_subscriber("conn-entah");
        assert_eq!(r.viewer_count(), 1);
    }

    /// Id koneksi yang sama dipakai ulang (mis. klien menyambung ulang dengan
    /// id yang sama) tak boleh menggandakan hitungan uniknya.
    #[test]
    fn id_koneksi_dipakai_ulang_tak_menggandakan() {
        let r = ruang();
        r.add_subscriber("conn-a", orang("u1"));
        r.add_subscriber("conn-a", orang("u1"));
        assert_eq!(r.viewer_count(), 1);
        assert_eq!(r.viewers().len(), 1);
        r.remove_subscriber("conn-a");
        assert!(r.viewers().is_empty());
    }

    #[test]
    fn banyak_penonton_terhitung_terpisah() {
        let r = ruang();
        for i in 0..5 {
            r.add_subscriber(&format!("c{i}"), orang(&format!("u{i}")));
        }
        assert_eq!(r.viewers().len(), 5);
        assert_eq!(r.info().viewer_count, 5);
    }

    // ── Keranjang kuning ──────────────────────────────────────────────────

    /// Mengganti, bukan menggabung: merchant mengirim daftar utuh dari layarnya,
    /// jadi produk yang ia cabut harus benar-benar hilang dari keranjang
    /// penonton — menggabung membuatnya menetap tanpa cara membuangnya.
    #[test]
    fn daftar_produk_diganti_bukan_ditambah() {
        let r = ruang();
        r.set_products(&["p1".into(), "p2".into()]);
        r.set_products(&["p3".into()]);
        assert_eq!(r.product_ids(), vec!["p3".to_string()]);
    }

    #[test]
    fn daftar_produk_dikosongkan() {
        let r = ruang();
        r.set_products(&["p1".into()]);
        r.set_products(&[]);
        assert!(r.product_ids().is_empty());
        assert!(r.info().product_ids.is_empty());
    }
}
