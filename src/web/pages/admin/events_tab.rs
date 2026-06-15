use leptos::prelude::*;
use leptos_router::components::A;

use crate::web::models::{format_date, format_price, Event};

#[derive(Clone, PartialEq)]
pub(super) enum EventStatus {
    OnSale,
    SoldOut,
    Presale,
}

impl EventStatus {
    pub(super) fn from_event(e: &Event) -> Self {
        if e.total_quota > 0 && e.total_sold >= e.total_quota {
            Self::SoldOut
        } else if e.status == "active" {
            Self::OnSale
        } else {
            Self::Presale
        }
    }
    pub(super) fn css_mod(&self) -> &'static str {
        match self {
            Self::OnSale  => "mhub-event-status mhub-event-status--sale",
            Self::SoldOut => "mhub-event-status mhub-event-status--sold",
            Self::Presale => "mhub-event-status mhub-event-status--presale",
        }
    }
    pub(super) fn label(&self) -> &'static str {
        match self {
            Self::OnSale  => "Dijual",
            Self::SoldOut => "Habis Terjual",
            Self::Presale => "Pre-order",
        }
    }
}

pub(super) fn view_all_events(evs: Vec<Event>) -> impl IntoView {
    view! {
        <section class="mhub-events-section">
            <div class="mhub-events-header">
                <h3 class="mhub-events-title">"Semua Acara Platform"</h3>
                <span class="mhub-live-badge">
                    <span class="mhub-live-dot"></span>
                    "Langsung"
                </span>
            </div>
            {if evs.is_empty() {
                view! {
                    <div class="mhub-empty">
                        <div class="mhub-empty-icon-wrap">
                            <svg width="38" height="38" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
                                <rect x="1" y="4" width="22" height="16" rx="2" ry="2"/>
                                <line x1="1" y1="10" x2="23" y2="10"/>
                            </svg>
                        </div>
                        <p class="mhub-empty-title">"Belum Ada Acara"</p>
                        <p class="mhub-empty-body">"Belum ada acara yang terdaftar di platform."</p>
                    </div>
                }.into_any()
            } else {
                evs.into_iter().map(|ev| {
                    let cover = ev.cover_url.as_deref()
                        .filter(|s| !s.is_empty())
                        .unwrap_or("https://images.unsplash.com/photo-1514525253161-7a46d19cd819?w=600&q=80")
                        .to_string();
                    let status     = EventStatus::from_event(&ev);
                    let title      = ev.name.clone();
                    let date       = format_date(&ev.event_date);
                    let venue_str  = match (ev.venue.as_deref(), ev.city.as_deref()) {
                        (Some(v), Some(c)) if !c.is_empty() => format!("{v} • {c}"),
                        (Some(v), _) => v.to_string(),
                        _ => String::new(),
                    };
                    let sold  = ev.total_sold;
                    let quota = ev.total_quota;
                    let avail = (quota - sold).max(0);
                    let pct   = if quota > 0 {
                        ((sold as f64 / quota as f64) * 100.0).round() as u32
                    } else { 0 };
                    let fill_style = format!("width:{pct}%");
                    let (val_text, val_cls) = if status == EventStatus::SoldOut {
                        ("100% Habis Terjual".to_string(), "mhub-event-progress-val mhub-event-progress-val--sold")
                    } else if quota == 0 {
                        ("—".to_string(), "mhub-event-progress-val")
                    } else {
                        (format!("{sold}/{quota} Terjual"), "mhub-event-progress-val")
                    };
                    let remaining_text = if quota == 0 { String::new() } else { format!("{avail} sisa") };
                    let fill_cls = match &status {
                        EventStatus::SoldOut => "mhub-event-progress-fill mhub-event-progress-fill--sold",
                        EventStatus::Presale => "mhub-event-progress-fill mhub-event-progress-fill--lime",
                        _                    => "mhub-event-progress-fill",
                    };
                    let price      = format_price(ev.display_price);
                    let slug       = ev.slug.clone();
                    let status_css = status.css_mod();
                    let status_lbl = status.label();

                    view! {
                        <div class="mhub-event-card">
                            <div class="mhub-event-card-img-wrap">
                                <img src=cover alt=title.clone() class="mhub-event-card-img"/>
                                <span class=status_css>{status_lbl}</span>
                            </div>
                            <div class="mhub-event-card-body">
                                <div class="mhub-event-card-top-row">
                                    <p class="mhub-event-card-title">{title}</p>
                                    <div class="mhub-event-card-price-block">
                                        <span class="mhub-event-price-label">"Mulai dari"</span>
                                        <span class="mhub-event-price-value">{price}</span>
                                    </div>
                                </div>
                                <p class="mhub-event-card-meta">{date}" • "{venue_str}</p>
                                <div class="mhub-event-progress-section">
                                    <div class="mhub-event-progress-row">
                                        <span class="mhub-event-progress-key">"Penjualan"</span>
                                        <span class=val_cls>{val_text}</span>
                                    </div>
                                    <div class="mhub-event-progress-bar">
                                        <div class=fill_cls style=fill_style></div>
                                    </div>
                                    {(!remaining_text.is_empty()).then(|| view! {
                                        <div class="mhub-event-remaining-row">
                                            <span class="mhub-event-remaining-badge">
                                                <svg width="10" height="10" viewBox="0 0 24 24" fill="none"
                                                     stroke="currentColor" stroke-width="2.5">
                                                    <circle cx="12" cy="12" r="10"/>
                                                    <line x1="12" y1="8" x2="12" y2="12"/>
                                                    <line x1="12" y1="16" x2="12.01" y2="16"/>
                                                </svg>
                                                {remaining_text}
                                            </span>
                                        </div>
                                    })}
                                </div>
                                <div class="mhub-event-card-actions">
                                    <A href=format!("/admin/events/{slug}/edit")
                                       attr:class="mhub-event-manage-btn">
                                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none"
                                             stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                            <path d="M11 4H4a2 2 0 00-2 2v14a2 2 0 002 2h14a2 2 0 002-2v-7"/>
                                            <path d="M18.5 2.5a2.121 2.121 0 013 3L12 15l-4 1 1-4 9.5-9.5z"/>
                                        </svg>
                                        "Sunting Acara"
                                    </A>
                                </div>
                            </div>
                        </div>
                    }
                }).collect_view().into_any()
            }}
        </section>
    }
}
