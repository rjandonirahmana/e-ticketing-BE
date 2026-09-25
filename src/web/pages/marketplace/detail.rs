//! web/pages/marketplace/detail.rs — `/marketplace/:id`: detail satu posting
//! Pasar (galeri foto, deskripsi, suka, komentar, hubungi penjual / aksi
//! pemilik).

use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::components::A;
use leptos_router::hooks::{use_navigate, use_params_map};

use crate::web::components::{BottomNav, IconBack};
use crate::web::models::{format_date, PostComment};

#[component]
pub fn PostDetailPage() -> impl IntoView {
    let params = use_params_map();
    let post_id = move || params.read().get("id").unwrap_or_default();

    let auth = use_context::<crate::web::app::AuthResource>();
    let current_user_id = move || {
        auth.and_then(|a| a.get())
            .and_then(|r| r.ok())
            .flatten()
            .map(|u| u.id)
    };

    let post_res = Resource::new_blocking(post_id, |id| async move {
        if id.is_empty() {
            return Err(ServerFnError::ServerError("id kosong".into()));
        }
        crate::web::api::get_post_detail(id).await
    });

    // Suka: state lokal dituntun oleh respons resource pertama kali, lalu
    // dipegang sendiri (optimistic) begitu tombolnya ditekan — supaya klik
    // beruntun tidak menunggu bolak-balik server tiap kali.
    let liked = RwSignal::new(false);
    let like_count = RwSignal::new(0i32);
    let synced_once = RwSignal::new(false);
    Effect::new(move |_| {
        if synced_once.get_untracked() {
            return;
        }
        if let Some(Ok(p)) = post_res.get() {
            liked.set(p.liked_by_viewer.unwrap_or(false));
            like_count.set(p.like_count);
            synced_once.set(true);
        }
    });

    let toggle_like = move |_| {
        if current_user_id().is_none() {
            return;
        }
        let id = post_id();
        // Optimistic dulu — tombol suka harus terasa seketika.
        let was = liked.get_untracked();
        liked.set(!was);
        like_count.update(|c| *c += if was { -1 } else { 1 });
        leptos::task::spawn_local(async move {
            match crate::web::api::toggle_like(id).await {
                Ok((l, c)) => {
                    liked.set(l);
                    like_count.set(c);
                }
                Err(_) => {
                    // Gagal → kembalikan seperti semula.
                    liked.set(was);
                    like_count.update(|c| *c += if was { 1 } else { -1 });
                }
            }
        });
    };

    // ── Komentar ─────────────────────────────────────────────────────────
    let comments = RwSignal::new(Vec::<PostComment>::new());
    let comments_page = RwSignal::new(1i64);
    let comments_has_more = RwSignal::new(false);
    let comments_loading_more = RwSignal::new(false);
    let comment_draft = RwSignal::new(String::new());
    let sending_comment = RwSignal::new(false);

    let comments_res = Resource::new(post_id, |id| async move {
        if id.is_empty() {
            return None;
        }
        crate::web::api::list_comments(id, Some(1)).await.ok()
    });
    Effect::new(move |_| {
        if let Some(Some(pg)) = comments_res.get() {
            comments.set(pg.data);
            comments_page.set(1);
            comments_has_more.set(pg.page < pg.total_pages);
        }
    });

    let load_more_comments = move |_| {
        if comments_loading_more.get_untracked() || !comments_has_more.get_untracked() {
            return;
        }
        comments_loading_more.set(true);
        let id = post_id();
        let next = comments_page.get_untracked() + 1;
        leptos::task::spawn_local(async move {
            if let Ok(pg) = crate::web::api::list_comments(id, Some(next)).await {
                comments_page.set(next);
                comments_has_more.set(pg.page < pg.total_pages);
                comments.update(|v| v.extend(pg.data));
            }
            comments_loading_more.set(false);
        });
    };

    let send_comment = move |_| {
        if sending_comment.get_untracked() {
            return;
        }
        let body = comment_draft.get_untracked().trim().to_string();
        if body.is_empty() {
            return;
        }
        let id = post_id();
        sending_comment.set(true);
        leptos::task::spawn_local(async move {
            if let Ok(c) = crate::web::api::add_comment(id, body).await {
                comments.update(|v| v.insert(0, c));
                comment_draft.set(String::new());
            }
            sending_comment.set(false);
        });
    };

    // ── Aksi pemilik ─────────────────────────────────────────────────────
    // `nav` (leptos_router) BUKAN `Copy` — dipakai di dalam blok Suspense yang
    // dipanggil ULANG tiap resource berubah (harus `FnMut`), jadi tombol di
    // bawah tak boleh MEMINDAHKAN `nav` keluar dari lingkungannya (itu hanya
    // bisa terjadi sekali, dan Suspense-nya berhenti bisa dipanggil lagi).
    // Setiap closure tombol memegang salinannya SENDIRI lewat `.clone()`,
    // dibuat ulang tiap render — persis pola `kirim` di `chat_new.rs`.
    let nav = use_navigate();

    view! {
        <Title text="Detail Barang — Hub PULSE" />
        <div class="page pk-detail-page">
            <header class="page-header pk-header">
                <A href="/marketplace" attr:class="pk-back-btn" attr:aria-label="Kembali">
                    <IconBack />
                </A>
                <span class="page-logo">"DETAIL BARANG"</span>
            </header>

            <Suspense fallback=|| view! {
                <div class="pk-detail-shim">
                    <div class="shim" style="aspect-ratio:1;border-radius:16px"></div>
                </div>
            }>
                {move || {
                    post_res.get().map(|res| match res {
                        Err(_) => view! {
                            <div class="pk-empty">
                                <p>"Postingan tidak ditemukan."</p>
                            </div>
                        }.into_any(),
                        Ok(post) => {
                            let is_owner = current_user_id().as_deref() == Some(post.user_id.as_str());
                            let seller_href = format!("/u/{}", post.user_id);
                            let images = post.images.clone();
                            let kind_label = if post.kind == "jual" { "JUAL" } else { "CARI" };
                            // BUG lama: kelas badge di sini dulu ditulis mati
                            // `pk-badge--jual` — postingan "cari" ikut tampil biru
                            // (warna Jual) alih-alih pink (warna Cari).
                            let kind_badge_cls = if post.kind == "jual" {
                                "pk-badge pk-badge--jual"
                            } else {
                                "pk-badge pk-badge--cari"
                            };
                            let condition_label = post.condition.clone();
                            let price_disp = match post.price {
                                Some(p) if p > 0 => crate::web::models::format_price(p as f64),
                                _ => "Nego".to_string(),
                            };
                            let status = post.status.clone();
                            let image_count = images.len();

                            view! {
                                <div class="pk-detail-gallery">
                                    {if images.is_empty() {
                                        view! { <div class="pk-detail-img pk-img--kosong"></div> }.into_any()
                                    } else {
                                        view! {
                                            <div class="pk-detail-gallery-scroll">
                                                {images.iter().map(|url| view! {
                                                    <img src=url.clone() class="pk-detail-img" alt=post.title.clone() loading="lazy" />
                                                }).collect_view()}
                                            </div>
                                            // Statis "1/N" (bukan diturunkan dari posisi gulir) — cukup
                                            // memberi tahu ADA beberapa foto tanpa signal+listener scroll
                                            // tambahan untuk sesuatu yang sifatnya hint kecil.
                                            {(image_count > 1).then(|| view! {
                                                <span class="pk-detail-count">"1 / "{image_count}</span>
                                            })}
                                        }.into_any()
                                    }}
                                </div>

                                <div class="pk-detail-body">
                                    <div class="pk-detail-badges">
                                        <span class=kind_badge_cls><span class="pk-badge-dot"></span>{kind_label}</span>
                                        {condition_label.map(|c| {
                                            let label = if c == "baru" { "Baru" } else { "Bekas" };
                                            view! { <span class="pk-badge pk-badge--kondisi">{label}</span> }
                                        })}
                                        {(status != "active").then(|| {
                                            let label = if status == "sold" { "TERJUAL" } else { "DIARSIPKAN" };
                                            view! { <span class="pk-badge pk-badge--status">{label}</span> }
                                        })}
                                    </div>
                                    <h1 class="pk-detail-title">{post.title.clone()}</h1>
                                    <p class="pk-detail-price">
                                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                                             stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                            <circle cx="12" cy="12" r="9" />
                                            <path d="M12 7v10M9.5 9.5a2.5 2.5 0 012.5-1.5h.5a2 2 0 010 4h-1a2 2 0 000 4h.5a2.5 2.5 0 002.5-1.5" />
                                        </svg>
                                        {price_disp}
                                    </p>
                                    {post.city.clone().filter(|c| !c.is_empty()).map(|c| view! {
                                        <p class="pk-detail-city">
                                            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                                                 stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                                <path d="M21 10c0 7-9 12-9 12s-9-5-9-12a9 9 0 0118 0z" />
                                                <circle cx="12" cy="10" r="3" />
                                            </svg>
                                            {c}
                                        </p>
                                    })}
                                    <p class="pk-detail-desc">{post.description.clone()}</p>
                                    <p class="pk-detail-time">
                                        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                                             stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                            <circle cx="12" cy="12" r="9" />
                                            <path d="M12 7v5l3 3" />
                                        </svg>
                                        {"Diposting "}{format_date(&post.created_at)}
                                    </p>

                                    <p class="pk-seller-label">"Diposting Oleh"</p>
                                    <A href=seller_href attr:class="pk-seller-card">
                                        <div class="pk-seller-avatar">
                                            {if post.user_avatar.is_empty() {
                                                view! { <div class="pk-seller-avatar-fallback"></div> }.into_any()
                                            } else {
                                                view! { <img src=post.user_avatar.clone() alt="" /> }.into_any()
                                            }}
                                        </div>
                                        <div class="pk-seller-meta">
                                            <span class="pk-seller-name">{post.user_name.clone()}</span>
                                            <span class="pk-seller-hint">"Lihat profil"</span>
                                        </div>
                                        <svg class="pk-seller-chevron" width="16" height="16" viewBox="0 0 24 24" fill="none"
                                             stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round">
                                            <path d="M9 6l6 6-6 6" />
                                        </svg>
                                    </A>

                                    <div class="pk-detail-actions">
                                        <button class="pk-like-btn" class:pk-like-btn--on=move || liked.get() on:click=toggle_like>
                                            <svg width="18" height="18" viewBox="0 0 24 24" fill=move || if liked.get() { "currentColor" } else { "none" }
                                                 stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                                <path d="M20.8 4.6a5.5 5.5 0 00-7.8 0L12 5.6l-1-1a5.5 5.5 0 00-7.8 7.8l1 1L12 21l7.8-7.6 1-1a5.5 5.5 0 000-7.8z" />
                                            </svg>
                                            {move || like_count.get()}
                                        </button>

                                        {if is_owner {
                                            let nav_delete = nav.clone();
                                            let id_sold = post_id();
                                            let id_archive = post_id();
                                            let id_delete = post_id();
                                            view! {
                                                <div class="pk-owner-actions">
                                                    <button class="pk-owner-btn" on:click=move |_| {
                                                        let id = id_sold.clone();
                                                        leptos::task::spawn_local(async move {
                                                            if crate::web::api::update_post_status(id, "sold".to_string()).await.is_ok() {
                                                                post_res.refetch();
                                                            }
                                                        });
                                                    }>"Tandai Terjual"</button>
                                                    <button class="pk-owner-btn" on:click=move |_| {
                                                        let id = id_archive.clone();
                                                        leptos::task::spawn_local(async move {
                                                            if crate::web::api::update_post_status(id, "archived".to_string()).await.is_ok() {
                                                                post_res.refetch();
                                                            }
                                                        });
                                                    }>"Arsipkan"</button>
                                                    <button class="pk-owner-btn pk-owner-btn--danger" on:click=move |_| {
                                                        let nav = nav_delete.clone();
                                                        let id = id_delete.clone();
                                                        leptos::task::spawn_local(async move {
                                                            if crate::web::api::delete_post(id).await.is_ok() {
                                                                nav("/marketplace", Default::default());
                                                            }
                                                        });
                                                    }>"Hapus"</button>
                                                </div>
                                            }.into_any()
                                        } else {
                                            let href = format!("/pulse/pengguna/{}?post={}", post.user_id, post.id);
                                            view! {
                                                <A href=href attr:class="pk-contact-btn">"Hubungi Penjual"</A>
                                            }.into_any()
                                        }}
                                    </div>
                                </div>
                            }.into_any()
                        }
                    })
                }}
            </Suspense>

            <div class="pk-comments">
                <h2 class="pk-comments-title">"Komentar"</h2>
                {move || {
                    if current_user_id().is_some() {
                        view! {
                            <div class="pk-comment-composer">
                                <textarea
                                    class="pk-comment-input"
                                    rows="1"
                                    placeholder="Tulis komentar..."
                                    prop:value=move || comment_draft.get()
                                    on:input=move |e| comment_draft.set(event_target_value(&e))
                                />
                                <button
                                    class="pk-comment-send"
                                    disabled=move || sending_comment.get() || comment_draft.get().trim().is_empty()
                                    on:click=send_comment
                                >"Kirim"</button>
                            </div>
                        }.into_any()
                    } else {
                        ().into_any()
                    }
                }}
                <div class="pk-comment-list">
                    {move || comments.get().into_iter().map(|c| {
                        let is_mine = current_user_id().as_deref() == Some(c.user_id.as_str());
                        let delete_id = c.id.clone();
                        let avatar = c.user_avatar.clone();
                        view! {
                            <div class="pk-comment-row">
                                <div class="pk-comment-avatar">
                                    {if avatar.is_empty() {
                                        ().into_any()
                                    } else {
                                        view! { <img src=avatar alt="" /> }.into_any()
                                    }}
                                </div>
                                <div class="pk-comment-main">
                                    <span class="pk-comment-name">{c.user_name.clone()}</span>
                                    <span class="pk-comment-body">{c.body.clone()}</span>
                                </div>
                                {is_mine.then(|| {
                                    let delete_id = delete_id.clone();
                                    view! {
                                        <button
                                            class="pk-comment-delete"
                                            attr:aria-label="Hapus komentar"
                                            on:click=move |_| {
                                                let id = delete_id.clone();
                                                leptos::task::spawn_local(async move {
                                                    if crate::web::api::delete_comment(id.clone()).await.is_ok() {
                                                        comments.update(|v| v.retain(|x| x.id != id));
                                                    }
                                                });
                                            }
                                        >"Hapus"</button>
                                    }
                                })}
                            </div>
                        }
                    }).collect_view()}
                </div>
                {move || comments_has_more.get().then(|| view! {
                    <button class="pk-comment-more" on:click=load_more_comments>
                        {if comments_loading_more.get() { "Memuat..." } else { "Muat lebih banyak" }}
                    </button>
                })}
            </div>

            <BottomNav active="marketplace" />
        </div>
    }
}
