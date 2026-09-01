//! waktu.rs — Satu tempat untuk mengubah waktu jadi tulisan yang dibaca orang.
//!
//! ── KENAPA DISATUKAN ──────────────────────────────────────────────────────
//! Sebelum berkas ini, aplikasi punya LIMA pemformat waktu di lima halaman —
//! dua di antaranya byte-identik — dan zona waktu Jakarta ditulis tangan dengan
//! DUA teknik berbeda:
//!
//!     ticket_detail.rs  `FixedOffset::east_opt(7 * 3600)`   ← benar
//!     chat_room.rs      `ms / 1000 + 7 * 3600`              ← kebetulan benar
//!     notification_detail.rs  potong string RFC3339          ← SALAH, tampil UTC
//!
//! Yang ketiga itu bukan pengulangan yang cuma boros; ia keliru tanpa pernah
//! gagal — memotong "2026-09-01T04:03:26Z" jadi "2026-09-01 04:03" menghasilkan
//! kalimat yang terbaca sempurna dan meleset tujuh jam. Kekeliruan seperti itu
//! tak akan pernah ditemukan lewat membaca kodenya; ia hanya bisa dicegah
//! dengan tidak menyediakan tempat untuk menuliskannya lagi.
//!
//! Aritmetika manual juga tak tahu apa pun tentang zona waktu. Ia hanya
//! menambah tujuh jam, dan itu kebetulan benar untuk WIB. Semua di sini lewat
//! `FixedOffset` supaya yang dinyatakan adalah maksudnya, bukan angkanya.

use chrono::{DateTime, FixedOffset, Utc};

/// WIB — Waktu Indonesia Barat, UTC+7. Tetap sepanjang tahun; Indonesia tak
/// mengenal waktu musim panas, jadi offset tetap memang jawaban yang benar di
/// sini, bukan penyederhanaan.
const OFFSET_WIB: i32 = 7 * 3600;

/// Ubah ke zona WIB. `unwrap` aman: 7×3600 tetap di dalam batas ±26 jam yang
/// diterima `east_opt`, dan itu konstanta — tak ada masukan yang bisa
/// mengubahnya.
pub fn wib(dt: &DateTime<Utc>) -> DateTime<FixedOffset> {
    dt.with_timezone(&FixedOffset::east_opt(OFFSET_WIB).expect("offset WIB tetap"))
}

/// Nama bulan Indonesia. `chrono` tak punya lokal tanpa crate tambahan, dan
/// `%b` menghasilkan "Aug"/"Dec" — bahasa Inggris di tengah antarmuka yang
/// seluruhnya berbahasa Indonesia.
const BULAN: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "Mei", "Jun", "Jul", "Agu", "Sep", "Okt", "Nov", "Des",
];

/// `1 Sep 2026`
pub fn tanggal(dt: &DateTime<Utc>) -> String {
    use chrono::Datelike;
    let d = wib(dt);
    format!("{} {} {}", d.day(), BULAN[(d.month() as usize) - 1], d.year())
}

/// `11:03`
pub fn jam(dt: &DateTime<Utc>) -> String {
    wib(dt).format("%H:%M").to_string()
}

/// `11:03 WIB` — dipakai saat waktunya berdiri sendiri tanpa keterangan lain,
/// sehingga zonanya perlu disebutkan.
pub fn jam_berzona(dt: &DateTime<Utc>) -> String {
    wib(dt).format("%H:%M WIB").to_string()
}

/// `1 Sep 2026, 11:03 WIB`
pub fn tanggal_jam(dt: &DateTime<Utc>) -> String {
    format!("{}, {}", tanggal(dt), jam_berzona(dt))
}

/// `11:03` dari unix millis — bentuk yang dipakai WebSocket obrolan.
///
/// Millis yang tak masuk akal menghasilkan string KOSONG, bukan panik dan bukan
/// tanggal 1970. Nol adalah nilai yang sungguhan lewat di sini: pesan optimistis
/// dibuat dengan `sent_at: 0` sebelum server menyahut, dan menampilkan
/// "07:00" di sebelah pesan yang baru saja diketik akan tampak seperti kerusakan.
pub fn jam_dari_millis(ms: u64) -> String {
    if ms == 0 {
        return String::new();
    }
    // `try_from`, bukan `as`. Cast `u64 → i64` MEMBUNGKUS diam-diam: `u64::MAX`
    // menjadi −1, yang merupakan cap waktu yang sah (akhir 1969) — jadi
    // `from_timestamp_millis` berhasil dan mengembalikan jam yang tampak wajar
    // untuk masukan yang sama sekali tak masuk akal. Ujinya yang menemukan ini.
    let Ok(ms) = i64::try_from(ms) else {
        return String::new();
    };
    match DateTime::<Utc>::from_timestamp_millis(ms) {
        Some(dt) => jam(&dt),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn menggeser_ke_wib() {
        assert_eq!(jam(&utc("2026-09-01T04:03:26Z")), "11:03");
    }

    /// Kasus yang membuat pemformat lama salah tanpa terlihat salah: sesudah
    /// pukul 17.00 UTC, TANGGAL di Jakarta sudah berganti. Pemformat yang
    /// melewatkan konversi zona akan menampilkan tanggal kemarin sepanjang tujuh
    /// jam setiap hari — dan tetap terbaca masuk akal.
    #[test]
    fn melewati_tengah_malam_tanggalnya_ikut_maju() {
        let malam = utc("2026-08-31T17:30:00Z");
        assert_eq!(tanggal(&malam), "1 Sep 2026");
        assert_eq!(jam(&malam), "00:30");
    }

    #[test]
    fn sebelum_pukul_tujuh_belas_tanggalnya_tetap() {
        assert_eq!(tanggal(&utc("2026-08-31T16:59:00Z")), "31 Agu 2026");
    }

    #[test]
    fn millis_nol_tak_menghasilkan_waktu_palsu() {
        // Pesan optimistis memakai 0. "07:00" di sebelahnya akan tampak rusak.
        assert_eq!(jam_dari_millis(0), "");
    }

    #[test]
    fn millis_diubah_sama_dengan_jalur_datetime() {
        let dt = utc("2026-09-01T04:03:26Z");
        assert_eq!(jam_dari_millis(dt.timestamp_millis() as u64), jam(&dt));
    }

    #[test]
    fn millis_mustahil_tak_memanikkan() {
        assert_eq!(jam_dari_millis(u64::MAX), "");
    }

    /// `%b` milik chrono akan menghasilkan "Aug"/"Dec" di sini — bahasa Inggris
    /// di tengah antarmuka yang seluruhnya berbahasa Indonesia.
    #[test]
    fn bulan_berbahasa_indonesia() {
        assert_eq!(tanggal(&utc("2026-08-15T03:00:00Z")), "15 Agu 2026");
        assert_eq!(tanggal(&utc("2026-12-15T03:00:00Z")), "15 Des 2026");
        assert_eq!(tanggal(&utc("2026-05-15T03:00:00Z")), "15 Mei 2026");
    }

    #[test]
    fn berzona_menyebut_wib() {
        assert_eq!(jam_berzona(&utc("2026-09-01T04:03:26Z")), "11:03 WIB");
    }

    #[test]
    fn tanggal_jam_utuh() {
        assert_eq!(
            tanggal_jam(&utc("2026-09-01T04:03:26Z")),
            "1 Sep 2026, 11:03 WIB"
        );
    }
}
