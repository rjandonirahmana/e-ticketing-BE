use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::web::api::{
    get_notif_unread_count, get_notifications, mark_all_notifications_read,
    mark_notification_read as sf_mark_read,
};
use crate::web::models::NotificationItem;

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

fn item_to_notif(n: NotificationItem) -> Notif {
    // Tentukan pill berdasarkan kind
    let (pill, pill_kind) = match n.kind.as_str() {
        "payment_success" | "order_paid" => (Some("PAID".into()), "live".to_string()),
        "promo" => (Some("PROMO".into()), "promo".to_string()),
        "event_reminder" => (None, "new".to_string()),
        "artist_update" => (Some("NEW".into()), "new".to_string()),
        _ => (None, "new".to_string()),
    };

    // CTA dan href berdasarkan kind + target_id
    let (cta, cta_href) = match (n.kind.as_str(), n.target_id.as_deref()) {
        ("order", Some(id))  => (Some("Lihat Order".into()), Some(format!("/orders/{}", id))),
        ("ticket", Some(id)) => (Some("Lihat Tiket".into()), Some(format!("/tickets/{}", id))),
        _                    => (None, None),
    };

    // Format waktu dari timestamp (YYYY-MM-DD HH:MM)
    let time = n
        .created_at
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
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

impl NotificationsCtx {
    pub fn load(&self) {
        // SSR guard: spawn_local tidak tersedia di server
        if is_server() {
            return;
        }
        if self.loading.get_untracked() {
            return;
        }
        self.loading.set(true);
        let items = self.items;
        let loading = self.loading;
        let unread_count = self.unread_count;
        let error = self.error;

        spawn_local(async move {
            match get_notifications().await {
                Ok(list) => {
                    let mapped: Vec<Notif> = list.into_iter().map(item_to_notif).collect();
                    items.set(mapped);
                }
                Err(e) => error.set(e.to_string()),
            }

            if let Ok(count) = get_notif_unread_count().await {
                unread_count.set(count);
            }

            loading.set(false);
        });
    }

    /// Tandai satu notif sebagai dibaca (optimistic update + server fn).
    pub fn mark_read(&self, id: String) {
        let items = self.items;
        let unread_count = self.unread_count;

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
            let _ = sf_mark_read(id_clone).await;
        });
    }

    pub fn mark_all_read(&self) {
        let items = self.items;
        let unread_count = self.unread_count;

        items.update(|list| {
            for n in list.iter_mut() {
                n.is_read = true;
            }
        });
        unread_count.set(0);

        spawn_local(async move {
            let _ = mark_all_notifications_read().await;
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
