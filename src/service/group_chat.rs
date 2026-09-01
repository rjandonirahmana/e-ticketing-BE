use anyhow::{bail, Result};
use std::sync::Arc;

use crate::{
    models::group_chat::{GroupMessage, GroupRoom, MsgType, TicketCard},
    repository::group_chat::GroupChatRepository,
    utils::ulid::new_ulid,
    ws::manager::WsManager,
    ws::proto::{WsEvent, WsMessage},
};

pub struct GroupChatService {
    /// FIX: Arc<dyn GroupChatRepository> — konsisten dengan OrderService, AuthService, dll.
    /// Memungkinkan mock/test tanpa database, dan swap implementasi tanpa recompile service.
    pub repo: Arc<dyn GroupChatRepository>,
    pub ws_mgr: Arc<WsManager>,
}

impl GroupChatService {
    pub fn new(repo: Arc<dyn GroupChatRepository>, ws_mgr: Arc<WsManager>) -> Self {
        Self { repo, ws_mgr }
    }

    // ── Retensi ──────────────────────────────────────────────────────────────

    /// Buang pesan yang sudah lewat masa simpan, berikut berkasnya di RustFS.
    ///
    /// Mengembalikan `(pesan_dihapus, berkas_dihapus)`.
    ///
    /// ── URUTANNYA TIDAK BOLEH DIBALIK ────────────────────────────────────
    /// Berkas dibuang LEBIH DULU, barisnya menyusul. Sebaliknya — baris dulu —
    /// berarti kehilangan satu-satunya alamat berkasnya begitu barisnya hilang,
    /// dan yang tertinggal adalah berkas yatim di penyimpanan yang tak ada lagi
    /// cara menemukannya. Kalaupun proses ini mati di antara keduanya, yang
    /// terjadi hanya berkas terhapus sementara barisnya masih ada: jalanan
    /// berikutnya akan menemukan baris itu lagi, gagal menghapus berkas yang
    /// memang sudah tiada (dicatat, tidak fatal), lalu membuang barisnya.
    /// Arah kegagalan yang bisa pulih sendiri.
    pub async fn buang_kadaluarsa(
        &self,
        hari: i64,
        batas_angkatan: i64,
        storage: &crate::service::storage::StorageService,
    ) -> Result<(u64, u64)> {
        let mut total_pesan = 0u64;
        let mut total_berkas = 0u64;

        loop {
            for url in self.repo.media_kadaluarsa(hari, batas_angkatan).await? {
                match storage.delete_by_url(&url).await {
                    Ok(_) => total_berkas += 1,
                    // Berkas yang memang sudah tidak ada bukan alasan untuk
                    // membatalkan pembersihan — barisnya tetap harus pergi.
                    Err(e) => tracing::warn!(url = %url, galat = ?e, "berkas chat gagal dihapus"),
                }
            }

            let n = self.repo.hapus_kadaluarsa(hari, batas_angkatan).await?;
            total_pesan += n;
            // Angkatan yang tak penuh berarti sudah habis. Berhenti di sini,
            // bukan menunggu nol, supaya tak ada satu putaran sia-sia di akhir.
            if (n as i64) < batas_angkatan {
                break;
            }
            // Beri napas ke basis data di antara angkatan. Pembersihan ini tak
            // mendesak sama sekali; yang mendesak adalah pesan yang sedang
            // dikirim orang saat ini juga.
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }

        Ok((total_pesan, total_berkas))
    }

    // ── Room ─────────────────────────────────────────────────────────────────

    // `get_or_create_room` DIBUANG bersama grup produk (migrasi 029).
    // Percakapan kini lahir lewat `ensure_dm` atas kemauan pembeli, bukan
    // dibuatkan merchant per produk.
    //
    // Nama & logo toko juga berhenti jadi parameter: sejak 029 keduanya di-JOIN
    // dari `merchant_details` saat dibaca. Menyalinnya ke baris chat hanya
    // melahirkan nama basi — toko yang berganti nama akan tetap tampil dengan
    // nama lamanya di inbox, selamanya.

    /// Cari percakapan tanpa membuatnya. Dipakai halaman chat untuk memutuskan
    /// apakah ia memuat riwayat atau menampilkan percakapan kosong.
    pub async fn find_dm(&self, buyer_id: &str, merchant_id: &str) -> Result<Option<GroupRoom>> {
        self.repo.find_dm(buyer_id, merchant_id).await
    }

    pub async fn ensure_dm(&self, buyer_id: &str, merchant_id: &str) -> Result<GroupRoom> {
        if buyer_id == merchant_id {
            anyhow::bail!("Tidak bisa membuka percakapan dengan diri sendiri.");
        }
        if let Some(room) = self.repo.find_dm(buyer_id, merchant_id).await? {
            return Ok(room);
        }
        let room = self
            .repo
            .create_dm(buyer_id, merchant_id)
            .await?;

        // TIDAK menulis ke `group_members`, dan itu inti penyederhanaannya.
        //
        // Baris room sudah memuat `buyer_id` dan `merchant_id` — mereka MEMANG
        // kedua anggotanya. Menyalinnya lagi ke tabel terpisah menciptakan
        // sumber kebenaran kedua yang bisa berselisih dengan yang pertama, dan
        // selisih itu berbentuk paling buruk: room yang ada tapi tak bisa
        // dimasuki siapa pun. `is_member`/`get_member_ids` kini membacanya
        // langsung dari baris room (lihat repository).
        Ok(room)
    }

    pub async fn get_user_rooms(&self, user_id: &str) -> Result<Vec<GroupRoom>> {
        self.repo.get_user_rooms(user_id).await
    }

    // ── Join room ─────────────────────────────────────────────────────────────

    // `join_room` dan `auto_join_after_payment` DIBUANG bersama grup produk.
    // Tak ada lagi yang "bergabung": percakapan berdua sudah punya kedua
    // pesertanya sejak baris `chats` lahir, dan keanggotaan dibaca dari baris
    // itu — bukan dari tabel anggota yang sudah tak ada.

    pub async fn send_text(
        &self,
        room_id: &str,
        sender_id: &str,
        sender_name: &str,
        content: &str,
    ) -> Result<GroupMessage> {
        if content.trim().is_empty() {
            bail!("Pesan tidak boleh kosong");
        }

        let msg = GroupMessage {
            id: new_ulid(),
            room_id: room_id.to_string(),
            sender_id: sender_id.to_string(),
            sender_name: sender_name.to_string(),
            msg_type: MsgType::Text,
            content: content.to_string(),
            media_url: None,
            ticket_card: None,
            sent_at: chrono::Utc::now(),
            is_system: false,
        };

        self.authorize_and_save(&msg).await?;
        self.fanout(room_id, &msg).await;
        Ok(msg)
    }

    /// Share ticket card ke grup.
    /// Juga menggunakan send-limit yang sama dengan pesan teks.
    pub async fn share_ticket(
        &self,
        room_id: &str,
        sender_id: &str,
        sender_name: &str,
        ticket: TicketCard,
        caption: &str,
    ) -> Result<GroupMessage> {
        let msg = GroupMessage {
            id: new_ulid(),
            room_id: room_id.to_string(),
            sender_id: sender_id.to_string(),
            sender_name: sender_name.to_string(),
            msg_type: MsgType::SharedTicket,
            content: caption.to_string(),
            media_url: None,
            ticket_card: Some(ticket),
            sent_at: chrono::Utc::now(),
            is_system: false,
        };

        self.authorize_and_save(&msg).await?;
        self.fanout(room_id, &msg).await;
        Ok(msg)
    }

    /// Tandai percakapan sudah dibaca oleh `user_id`.
    ///
    /// Keanggotaan diperiksa DI DALAM SQL-nya (`WHERE ... buyer_id = $2 OR
    /// merchant_id = $2`), jadi id percakapan orang lain tak menghasilkan apa
    /// pun — bukan galat, hanya nol baris. Itu jawaban yang benar: memberi tahu
    /// pemanggil bahwa percakapan itu ada tapi bukan miliknya sudah membocorkan
    /// keberadaannya.
    pub async fn mark_read(&self, room_id: &str, user_id: &str) -> anyhow::Result<()> {
        self.repo.mark_read(room_id, user_id).await
    }

    pub async fn get_history(
        &self,
        room_id: &str,
        user_id: &str,
        limit: i64,
        before_id: Option<&str>,
    ) -> Result<(Vec<GroupMessage>, bool)> {
        if !self.repo.is_member(room_id, user_id).await? {
            bail!("Bukan member room ini");
        }
        let limit = limit.clamp(1, 100);
        self.repo.get_history(room_id, limit, before_id).await
    }

    /// Berapa pesan yang sudah dikirim user di room ini (untuk UI hint)
    pub async fn sent_count(&self, room_id: &str, user_id: &str) -> Result<i64> {
        self.repo.count_user_messages(room_id, user_id).await
    }

    // ── Internal ──────────────────────────────────────────────────────────────

    /// Satu-satunya syarat menulis: penulisnya salah satu dari dua peserta
    /// percakapan.
    ///
    /// ── KENAPA PLAFON SATU PESAN DIBUANG ─────────────────────────────────
    /// Dulu di sini ada plafon SATU pesan seumur hidup untuk peran `customer`.
    /// Itu masuk akal saat chat masih berupa GRUP per produk: puluhan pembeli
    /// dalam satu ruangan, dan plafon itu menahan pertanyaan yang sama
    /// diulang-ulang oleh banyak orang.
    ///
    /// Migrasi 029 mengubah chat menjadi percakapan BERDUA antara pembeli dan
    /// toko. Plafonnya ikut terbawa, dan di bentuk yang baru ia tidak lagi
    /// menahan spam — ia MENGHENTIKAN percakapan setelah satu kalimat:
    ///
    ///   * Pembeli mengirim pertanyaan pertama, lalu tak bisa menjawab
    ///     balasan tokonya sendiri.
    ///   * Pertanyaan KEDUA ke toko yang sama — dari halaman produk mana pun —
    ///     gagal, karena `ensure_dm` mengembalikan percakapan yang SUDAH ada
    ///     dan pesan keduanya menabrak plafon.
    ///
    /// Pesan gagalnya pun menyesatkan dua kali: ia menyebut "grup" yang sudah
    /// tak ada lagi, dan menyarankan "upgrade ke merchant" — yang bukan
    /// jawabannya, karena yang bersangkutan memang pembeli.
    ///
    /// Perlindungan dari banjir pesan tidak hilang, hanya pindah ke tempat
    /// yang benar: `RateLimitRegistry` per-user (30 pesan / 10 detik), yang
    /// dilewati SETIAP jalur kirim — WebSocket maupun server function.
    async fn authorize_and_save(&self, msg: &GroupMessage) -> Result<()> {
        if !self.repo.is_member(&msg.room_id, &msg.sender_id).await? {
            bail!("Bukan member room ini");
        }
        self.repo.save_message(msg).await
    }

    // `build_system_msg` dibuang bersama grup produk: pesan sistem dulu hanya
    // dipakai untuk mengumumkan "X bergabung". Skema masih mendukung
    // `MsgType::System` bila kelak dibutuhkan lagi.


    /// FIX: Tidak ada tokio::spawn — await langsung.
    /// Spawn tanpa bound menyebabkan task buildup pada load tinggi (Redis retry
    /// 3× × 200ms worst-case = ~350ms per fanout, 1k msg/s = ~350 outstanding tasks).
    /// broadcast_room local-delivery via try_send (non-blocking), Redis 1 publish.
    /// Latency tambahan ke caller hanya μs untuk local + ~1ms untuk Redis — acceptable.
    async fn fanout(&self, room_id: &str, msg: &GroupMessage) {
        let product = WsEvent::NewMessage(Box::new(WsMessage::from_model(msg)));
        self.ws_mgr.broadcast_room(room_id, product).await;
    }
}
