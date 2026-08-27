//! reviews.rs — Rating & ulasan merchant (/m/:id/reviews), sisi user.
//!
//! Ringkasan (angka besar + bintang + distribusi bar per bintang), form tulis
//! ulasan (login required; satu ulasan per user — submit ulang = perbarui),
//! dan daftar ulasan terbaru.

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use crate::web::api::{can_review_merchant, get_reviews, submit_merchant_review};
use crate::web::app::AuthResource;
use crate::web::hooks::ThemeToggle;

/// Baris bintang statis (untuk ringkasan & item ulasan).
#[component]
fn Stars(#[prop(into)] rating: f64) -> impl IntoView {
    view! {
        <span class="mrv-stars" aria-label=format!("{rating:.1} dari 5")>
            {(1..=5)
                .map(|i| {
                    let cls = if (i as f64) <= rating + 0.25 {
                        "mrv-star mrv-star--on"
                    } else {
                        "mrv-star"
                    };
                    view! { <span class=cls>"★"</span> }
                })
                .collect_view()}
        </span>
    }
}

/// Skeleton halaman reviews: ringkasan (angka + bintang + distribusi) & daftar.
#[component]
fn ReviewsShimmer() -> impl IntoView {
    view! {
        <div class="shimmer-bg" style="width:120px;height:12px;border-radius:5px;margin:16px 0 6px;"></div>
        <div class="mrv-summary">
            <div class="mrv-big">
                <div class="shimmer-bg" style="width:92px;height:64px;border-radius:14px;"></div>
                <div class="mrv-big-side">
                    <div class="shimmer-bg" style="width:116px;height:20px;border-radius:6px;"></div>
                    <div
                        class="shimmer-bg"
                        style="width:56px;height:13px;border-radius:5px;margin-top:8px;"
                    ></div>
                </div>
            </div>
            <div
                class="shimmer-bg"
                style="width:96px;height:14px;border-radius:6px;margin:14px 0 16px;"
            ></div>
            <div class="mrv-dist">
                {(0..5)
                    .map(|_| {
                        view! {
                            <div class="mrv-dist-row">
                                <span
                                    class="shimmer-bg"
                                    style="width:10px;height:12px;border-radius:3px;"
                                ></span>
                                <div class="shimmer-bg" style="flex:1;height:9px;border-radius:999px;"></div>
                                <span
                                    class="shimmer-bg"
                                    style="width:28px;height:12px;border-radius:3px;"
                                ></span>
                            </div>
                        }
                    })
                    .collect_view()}
            </div>
        </div>
        <div class="shimmer-bg" style="width:130px;height:12px;border-radius:5px;margin:6px 0 10px;"></div>
        <div class="mrv-list">
            {(0..3)
                .map(|_| {
                    view! {
                        <div class="mrv-item">
                            <div class="mrv-item-head">
                                <span
                                    class="shimmer-bg"
                                    style="width:38px;height:38px;border-radius:50%;flex:none;"
                                ></span>
                                <div class="mrv-item-who" style="gap:6px;">
                                    <span
                                        class="shimmer-bg"
                                        style="width:100px;height:13px;border-radius:5px;"
                                    ></span>
                                    <span
                                        class="shimmer-bg"
                                        style="width:64px;height:10px;border-radius:4px;"
                                    ></span>
                                </div>
                            </div>
                            <div
                                class="shimmer-bg"
                                style="width:100%;height:34px;border-radius:8px;margin-top:10px;"
                            ></div>
                        </div>
                    }
                })
                .collect_view()}
        </div>
    }
}

#[component]
pub fn MerchantReviewsPage() -> impl IntoView {
    let params = use_params_map();
    let mid = move || params.read().get("id").unwrap_or_default();

    let auth = use_context::<AuthResource>().expect("AuthResource missing");
    let is_logged_in = move || auth.get().and_then(|r| r.ok()).flatten().is_some();

    let reviews = Resource::new(mid, |id| async move {
        if id.is_empty() {
            return Err(ServerFnError::ServerError("not_ready".into()));
        }
        get_reviews(id, Some(1)).await
    });

    // Kelayakan menulis ulasan: hanya user dengan ≥1 pesanan 'paid' ke merchant.
    // Server fn membaca identitas dari cookie (di-fetch saat mount, ikut SSR).
    let can_review = Resource::new(mid, |id| async move {
        if id.is_empty() {
            return false;
        }
        can_review_merchant(id).await.unwrap_or(false)
    });

    // ── Form ulasan ───────────────────────────────────────────────────────────
    let f_rating = RwSignal::new(0i32);
    let f_comment = RwSignal::new(String::new());
    let submitting = RwSignal::new(false);
    let form_msg = RwSignal::new(String::new());

    let do_submit = move |_| {
        form_msg.set(String::new());
        let rating = f_rating.get_untracked();
        if !(1..=5).contains(&rating) {
            form_msg.set("Pilih jumlah bintang dulu.".into());
            return;
        }
        let id = mid();
        let comment = f_comment.get_untracked();
        submitting.set(true);
        leptos::task::spawn_local(async move {
            match submit_merchant_review(id, rating, comment).await {
                Ok(()) => {
                    form_msg.set("Ulasan tersimpan. Terima kasih!".into());
                    f_comment.set(String::new());
                    f_rating.set(0);
                    reviews.refetch();
                }
                Err(e) => form_msg.set(format!("Gagal menyimpan ulasan: {e}")),
            }
            submitting.set(false);
        });
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
                <span class="page-logo">"REVIEWS"</span>
                <div class="header-actions">
                    <ThemeToggle />
                </div>
            </header>

            <div class="mp-container">
                <Suspense fallback=|| {
                    view! { <ReviewsShimmer /> }
                }>
                    {move || {
                        match reviews.get() {
                            None => view! { <ReviewsShimmer /> }.into_any(),
                            Some(Err(e)) if e.to_string().contains("not_ready") => {
                                view! { <ReviewsShimmer /> }.into_any()
                            }
                            Some(Err(_)) => {
                                view! {
                                    <div class="medit-error-banner">"Gagal memuat ulasan."</div>
                                }
                                    .into_any()
                            }
                            Some(Ok(d)) => {
                                let total = d.total.max(0);
                                view! {
                                    <p class="mrv-store">{d.store_name.clone()}</p>

                                    // ── Ringkasan ─────────────────────────────
                                    <div class="mrv-summary">
                                        <div class="mrv-big">
                                            <span class="mrv-avg">{format!("{:.1}", d.avg)}</span>
                                            <div class="mrv-big-side">
                                                <Stars rating=d.avg />
                                                <span class="mrv-outof">"dari 5"</span>
                                            </div>
                                        </div>
                                        <p class="mrv-total">
                                            {super::merchant_public::fmt_count(total)}" ulasan"
                                        </p>

                                        <div class="mrv-dist">
                                            {(0..5)
                                                .rev()
                                                .map(|i| {
                                                    let cnt = d.dist[i];
                                                    let pct = if total > 0 { cnt * 100 / total } else { 0 };
                                                    // i = index dist (0 = bintang 1) → render 5..1.
                                                    view! {
                                                        <div class="mrv-dist-row">
                                                            <span class="mrv-dist-star">{(i + 1).to_string()}</span>
                                                            <div class="mrv-dist-bar">
                                                                <div
                                                                    class="mrv-dist-fill"
                                                                    style=format!(
                                                                        "width:{}%",
                                                                        pct.max(if cnt > 0 { 3 } else { 0 }),
                                                                    )
                                                                ></div>
                                                            </div>
                                                            <span class="mrv-dist-cnt">
                                                                {super::merchant_public::fmt_count(cnt)}
                                                            </span>
                                                        </div>
                                                    }
                                                })
                                                .collect_view()}
                                        </div>
                                    </div>

                                    // ── Daftar ulasan ─────────────────────────
                                    <div class="mrv-list-head">
                                        <span class="medit-section-label">"ULASAN TERBARU"</span>
                                    </div>
                                    {if d.items.is_empty() {
                                        view! {
                                            <p class="mp-empty">
                                                "Belum ada ulasan. Jadilah yang pertama!"
                                            </p>
                                        }
                                            .into_any()
                                    } else {
                                        let share_store = d.store_name.clone();
                                        view! {
                                            <div class="mrv-list">
                                                {d
                                                    .items
                                                    .iter()
                                                    .map(|r| {
                                                        let share_store = share_store.clone();
                                                        let initial: String = r
                                                            .user_name
                                                            .chars()
                                                            .next()
                                                            .unwrap_or('P')
                                                            .to_uppercase()
                                                            .to_string();
                                                        let date = r.created_at.format("%d %b %Y").to_string();
                                                        let user_href = format!("/u/{}", r.user_id);
                                                        view! {
                                                            <div class="mrv-item">
                                                                <div class="mrv-item-head">
                                                                    <a class="mrv-item-avatar" href=user_href.clone()>
                                                                        {initial}
                                                                    </a>
                                                                    <a class="mrv-item-who mrv-item-who--link" href=user_href>
                                                                        <span class="mrv-item-name">{r.user_name.clone()}</span>
                                                                        <span class="mrv-item-date">{date}</span>
                                                                    </a>
                                                                    <Stars rating=r.rating as f64 />
                                                                    // Bagikan ulasan ini sebagai STORY
                                                                    // (kartu rating + kutipan, tap-through
                                                                    // ke /m/{id} via konvensi slug "m/…").
                                                                    <button
                                                                        class="mrv-share-btn"
                                                                        aria-label="Bagikan ulasan sebagai story"
                                                                        title="Jadikan story"
                                                                        on:click={
                                                                            let rating = r.rating;
                                                                            let comment = r.comment.clone();
                                                                            let store = share_store.clone();
                                                                            move |_| {
                                                                                #[cfg(target_arch = "wasm32")]
                                                                                {
                                                                                    let params = web_sys::UrlSearchParams::new()
                                                                                        .expect("UrlSearchParams");
                                                                                    params.append("merchant", "1");
                                                                                    params.append(
                                                                                        "event_slug",
                                                                                        &format!("m/{}", mid()),
                                                                                    );
                                                                                    params.append("event_title", &store);
                                                                                    params.append("event_cover", "");
                                                                                    params.append(
                                                                                        "review_rating",
                                                                                        &rating.to_string(),
                                                                                    );
                                                                                    params.append("review_comment", &comment);
                                                                                    if let Some(win) = web_sys::window() {
                                                                                        let _ = win.location().assign(
                                                                                            &format!("/story?{}", params.to_string()),
                                                                                        );
                                                                                    }
                                                                                }
                                                                                #[cfg(not(target_arch = "wasm32"))]
                                                                                {
                                                                                    let _ = (&rating, &comment, &store);
                                                                                }
                                                                            }
                                                                        }
                                                                    >
                                                                        <svg
                                                                            width="15"
                                                                            height="15"
                                                                            viewBox="0 0 24 24"
                                                                            fill="none"
                                                                            stroke="currentColor"
                                                                            stroke-width="2"
                                                                            stroke-linecap="round"
                                                                        >
                                                                            <circle cx="12" cy="12" r="9" />
                                                                            <line x1="12" y1="8" x2="12" y2="16" />
                                                                            <line x1="8" y1="12" x2="16" y2="12" />
                                                                        </svg>
                                                                    </button>
                                                                </div>
                                                                {(!r.comment.is_empty())
                                                                    .then(|| {
                                                                        view! { <p class="mrv-item-text">{r.comment.clone()}</p> }
                                                                    })}
                                                            </div>
                                                        }
                                                    })
                                                    .collect_view()}
                                            </div>
                                        }
                                            .into_any()
                                    }}
                                }
                                    .into_any()
                            }
                        }
                    }}
                </Suspense>

                // ── Form tulis ulasan ─────────────────────────────────────────
                <div class="mrv-form">
                    <span class="medit-section-label">"TULIS ULASAN"</span>
                    {move || {
                        // Tampilkan form bila login DAN belum diketahui tak-layak
                        // (Some(false)). Saat masih loading (None) form tampil;
                        // submit tetap divalidasi server. Some(false) → pesan syarat.
                        if is_logged_in() && can_review.get() != Some(false) {
                            view! {
                                <div class="mrv-form-body">
                                    <div class="mrv-pick">
                                        {(1..=5)
                                            .map(|i| {
                                                let star_cls = move || {
                                                    if f_rating.get() >= i {
                                                        "mrv-pick-star mrv-pick-star--on"
                                                    } else {
                                                        "mrv-pick-star"
                                                    }
                                                };
                                                // `>=` TIDAK boleh di dalam atribut view! —
                                                // parser makro memutus tag di `>` (lihat
                                                // catatan variant_editor.rs).
                                                view! {
                                                    <button
                                                        type="button"
                                                        class=star_cls
                                                        on:click=move |_| f_rating.set(i)
                                                        aria-label=format!("{i} bintang")
                                                    >
                                                        "★"
                                                    </button>
                                                }
                                            })
                                            .collect_view()}
                                    </div>
                                    <textarea
                                        class="medit-input medit-textarea"
                                        placeholder="Ceritakan pengalamanmu berbelanja di toko ini..."
                                        prop:value=move || f_comment.get()
                                        on:input=move |e| f_comment.set(event_target_value(&e))
                                    ></textarea>
                                    {move || {
                                        (!form_msg.get().is_empty())
                                            .then(|| {
                                                view! { <p class="mrv-form-msg">{form_msg.get()}</p> }
                                            })
                                    }}
                                    <button
                                        class="medit-submit-btn"
                                        disabled=move || submitting.get()
                                        on:click=do_submit
                                    >
                                        {move || {
                                            if submitting.get() {
                                                "Menyimpan..."
                                            } else {
                                                "KIRIM ULASAN"
                                            }
                                        }}
                                    </button>
                                </div>
                            }
                                .into_any()
                        } else if !is_logged_in() {
                            view! {
                                <p class="mp-empty">
                                    <A href="/login">"Masuk"</A>
                                    " untuk menulis ulasan."
                                </p>
                            }
                                .into_any()
                        } else {
                            view! {
                                <p class="mp-empty">
                                    "Kamu bisa menulis ulasan setelah menyelesaikan (membayar) minimal 1 pesanan dengan penyelenggara ini."
                                </p>
                            }
                                .into_any()
                        }
                    }}
                </div>
            </div>
        </div>
    }
}
