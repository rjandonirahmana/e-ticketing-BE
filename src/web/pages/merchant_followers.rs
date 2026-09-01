//! merchant_followers.rs — Daftar follower merchant (/m/:id/followers).
//!
//! Dibuka dari statistik FOLLOWERS di profil publik /m/:id maupun preview
//! Merchant Hub. TANPA bottom navbar (halaman detail berdiri sendiri). Data:
//! `merchant_follows` → users (hanya `name` yang tersedia; belum ada
//! username/avatar di skema, jadi avatar = inisial & sublabel = tanggal follow).
//! Follower yang berperan merchant → baris menjadi tautan ke profil /m/{id}-nya.

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use crate::web::api::get_merchant_followers;
use crate::web::hooks::ThemeToggle;
use crate::web::models::FollowerItem;

use super::merchant_public::fmt_count;

const PER_PAGE: usize = 30;

use crate::web::utils::waktu::tanggal as fmt_date;

/// Skeleton daftar follower.
#[component]
fn FollowersShimmer() -> impl IntoView {
    view! {
        <div class="mflw-list">
            {(0..6)
                .map(|_| {
                    view! {
                        <div class="mflw-item">
                            <span
                                class="shimmer-bg"
                                style="width:52px;height:52px;border-radius:50%;flex:none;"
                            ></span>
                            <div class="mflw-meta" style="gap:8px;">
                                <span
                                    class="shimmer-bg"
                                    style="width:120px;height:14px;border-radius:6px;"
                                ></span>
                                <span
                                    class="shimmer-bg"
                                    style="width:80px;height:11px;border-radius:5px;"
                                ></span>
                            </div>
                        </div>
                    }
                })
                .collect_view()}
        </div>
    }
}

#[component]
pub fn MerchantFollowersPage() -> impl IntoView {
    let params = use_params_map();
    let mid = move || params.read().get("id").unwrap_or_default();

    // Daftar inkremental: halaman pertama via Resource (ikut SSR), berikutnya
    // di-append lewat "Muat lebih banyak".
    let items: RwSignal<Vec<FollowerItem>> = RwSignal::new(Vec::new());
    let total = RwSignal::new(0i64);
    let page = RwSignal::new(1i64);
    let has_more = RwSignal::new(false);
    let loading = RwSignal::new(false);
    let query = RwSignal::new(String::new());

    let first_page = Resource::new(mid, |id| async move {
        if id.is_empty() {
            return Err(ServerFnError::ServerError("not_ready".into()));
        }
        get_merchant_followers(id, Some(1)).await
    });
    Effect::new(move |_| {
        if let Some(Ok(d)) = first_page.get() {
            total.set(d.total);
            has_more.set(d.items.len() == PER_PAGE);
            page.set(2);
            items.set(d.items);
        }
    });

    let load_more = move |_| {
        if loading.get_untracked() || !has_more.get_untracked() {
            return;
        }
        loading.set(true);
        let id = mid();
        let next = page.get_untracked();
        leptos::task::spawn_local(async move {
            if let Ok(d) = get_merchant_followers(id, Some(next)).await {
                has_more.set(d.items.len() == PER_PAGE);
                page.set(next + 1);
                items.update(|v| v.extend(d.items));
            }
            loading.set(false);
        });
    };

    // Filter pencarian (client-side, per nama).
    let filtered = move || {
        let q = query.get().trim().to_lowercase();
        items.with(|v| {
            v.iter()
                .filter(|f| q.is_empty() || f.name.to_lowercase().contains(&q))
                .cloned()
                .collect::<Vec<_>>()
        })
    };

    view! {
        <div class="mp-page">
            <header class="page-header mp-header">
                <A
                    href=move || format!("/m/{}", mid())
                    attr:class="back-btn"
                    attr:aria-label="Kembali"
                >
                    <svg
                        width="20"
                        height="20"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2.5"
                        stroke-linecap="round"
                    >
                        <polyline points="15 18 9 12 15 6" />
                    </svg>
                </A>
                <span class="page-logo">"FOLLOWERS"</span>
                <div class="header-actions">
                    <span class="mflw-count-badge">{move || fmt_count(total.get())}</span>
                    <ThemeToggle />
                </div>
            </header>

            <div class="mp-container">
                // ── Pencarian ─────────────────────────────────────────────────
                <div class="mflw-search">
                    <svg
                        width="18"
                        height="18"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                    >
                        <circle cx="11" cy="11" r="8" />
                        <line x1="21" y1="21" x2="16.65" y2="16.65" />
                    </svg>
                    <input
                        class="mflw-search-input"
                        type="text"
                        placeholder="Cari follower..."
                        prop:value=move || query.get()
                        on:input=move |e| query.set(event_target_value(&e))
                    />
                </div>

                <Suspense fallback=|| {
                    view! { <FollowersShimmer /> }
                }>
                    {move || {
                        match first_page.get() {
                            None => view! { <FollowersShimmer /> }.into_any(),
                            Some(Err(e)) if e.to_string().contains("not_ready") => {
                                view! { <FollowersShimmer /> }.into_any()
                            }
                            Some(Err(_)) => {
                                view! {
                                    <div class="medit-error-banner">"Gagal memuat follower."</div>
                                }
                                    .into_any()
                            }
                            Some(Ok(_)) => {
                                let list = filtered();
                                if list.is_empty() {
                                    return view! {
                                        <p class="mp-empty">
                                            {move || {
                                                if query.get().trim().is_empty() {
                                                    "Belum ada follower."
                                                } else {
                                                    "Tidak ada follower yang cocok."
                                                }
                                            }}
                                        </p>
                                    }
                                        .into_any();
                                }
                                view! {
                                    <div class="mflw-list">
                                        {list
                                            .into_iter()
                                            .map(|f| {
                                                let initial: String = f
                                                    .name
                                                    .chars()
                                                    .next()
                                                    .unwrap_or('P')
                                                    .to_uppercase()
                                                    .to_string();
                                                // role='merchant' ⟺ punya /m/{id} (dijamin
                                                // trigger migrasi 016), jadi cukup cek role.
                                                let is_merchant = f.role == "merchant";
                                                let sub = if is_merchant {
                                                    format!("Toko · {}", fmt_date(&f.created_at))
                                                } else {
                                                    format!("Mengikuti sejak {}", fmt_date(&f.created_at))
                                                };
                                                // Rute berbasis role: follower yang
                                                // merchant → profil merchant /m/{id}
                                                // (merchant_id == user_id); selain itu →
                                                // profil user /u/{id}. Tag MERCHANT untuk
                                                // yang berperan penyelenggara.
                                                let profile_href = if is_merchant {
                                                    format!("/m/{}", f.user_id)
                                                } else {
                                                    format!("/u/{}", f.user_id)
                                                };
                                                view! {
                                                    <a
                                                        class="mflw-item mflw-item--link"
                                                        href=profile_href
                                                    >
                                                        <div class="mflw-avatar">{initial}</div>
                                                        <div class="mflw-meta">
                                                            <span class="mflw-name">{f.name.clone()}</span>
                                                            <span class="mflw-sub">{sub}</span>
                                                        </div>
                                                        {is_merchant
                                                            .then(|| {
                                                                view! { <span class="mflw-tag">"MERCHANT"</span> }
                                                            })}
                                                    </a>
                                                }
                                                    .into_any()
                                            })
                                            .collect_view()}
                                    </div>
                                }
                                    .into_any()
                            }
                        }
                    }}
                </Suspense>

                // ── Muat lebih banyak (disembunyikan saat sedang mencari) ──────
                {move || {
                    (has_more.get() && query.get().trim().is_empty())
                        .then(|| {
                            view! {
                                <div class="mp-more-wrap">
                                    <button
                                        class="mp-more-btn"
                                        disabled=move || loading.get()
                                        on:click=load_more
                                    >
                                        {move || {
                                            if loading.get() { "MEMUAT…" } else { "MUAT LEBIH BANYAK" }
                                        }}
                                    </button>
                                </div>
                            }
                        })
                }}
            </div>
        </div>
    }
}
