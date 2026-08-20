//! notification_detail.rs — Detail notifikasi dengan design nd-* (SSR port).

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use crate::web::api::{get_notification_detail, mark_notification_read};
use crate::web::hooks::ThemeToggle;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn kind_eyebrow(kind: &str) -> &'static str {
    match kind {
        "order" | "order_paid" | "payment_success" => "ORDER UPDATE",
        "ticket"  => "TICKET READY",
        "story"   => "STORY UPDATE",
        "promo"   => "PROMO",
        "artist_update" => "ARTIST UPDATE",
        _ => "NOTIFICATION",
    }
}

fn fmt_time(iso: &str) -> String {
    let s = iso.replace('T', " ");
    let head = s.split(['.', '+']).next().unwrap_or(&s);
    head.chars().take(16).collect()
}

// ── Skeleton body (used inside the always-rendered .page wrapper) ─────────────

#[component]
fn DetailSkeleton() -> impl IntoView {
    view! {
        <header class="page-header" style="position:relative;">
            <A href="/notifications" attr:class="back-btn">
                <svg width="22" height="22" viewBox="0 0 24 24" fill="none"
                     stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                    <polyline points="15 18 9 12 15 6"/>
                </svg>
            </A>
            <div class="shimmer-bg" style="width:140px;height:18px;border-radius:8px;"></div>
            <div class="shimmer-bg" style="width:36px;height:36px;border-radius:50%;"></div>
        </header>
        <div class="nd-hero">
            <div class="shimmer-bg" style="width:110px;height:110px;border-radius:50%;"></div>
            <div class="shimmer-bg" style="width:60%;height:10px;border-radius:4px;"></div>
            <div class="shimmer-bg" style="width:80%;height:34px;border-radius:8px;"></div>
            <div class="shimmer-bg" style="width:90%;height:14px;border-radius:6px;"></div>
        </div>
        <div class="nd-card">
            <div class="nd-row">
                <div class="nd-cell">
                    <div class="shimmer-bg" style="width:50px;height:9px;border-radius:4px;"></div>
                    <div class="shimmer-bg" style="width:100px;height:14px;border-radius:6px;margin-top:4px;"></div>
                </div>
                <div class="nd-cell nd-cell--right">
                    <div class="shimmer-bg" style="width:60px;height:9px;border-radius:4px;"></div>
                    <div class="shimmer-bg" style="width:70px;height:18px;border-radius:100px;margin-top:4px;"></div>
                </div>
            </div>
        </div>
        <div class="nd-cta-wrap">
            <div class="shimmer-bg" style="width:100%;height:52px;border-radius:100px;"></div>
        </div>
    }
}

// ── Header ────────────────────────────────────────────────────────────────────

#[component]
fn DetailHeader() -> impl IntoView {
    view! {
        <header class="page-header" style="position:relative;">
            <A href="/notifications" attr:class="back-btn">
                <svg width="22" height="22" viewBox="0 0 24 24" fill="none"
                     stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                    <polyline points="15 18 9 12 15 6"/>
                </svg>
            </A>
            <span class="page-logo"
                  style="position:absolute;left:50%;top:50%;transform:translate(-50%,-50%);">
                "NOTIFICATIONS"
            </span>
            <div class="header-actions">
                <ThemeToggle/>
                <A href="/profile" attr:class="nav-avatar">
                    <svg width="18" height="18" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="2" stroke-linecap="round">
                        <path d="M20 21v-2a4 4 0 00-4-4H8a4 4 0 00-4 4v2"/>
                        <circle cx="12" cy="7" r="4"/>
                    </svg>
                </A>
            </div>
        </header>
    }
}

// ── Main page ─────────────────────────────────────────────────────────────────

#[component]
pub fn NotificationDetailPage() -> impl IntoView {
    let params = use_params_map();
    let notif_id = move || params.read().get("id").unwrap_or_default();


    let reload = RwSignal::new(0u32);

    // Penggerbang `is_logged_in()` DIBUANG dari sumber resource.
    //
    // Menebak status login di klien sebelum memanggil server tak menjaga apa
    // pun — server function-nya sudah menuntut sesi sendiri — tapi ia menambah
    // satu cara untuk gagal: selama `AuthResource` belum terbaca, fetcher
    // menjawab `not_ready`, dan `not_ready` dirender sebagai skeleton yang sama
    // persis dengan "sedang memuat". Halaman lalu berkedip tanpa akhir, tanpa
    // pesan, tanpa percobaan ulang — dan dari sisi pengguna itu terbaca sebagai
    // "klik tombol, halaman tidak pindah". Muat ulang menolong hanya karena
    // jalur SSR membaca sesi dari cookie di server, jauh dari masalah ini.
    //
    // Sekarang satu-satunya syarat adalah id-nya ada.
    let notif = Resource::new(
        move || { let _ = reload.get(); notif_id() },
        |id| async move {
            if id.is_empty() {
                return Err(ServerFnError::ServerError("not_ready".into()));
            }
            let _ = mark_notification_read(id.clone()).await;
            get_notification_detail(id).await
        },
    );

    view! {
        // .page is always rendered — Suspense only replaces inner content
        <div class="page">
            <Suspense fallback=|| view! { <DetailSkeleton/> }>
                {move || notif.get().map(|res| match res {
                    Err(e) if e.to_string().contains("not_ready") => view! {
                        <DetailHeader/>
                    }.into_any(),

                    Err(e) => view! {
                        <DetailHeader/>
                        <div class="notif-empty-wrap" style="padding-top:60px;">
                            <div class="notif-empty-icon-wrap">
                                <div class="notif-empty-bell-circle">
                                    <svg width="30" height="30" viewBox="0 0 24 24" fill="none"
                                         stroke="currentColor" stroke-width="1.6"
                                         stroke-linecap="round" stroke-linejoin="round">
                                        <path d="M18 8a6 6 0 0 0-12 0c0 7-3 9-3 9h18s-3-2-3-9"/>
                                        <path d="M13.7 21a2 2 0 0 1-3.4 0"/>
                                    </svg>
                                </div>
                            </div>
                            <h2 class="notif-empty-title">"Gagal memuat"</h2>
                            <p class="notif-empty-body">{e.to_string()}</p>
                            <button class="notif-empty-cta"
                                on:click=move |_| reload.update(|n| *n += 1)>
                                "Coba Lagi"
                            </button>
                        </div>
                    }.into_any(),

                    Ok(n) => {
                        let eyebrow = kind_eyebrow(&n.kind).to_string();
                        let cta_href = n.target_id.as_ref().and_then(|id| match n.kind.as_str() {
                            "order"  => Some(format!("/orders/{}", id)),
                            "ticket" => Some(format!("/tickets/{}", id)),
                            _        => None,
                        });
                        let cta_label = match n.kind.as_str() {
                            "order"  => "Lihat Order",
                            "ticket" => "View Ticket",
                            _        => "",
                        };
                        let created = n.created_at
                            .map(|dt| fmt_time(&dt.to_rfc3339()))
                            .unwrap_or_default();
                        let id_short = n.id.chars().take(8).collect::<String>();
                        let title = n.title.clone();
                        let body = n.body.clone();

                        view! {
                            <DetailHeader/>
                            <div class="nd-hero">
                                <div class="nd-check-ring">
                                    <div class="nd-check-circle">
                                        <svg width="44" height="44" viewBox="0 0 24 24" fill="none"
                                             stroke="#0d0d1a" stroke-width="3" stroke-linecap="round" stroke-linejoin="round">
                                            <path d="M18 8a6 6 0 0 0-12 0c0 7-3 9-3 9h18s-3-2-3-9"/>
                                            <path d="M13.7 21a2 2 0 0 1-3.4 0"/>
                                        </svg>
                                    </div>
                                </div>
                                <span class="nd-eyebrow">{eyebrow}</span>
                                <h1 class="nd-title">{title}</h1>
                                <p class="nd-subtitle">{body}</p>
                            </div>

                            <div class="nd-card">
                                <div class="nd-row">
                                    <div class="nd-cell">
                                        <span class="nd-label">"NOTIFICATION ID"</span>
                                        <span class="nd-val">{format!("#{}", id_short)}</span>
                                    </div>
                                    <div class="nd-cell nd-cell--right">
                                        <span class="nd-label">"RECEIVED"</span>
                                        <span class="nd-val">{created}</span>
                                    </div>
                                </div>
                            </div>

                            {if let Some(href) = cta_href {
                                view! {
                                    <div class="nd-cta-wrap">
                                        <A href={href} attr:class="nd-cta-btn">
                                            <span>{cta_label}</span>
                                            <svg width="18" height="18" viewBox="0 0 24 24" fill="none"
                                                 stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                                <path d="M5 12h14M12 5l7 7-7 7"/>
                                            </svg>
                                        </A>
                                    </div>
                                }.into_any()
                            } else {
                                view! {}.into_any()
                            }}
                        }.into_any()
                    }
                }).unwrap_or_else(|| view! { <DetailSkeleton/> }.into_any())}
            </Suspense>
        </div>
    }
}
