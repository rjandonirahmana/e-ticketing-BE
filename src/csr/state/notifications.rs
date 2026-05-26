use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::Deserialize;

use crate::csr::services::client::get_private;

// ── BE response type ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
struct BeNotification {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub is_read: bool,
    #[serde(default)]
    pub order_id: Option<String>,
    #[serde(default)]
    pub ticket_id: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
}

// ── Frontend Notif model ──────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
pub struct Notif {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub time: String,
    pub pill: Option<String>,
    pub pill_kind: String,
    pub cta: Option<String>,
    /// Href untuk CTA — ke /orders/:id atau /tickets/:id jika ada relasi.
    pub cta_href: Option<String>,
    pub section: String,
    pub is_read: bool,
}

fn be_to_notif(n: BeNotification) -> Notif {
    // Tentukan pill berdasarkan kind
    let (pill, pill_kind) = match n.kind.as_str() {
        "payment_success" | "order_paid" => (Some("PAID".into()), "live".to_string()),
        "promo" => (Some("PROMO".into()), "promo".to_string()),
        "event_reminder" => (None, "new".to_string()),
        "artist_update" => (Some("NEW".into()), "new".to_string()),
        _ => (None, "new".to_string()),
    };

    // CTA dan href berdasarkan relasi order/ticket
    let (cta, cta_href) = if let Some(ref oid) = n.order_id {
        (Some("Lihat Order".into()), Some(format!("/orders/{}", oid)))
    } else if let Some(ref tid) = n.ticket_id {
        (Some("Lihat Tiket".into()), Some(format!("/tickets/{}", tid)))
    } else {
        (None, None)
    };

    // Format waktu dari ISO timestamp
    let time = n
        .created_at
        .as_deref()
        .and_then(|s| s.get(..16))
        .map(|s| s.replace('T', " "))
        .unwrap_or_else(|| "—".into());

    // Tentukan section berdasarkan kind
    let section = match n.kind.as_str() {
        "promo" => "PROMOTIONS",
        _ => "TODAY",
    }
    .to_string();

    Notif {
        id: n.id,
        kind: n.kind,
        title: n.title.to_uppercase(),
        body: n.body,
        time,
        pill,
        pill_kind,
        cta,
        cta_href,
        section,
        is_read: n.is_read,
    }
}

// ── Context ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
pub struct NotificationsCtx {
    pub items: RwSignal<Vec<Notif>>,
    pub loading: RwSignal<bool>,
    pub unread_count: RwSignal<i64>,
    pub error: RwSignal<String>,
}

#[derive(Debug, Deserialize)]
struct UnreadCountResp {
    count: i64,
}

impl NotificationsCtx {
    pub fn load(&self) {
        if self.loading.get_untracked() {
            return;
        }
        self.loading.set(true);
        let items = self.items;
        let loading = self.loading;
        let unread_count = self.unread_count;
        let error = self.error;

        spawn_local(async move {
            // Fetch notifications list
            match get_private::<Vec<BeNotification>>("/notifications").await {
                Ok(list) => {
                    let mapped: Vec<Notif> = list.into_iter().map(be_to_notif).collect();
                    items.set(mapped);
                }
                Err(e) => error.set(e.message),
            }

            // Fetch unread count (badge)
            if let Ok(resp) = get_private::<UnreadCountResp>("/notifications/unread-count").await {
                unread_count.set(resp.count);
            }

            loading.set(false);
        });
    }

    /// Tandai satu notif sebagai dibaca (optimistic update + API call).
    pub fn mark_read(&self, id: String) {
        let items = self.items;
        let unread_count = self.unread_count;

        // Optimistic: update UI langsung
        items.update(|list| {
            if let Some(n) = list.iter_mut().find(|n| n.id == id) {
                if !n.is_read {
                    n.is_read = true;
                    unread_count.update(|c| *c = c.saturating_sub(1));
                }
            }
        });

        let id_clone = id.clone();
        spawn_local(async move {
            use crate::csr::services::client::post_private;
            let path = format!("/notifications/{}/read", id_clone);
            let _: Result<serde_json::Value, _> =
                post_private(&path, &serde_json::json!({})).await;
        });
    }

    pub fn mark_all_read(&self) {
        let items = self.items;
        let unread_count = self.unread_count;

        // Optimistic
        items.update(|list| {
            for n in list.iter_mut() {
                n.is_read = true;
            }
        });
        unread_count.set(0);

        spawn_local(async move {
            use crate::csr::services::client::post_private;
            let _: Result<serde_json::Value, _> =
                post_private("/notifications/read-all", &serde_json::json!({})).await;
        });
    }
}

pub fn provide_notifications_store() {
    provide_context(NotificationsCtx {
        items: RwSignal::new(vec![]),
        loading: RwSignal::new(false),
        unread_count: RwSignal::new(0),
        error: RwSignal::new(String::new()),
    });
}

pub fn use_notifications_store() -> NotificationsCtx {
    use_context::<NotificationsCtx>().expect("NotificationsCtx not provided")
}
