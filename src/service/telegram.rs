use chrono::Utc;
use chrono_tz::Asia::Jakarta;
use reqwest::Client;
use serde_json::json;
use std::time::Duration;
use tracing::warn;

#[derive(Clone)]
pub struct TelegramService {
    bot_token: String,
    pub admin_chat_id: i64,
    http: Client,
}

impl TelegramService {
    pub fn new(bot_token: String, admin_chat_id: i64) -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(10))
            .pool_idle_timeout(Some(Duration::from_secs(30)))
            .build()
            .expect("build reqwest client for Telegram");

        Self {
            bot_token,
            admin_chat_id,
            http,
        }
    }

    /// Kirim teks HTML ke chat_id.
    pub async fn send_message(&self, chat_id: i64, text: &str) -> anyhow::Result<()> {
        let url = format!("https://api.telegram.org/bot{}/sendMessage", self.bot_token);

        let res = self
            .http
            .post(&url)
            .json(&json!({
                "chat_id":    chat_id,
                "text":       text,
                "parse_mode": "HTML"
            }))
            .send()
            .await?;

        if !res.status().is_success() {
            let status = res.status();
            let body = res.text().await.unwrap_or_default();
            anyhow::bail!("Telegram API error {status}: {body}");
        }

        Ok(())
    }

    /// Kirim SERVER ERROR ALERT ke admin_chat_id.
    /// Dipanggil fire-and-forget dari AppError::into_response — tidak boleh panic.
    pub async fn send_error_alert(&self, status_code: u16, error_kind: &str, detail: &str) {
        let timestamp = Utc::now()
            .with_timezone(&Jakarta)
            .format("%Y-%m-%d %H:%M:%S WIB")
            .to_string();

        // Escape karakter HTML agar Telegram tidak reject message
        // SQL sering mengandung <, >, & yang membreak HTML parser Telegram
        let safe_detail = html_escape(detail);

        // Potong kalau terlalu panjang (Telegram max 4096 chars)
        let safe_detail = truncate_str(&safe_detail, 800);

        let text = format!(
            "🚨 <b>SERVER ERROR ALERT</b> 🚨\n\
             \n\
             📅 <b>Waktu:</b> {timestamp}\n\
             🔧 <b>Service:</b> Kinetic API\n\
             📊 <b>Status Code:</b> {status_code}\n\
             💬 <b>Error Type:</b> {error_kind}\n\
             ❌ <b>Detail:</b>\n<pre>{safe_detail}</pre>\n\
             \n\
             #ServerError #Alert #Monitoring"
        );

        if let Err(e) = self.send_message(self.admin_chat_id, &text).await {
            warn!("Gagal kirim Telegram error alert: {e}");
        }
    }
}

/// Escape karakter khusus HTML untuk Telegram HTML mode.
/// Wajib: &amp; harus duluan sebelum yang lain.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Potong string ke max `max_chars` karakter (unicode-aware).
/// Tambah "…" kalau dipotong.
fn truncate_str(s: &str, max_chars: usize) -> String {
    let mut chars = s.chars();
    let taken: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{}\n…(truncated)", taken)
    } else {
        taken
    }
}
