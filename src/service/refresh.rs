//! service/refresh.rs — daur hidup refresh token.
//!
//! ── KENAPA TOKENNYA OPAQUE, BUKAN JWT ──────────────────────────────────────
//! Refresh token di sini adalah 32 byte acak, bukan JWT. Itu bukan sekadar
//! selera bentuk: karena access token adalah JWT dan refresh token bukan,
//! memakai access token sebagai refresh token menjadi MUSTAHIL SECARA STRUKTUR.
//! Versi sebelumnya mengembalikan token yang sama persis di kedua field, dan
//! endpoint refresh menerima apa pun yang lolos verifikasi JWT — artinya satu
//! access token yang bocor bisa terus ditukar menjadi token baru selamanya.
//!
//! Membedakannya lewat klaim (`token_type: "refresh"`) juga bisa, tetapi itu
//! bergantung pada satu pemeriksaan yang harus diingat penulisnya. Bentuk yang
//! berbeda tidak bisa lupa diperiksa.
//!
//! ── ROTASI DAN DETEKSI PEMAKAIAN ULANG ─────────────────────────────────────
//! Setiap refresh mencabut token lama dan menerbitkan yang baru dalam keluarga
//! (`family_id`) yang sama. Kalau token yang SUDAH dicabut muncul lagi, itu
//! tanda salinannya ada di dua tangan — pemilik sah dan pencuri. Yang dilakukan:
//! cabut SELURUH keluarga. Keduanya harus login ulang.
//!
//! Itu memang merepotkan pemilik sah, dan itu disengaja. Alternatifnya
//! membiarkan pencuri memegang rantai token yang bisa diperpanjang tanpa batas,
//! diam-diam, selama pemilik sah tak pernah sadar.

use std::sync::Arc;

use chrono::{Duration, Utc};
use sha2::{Digest, Sha256};

use crate::models::users::{User, UserResponse};
use crate::repository::refresh_token::RefreshTokenRepository;
use crate::repository::user::UserRepository;
use crate::utils::error::{AppError, AppResult};
use crate::utils::jwt::JwtService;
use crate::utils::ulid::new_ulid;

/// Masa berlaku refresh token. Jauh lebih panjang dari access token — itulah
/// gunanya: pengguna tetap masuk berhari-hari tanpa access token yang berumur
/// panjang berkeliaran.
const REFRESH_TTL_DAYS: i64 = 30;

/// Jendela toleransi ROTASI BERSAMAAN.
///
/// Satu halaman menembakkan banyak permintaan sekaligus (server function
/// `/api-fn`, `/api/*`, aset). Bila access token kebetulan mati saat itu,
/// SEMUANYA membawa refresh token yang sama dan semuanya memicu rotasi. Satu
/// menang; sisanya tiba beberapa milidetik kemudian dan menemukan baris yang
/// baru saja dicabut.
///
/// Tanpa jendela ini, keadaan yang sepenuhnya normal itu tak bisa dibedakan
/// dari token curian, dan penanganannya — mencabut seluruh keluarga — MEMBUANG
/// sesi pengguna yang sah. Itulah yang terlihat di log produksi sebagai lima
/// baris "DIPAKAI ULANG" dalam rentang 30 milidetik, diikuti pengguna yang
/// tiba-tiba "Tidak terautentikasi" padahal baru saja masuk.
///
/// Pencurian sungguhan tetap tertangkap: token yang dicabut dan muncul lagi
/// SESUDAH jendela ini lewat tetap mencabut seluruh keluarga.
const GRACE_ROTASI: i64 = 30;

pub struct RefreshResult {
    pub access_token: String,
    /// KOSONG artinya "jangan pasang cookie refresh baru" — dipakai pada jalur
    /// rotasi-bersamaan, di mana pemenanglah yang sudah menerbitkan token baru
    /// dan yang kalah tak boleh menerbitkan token kedua.
    pub refresh_token: String,
    pub expires_in: i64,
    pub user: UserResponse,
}

pub struct RefreshService {
    repo: Arc<dyn RefreshTokenRepository>,
    users: Arc<dyn UserRepository>,
    jwt: JwtService,
}

impl RefreshService {
    pub fn new(
        repo: Arc<dyn RefreshTokenRepository>,
        users: Arc<dyn UserRepository>,
        jwt: JwtService,
    ) -> Self {
        Self { repo, users, jwt }
    }

    /// SHA-256 heksadesimal. Yang tersimpan di database adalah hasil ini,
    /// bukan tokennya.
    fn hash(token: &str) -> String {
        let mut h = Sha256::new();
        h.update(token.as_bytes());
        hex::encode(h.finalize())
    }

    /// 32 byte acak dari sumber acak sistem operasi.
    fn generate_token() -> String {
        use rand::RngExt;
        let mut rng = rand::rng();
        let mut buf = [0u8; 32];
        for b in buf.iter_mut() {
            *b = rng.random();
        }
        hex::encode(buf)
    }

    /// Terbitkan refresh token baru. `family` kosong berarti login baru
    /// (keluarga baru), terisi berarti kelanjutan rotasi.
    pub async fn issue(
        &self,
        user_id: &str,
        family: Option<&str>,
        user_agent: &str,
    ) -> AppResult<String> {
        let token = Self::generate_token();
        let id = new_ulid();
        let family_id = family.map(str::to_string).unwrap_or_else(new_ulid);
        let expires_at = Utc::now() + Duration::days(REFRESH_TTL_DAYS);

        // `user_agent` dipotong agar cocok dengan kolomnya. Panjangnya
        // dikendalikan klien, jadi tak boleh dipercaya apa adanya.
        let ua: String = user_agent.chars().take(255).collect();

        self.repo
            .insert(&id, user_id, &Self::hash(&token), &family_id, expires_at, &ua)
            .await
            .map_err(AppError::Internal)?;

        Ok(token)
    }

    /// Terbitkan HANYA access token untuk `user_id`, tanpa merotasi refresh.
    ///
    /// Dipakai pada dua jalur rotasi-bersamaan. Aman secara keamanan: refresh
    /// token yang dibawa peminta memang sah beberapa milidetik lalu, keluarganya
    /// utuh, dan tak ada token refresh kedua yang diterbitkan — hanya access
    /// token berumur pendek, dengan peran yang dibaca ULANG dari database
    /// seperti pada rotasi biasa.
    async fn hanya_access(&self, user_id: &str) -> AppResult<RefreshResult> {
        let user: User = self
            .users
            .find_by_id(user_id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::Unauthorized("Akun tidak ditemukan".into()))?;

        let access_token = self
            .jwt
            .sign(&user.id, &user.name, &user.phone, &user.role.to_string())
            .map_err(AppError::Internal)?;

        Ok(RefreshResult {
            access_token,
            // Kosong = penanda "jangan sentuh cookie refresh".
            refresh_token: String::new(),
            expires_in: self.jwt.expires_in_secs(),
            user: UserResponse {
                id: user.id,
                email: user.email,
                name: user.name,
                phone: user.phone,
                role: user.role.to_string(),
                created_at: user.created_at,
            },
        })
    }

    /// Tukar refresh token dengan sepasang token baru.
    pub async fn rotate(&self, presented: &str, user_agent: &str) -> AppResult<RefreshResult> {
        let hash = Self::hash(presented);

        let row = self
            .repo
            .find_by_hash(&hash)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::Unauthorized("Refresh token tidak valid".into()))?;

        // ── Pemakaian ulang ─────────────────────────────────────────────────
        // Token yang sudah dicabut muncul lagi. Rantai ini tak bisa dipercaya
        // lagi seluruhnya, jadi seluruh keluarganya dicabut.
        if row.is_revoked() {
            // Dicabut BARU SAJA = hampir pasti permintaan saudara dari halaman
            // yang sama, bukan penyerang. Layani ia dengan access token baru,
            // tanpa merotasi ulang dan tanpa menyentuh keluarganya.
            let baru_saja = row
                .revoked_at
                .map(|t| Utc::now().signed_duration_since(t) < Duration::seconds(GRACE_ROTASI))
                .unwrap_or(false);
            if baru_saja {
                tracing::debug!(
                    user_id = %row.user_id,
                    family_id = %row.family_id,
                    "rotasi bersamaan — access token diterbitkan tanpa rotasi ulang"
                );
                return self.hanya_access(&row.user_id).await;
            }

            let n = self
                .repo
                .revoke_family(&row.family_id)
                .await
                .map_err(AppError::Internal)?;
            tracing::warn!(
                user_id = %row.user_id,
                family_id = %row.family_id,
                dicabut = n,
                "refresh token DIPAKAI ULANG — seluruh keluarga dicabut"
            );
            return Err(AppError::Unauthorized(
                "Sesi tidak lagi valid, silakan masuk kembali".into(),
            ));
        }

        if row.is_expired() {
            return Err(AppError::Unauthorized(
                "Sesi sudah kedaluwarsa, silakan masuk kembali".into(),
            ));
        }

        // ── Peran diambil ULANG dari database ───────────────────────────────
        // Inilah yang membatasi kerusakan dari peran basi di JWT: sepanjang
        // access token berumur pendek, pencabutan hak akses paling lama
        // tertinggal selama sisa umur access token, bukan sampai token
        // berikutnya kedaluwarsa berhari-hari kemudian.
        let user: User = self
            .users
            .find_by_id(&row.user_id)
            .await
            .map_err(AppError::Internal)?
            .ok_or_else(|| AppError::Unauthorized("Akun tidak ditemukan".into()))?;

        let new_token = Self::generate_token();
        let new_id = new_ulid();
        let expires_at = Utc::now() + Duration::days(REFRESH_TTL_DAYS);
        let ua: String = user_agent.chars().take(255).collect();

        // Cabut yang lama LEBIH DULU. Kalau dua permintaan datang bersamaan
        // dengan token yang sama, hanya satu yang berhasil mencabut — yang
        // kalah berhenti di sini alih-alih ikut menerbitkan token kedua.
        let menang = self
            .repo
            .revoke(&row.id, Some(&new_id))
            .await
            .map_err(AppError::Internal)?;

        // Kalah balapan: peminta lain mencabut baris ini lebih dulu, di antara
        // `find_by_hash` dan `revoke` di atas. Sama persis dengan kasus di atas
        // — permintaan saudara, bukan penyerang.
        if !menang {
            tracing::debug!(
                user_id = %row.user_id,
                "kalah balapan rotasi — access token diterbitkan tanpa rotasi ulang"
            );
            return self.hanya_access(&row.user_id).await;
        }

        self.repo
            .insert(
                &new_id,
                &row.user_id,
                &Self::hash(&new_token),
                &row.family_id,
                expires_at,
                &ua,
            )
            .await
            .map_err(AppError::Internal)?;

        // Urutan argumen: (user_id, name, phone, role) — lihat utils/jwt.rs.
        let access_token = self
            .jwt
            .sign(&user.id, &user.name, &user.phone, &user.role.to_string())
            .map_err(AppError::Internal)?;

        Ok(RefreshResult {
            access_token,
            refresh_token: new_token,
            expires_in: self.jwt.expires_in_secs(),
            user: UserResponse {
                id: user.id,
                email: user.email,
                name: user.name,
                phone: user.phone,
                role: user.role.to_string(),
                created_at: user.created_at,
            },
        })
    }

    /// Logout: cabut seluruh keluarga token yang ditunjukkan.
    ///
    /// Token yang tak dikenal tidak dianggap error — logout harus selalu
    /// berhasil dari sudut pandang pengguna, dan membedakan "token tak dikenal"
    /// dari "token dicabut" hanya memberi penyerang cara menguji token.
    pub async fn revoke(&self, presented: &str) -> AppResult<()> {
        let hash = Self::hash(presented);
        if let Some(row) = self
            .repo
            .find_by_hash(&hash)
            .await
            .map_err(AppError::Internal)?
        {
            self.repo
                .revoke_family(&row.family_id)
                .await
                .map_err(AppError::Internal)?;
        }
        Ok(())
    }

    /// Cabut SEMUA sesi milik satu user. Dipakai saat ganti kata sandi atau
    /// pencabutan hak akses.
    pub async fn revoke_all(&self, user_id: &str) -> AppResult<u64> {
        self.repo
            .revoke_all_for_user(user_id)
            .await
            .map_err(AppError::Internal)
    }

    pub async fn cleanup_expired(&self) -> AppResult<u64> {
        self.repo.delete_expired().await.map_err(AppError::Internal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hash harus deterministik (supaya bisa dicari lewat index) dan berbeda
    /// untuk token berbeda. Kalau tidak, seluruh pencarian refresh gagal.
    #[test]
    fn hash_deterministik() {
        let a = "abc123";
        assert_eq!(RefreshService::hash(a), RefreshService::hash(a));
        assert_ne!(RefreshService::hash(a), RefreshService::hash("abc124"));
        assert_eq!(RefreshService::hash(a).len(), 64);
    }

    /// Token yang diterbitkan tak boleh berulang. Dua token identik berarti
    /// dua sesi berbagi satu baris database — dan pencabutan salah satunya
    /// akan mencabut keduanya.
    #[test]
    fn token_acak_dan_panjang() {
        let a = RefreshService::generate_token();
        let b = RefreshService::generate_token();
        assert_eq!(a.len(), 64, "32 byte dalam heksadesimal");
        assert_ne!(a, b);
    }

    /// Token yang ditandai dicabut harus terbaca sebagai dicabut, dan yang
    /// kedaluwarsa terbaca kedaluwarsa — dua gerbang yang menjaga `rotate`.
    #[test]
    fn status_baris_terbaca_benar() {
        use crate::repository::refresh_token::RefreshTokenRow;
        let hidup = RefreshTokenRow {
            id: new_ulid(),
            user_id: new_ulid(),
            family_id: new_ulid(),
            expires_at: Utc::now() + Duration::days(1),
            revoked_at: None,
        };
        assert!(!hidup.is_revoked());
        assert!(!hidup.is_expired());

        let mati = RefreshTokenRow {
            expires_at: Utc::now() - Duration::seconds(1),
            revoked_at: Some(Utc::now()),
            ..hidup.clone()
        };
        assert!(mati.is_revoked());
        assert!(mati.is_expired());
    }
}

// ─── Uji rotasi refresh token ─────────────────────────────────────────────────
//
// Insiden yang melahirkan uji ini: log produksi memuat LIMA baris "refresh
// token DIPAKAI ULANG — seluruh keluarga dicabut" dalam rentang 30 milidetik,
// untuk satu pengguna yang baru saja masuk dengan wajar. Tak ada penyerang.
//
// Satu muat halaman menembakkan banyak permintaan sekaligus. Bila access token
// kebetulan mati saat itu, SEMUANYA membawa refresh token yang sama dan
// semuanya memicu rotasi: satu menang, sisanya tiba beberapa milidetik kemudian
// dan menemukan baris yang baru saja dicabut. Penanganan lama tak bisa
// membedakan itu dari token curian, dan tindakannya — mencabut seluruh keluarga
// — MEMBUANG sesi pengguna yang sah. Gejalanya di layar: "Tidak terautentikasi"
// di halaman yang jelas-jelas menampilkan nama pengguna, dan "GO LIVE" gagal.
//
// Yang dijaga uji-uji di bawah adalah GARIS PEMISAHNYA: bersamaan dilayani,
// pencurian sungguhan tetap dihukum.
#[cfg(test)]
mod tests_rotasi {
    use super::*;
    use crate::models::users::{RegisterRequest, UserRole};
    use crate::repository::refresh_token::RefreshTokenRow;
    use crate::repository::user::UserWithPassword;
    use async_trait::async_trait;
    use chrono::DateTime;
    use std::sync::Mutex;

    // ── Repo palsu: hanya secukupnya untuk menempuh jalur `rotate` ───────────

    #[derive(Default)]
    struct RepoRefreshPalsu {
        /// token_hash → baris.
        baris: Mutex<Vec<(String, RefreshTokenRow)>>,
    }

    impl RepoRefreshPalsu {
        fn seed(&self, hash: &str, id: &str, family: &str, revoked_at: Option<DateTime<Utc>>) {
            self.baris.lock().unwrap().push((
                hash.to_string(),
                RefreshTokenRow {
                    id: id.into(),
                    user_id: "user-1".into(),
                    family_id: family.into(),
                    expires_at: Utc::now() + Duration::days(30),
                    revoked_at,
                },
            ));
        }
        fn jumlah_dicabut(&self) -> usize {
            self.baris.lock().unwrap().iter().filter(|(_, r)| r.is_revoked()).count()
        }
    }

    #[async_trait]
    impl RefreshTokenRepository for RepoRefreshPalsu {
        async fn insert(
            &self,
            id: &str,
            _user_id: &str,
            token_hash: &str,
            family_id: &str,
            expires_at: DateTime<Utc>,
            _user_agent: &str,
        ) -> anyhow::Result<()> {
            self.baris.lock().unwrap().push((
                token_hash.to_string(),
                RefreshTokenRow {
                    id: id.into(),
                    user_id: "user-1".into(),
                    family_id: family_id.into(),
                    expires_at,
                    revoked_at: None,
                },
            ));
            Ok(())
        }

        async fn find_by_hash(&self, token_hash: &str) -> anyhow::Result<Option<RefreshTokenRow>> {
            Ok(self
                .baris
                .lock()
                .unwrap()
                .iter()
                .find(|(h, _)| h == token_hash)
                .map(|(_, r)| r.clone()))
        }

        /// Meniru `UPDATE ... WHERE id = $1 AND revoked_at IS NULL`: hanya satu
        /// pemanggil yang bisa menang. Semantik itulah yang membuat balapan
        /// rotasi punya pemenang tunggal, jadi tiruannya harus setia.
        async fn revoke(&self, id: &str, _replaced_by: Option<&str>) -> anyhow::Result<bool> {
            let mut b = self.baris.lock().unwrap();
            for (_, r) in b.iter_mut() {
                if r.id == id && r.revoked_at.is_none() {
                    r.revoked_at = Some(Utc::now());
                    return Ok(true);
                }
            }
            Ok(false)
        }

        async fn revoke_family(&self, family_id: &str) -> anyhow::Result<u64> {
            let mut b = self.baris.lock().unwrap();
            let mut n = 0;
            for (_, r) in b.iter_mut() {
                if r.family_id == family_id && r.revoked_at.is_none() {
                    r.revoked_at = Some(Utc::now());
                    n += 1;
                }
            }
            Ok(n)
        }

        async fn revoke_all_for_user(&self, _user_id: &str) -> anyhow::Result<u64> {
            Ok(0)
        }
        async fn delete_expired(&self) -> anyhow::Result<u64> {
            Ok(0)
        }
    }

    struct RepoUserPalsu;

    #[async_trait]
    impl UserRepository for RepoUserPalsu {
        async fn create(
            &self,
            _req: &RegisterRequest,
            _password_hash: Option<&str>,
            _role: UserRole,
        ) -> anyhow::Result<User> {
            unimplemented!("tak dipakai jalur rotate")
        }
        async fn find_by_id(&self, id: &str) -> anyhow::Result<Option<User>> {
            Ok(Some(User {
                id: id.into(),
                email: None,
                name: "Penjual".into(),
                phone: "0800".into(),
                role: UserRole::Merchant,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            }))
        }
        async fn find_by_email_with_password(
            &self,
            _email: &str,
        ) -> anyhow::Result<Option<UserWithPassword>> {
            Ok(None)
        }
        async fn update_profile(
            &self,
            _id: &str,
            _name: Option<&str>,
            _phone: Option<&str>,
        ) -> anyhow::Result<()> {
            Ok(())
        }
        async fn update_password_hash(&self, _id: &str, _new_hash: &str) -> anyhow::Result<()> {
            Ok(())
        }
        async fn find_by_phone(&self, _phone: &str) -> anyhow::Result<Option<User>> {
            Ok(None)
        }
        async fn find_by_phone_with_password(
            &self,
            _email: &str,
        ) -> anyhow::Result<Option<UserWithPassword>> {
            Ok(None)
        }
    }

    fn layanan(repo: Arc<RepoRefreshPalsu>) -> RefreshService {
        RefreshService::new(
            repo,
            Arc::new(RepoUserPalsu),
            JwtService::new("rahasia-uji-yang-cukup-panjang", 1),
        )
    }

    /// Rotasi biasa: token sah ditukar sepasang token baru, yang lama dicabut.
    #[tokio::test]
    async fn rotasi_normal_menerbitkan_refresh_baru() {
        let repo = Arc::new(RepoRefreshPalsu::default());
        repo.seed(&RefreshService::hash("token-a"), "id-a", "fam-1", None);
        let svc = layanan(repo.clone());

        let hasil = svc.rotate("token-a", "uji").await.expect("rotasi harus berhasil");
        assert!(!hasil.access_token.is_empty());
        assert!(
            !hasil.refresh_token.is_empty(),
            "rotasi normal WAJIB menerbitkan refresh token baru"
        );
        assert_eq!(repo.jumlah_dicabut(), 1, "hanya baris lama yang dicabut");
    }

    /// INTI PERBAIKAN: token yang baru saja dirotasi dipakai lagi oleh
    /// permintaan saudara. Harus dilayani, dan keluarganya harus UTUH.
    #[tokio::test]
    async fn rotasi_bersamaan_dilayani_tanpa_mencabut_keluarga() {
        let repo = Arc::new(RepoRefreshPalsu::default());
        repo.seed(&RefreshService::hash("token-a"), "id-a", "fam-1", None);
        let svc = layanan(repo.clone());

        // Permintaan pertama menang.
        let pertama = svc.rotate("token-a", "uji").await.unwrap();
        assert!(!pertama.refresh_token.is_empty());

        // Permintaan saudara tiba beberapa milidetik kemudian dengan token yang
        // SAMA — persis lima baris yang terlihat di log produksi.
        let kedua = svc
            .rotate("token-a", "uji")
            .await
            .expect("permintaan bersamaan TIDAK boleh gagal");

        assert!(
            !kedua.access_token.is_empty(),
            "harus tetap dapat access token supaya permintaannya tak jadi 401"
        );
        assert!(
            kedua.refresh_token.is_empty(),
            "TIDAK boleh menerbitkan refresh token kedua — penandanya string kosong"
        );

        // Yang paling penting: sesi pengguna masih hidup.
        let dicabut = repo.jumlah_dicabut();
        assert_eq!(
            dicabut, 1,
            "hanya baris lama yang boleh dicabut; keluarga TIDAK boleh ikut dicabut"
        );
    }

    /// Banyak permintaan saudara sekaligus — tak satu pun boleh mencabut sesi.
    #[tokio::test]
    async fn lima_permintaan_saudara_tak_membunuh_sesi() {
        let repo = Arc::new(RepoRefreshPalsu::default());
        repo.seed(&RefreshService::hash("token-a"), "id-a", "fam-1", None);
        let svc = layanan(repo.clone());

        let mut berhasil = 0;
        let mut refresh_baru = 0;
        for _ in 0..5 {
            let h = svc.rotate("token-a", "uji").await.expect("tak boleh gagal");
            berhasil += 1;
            if !h.refresh_token.is_empty() {
                refresh_baru += 1;
            }
        }
        assert_eq!(berhasil, 5, "kelimanya dilayani");
        assert_eq!(refresh_baru, 1, "hanya SATU yang boleh merotasi");
        assert_eq!(repo.jumlah_dicabut(), 1, "sesi tetap hidup");
    }

    /// Pencurian sungguhan tetap dihukum: token yang dicabut LAMA muncul lagi
    /// setelah jendela toleransi lewat → seluruh keluarga dicabut.
    #[tokio::test]
    async fn token_dicabut_lama_tetap_mencabut_keluarga() {
        let repo = Arc::new(RepoRefreshPalsu::default());
        let lama = Utc::now() - Duration::seconds(GRACE_ROTASI + 5);
        repo.seed(&RefreshService::hash("token-curian"), "id-a", "fam-1", Some(lama));
        // Token hidup lain di keluarga yang sama — inilah yang harus ikut mati.
        repo.seed(&RefreshService::hash("token-b"), "id-b", "fam-1", None);
        let svc = layanan(repo.clone());

        let hasil = svc.rotate("token-curian", "penyerang").await;
        assert!(hasil.is_err(), "token curian harus ditolak");
        assert_eq!(
            repo.jumlah_dicabut(),
            2,
            "seluruh keluarga dicabut, termasuk token yang masih hidup"
        );
    }

    /// Token yang tak dikenal ditolak tanpa menyentuh keluarga mana pun.
    #[tokio::test]
    async fn token_asing_ditolak() {
        let repo = Arc::new(RepoRefreshPalsu::default());
        repo.seed(&RefreshService::hash("token-a"), "id-a", "fam-1", None);
        let svc = layanan(repo.clone());

        assert!(svc.rotate("token-entah", "uji").await.is_err());
        assert_eq!(repo.jumlah_dicabut(), 0, "tak ada yang boleh dicabut");
    }
}
