//! chat_new_user.rs — Mulai chat dengan pengguna lain soal barang Pasar
//! (`/pulse/pengguna/:user_id?post=<id>`). Adaptasi `chat_new.rs` (yang
//! menyapa TOKO) untuk menyapa USER BIASA — konteksnya posting marketplace,
//! bukan produk event.
//!
//! Sama seperti `chat_new.rs`: room BELUM dibuat sampai pesan pertama
//! dikirim (`send_first_dm`), jadi membuka halaman ini lalu berubah pikiran
//! tak meninggalkan jejak apa pun.

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::{use_navigate, use_params_map, use_query_map};

use crate::web::components::IconBack;

#[component]
pub fn ChatNewUserPage() -> impl IntoView {
    let auth = use_context::<crate::web::app::AuthResource>();
    let current_user_id = move || {
        auth.and_then(|a| a.get())
            .and_then(|r| r.ok())
            .flatten()
            .map(|u| u.id)
    };

    let params = use_params_map();
    let query = use_query_map();
    let other_user_id = move || params.read().get("user_id").unwrap_or_default();
    let post_id = move || query.read().get("post").unwrap_or_default();

    // Kepala: identitas orang yang disapa.
    let profil = Resource::new_blocking(other_user_id, |id| async move {
        if id.is_empty() {
            return Err(ServerFnError::ServerError("user kosong".into()));
        }
        crate::web::api::get_user_public(id).await
    });

    // Konteks barang (opsional — hanya ada bila datang dari halaman detail
    // posting). Berdampingan dengan kotak ketik, tidak menghalanginya.
    let barang = Resource::new(post_id, |id| async move {
        if id.is_empty() {
            return None;
        }
        crate::web::api::get_post_detail(id).await.ok()
    });

    let riwayat = Resource::new(other_user_id, |id| async move {
        if id.is_empty() {
            return None;
        }
        crate::web::api::cari_chat_user(id).await.ok().flatten()
    });

    let draft = RwSignal::new(String::new());
    let mengirim = RwSignal::new(false);
    let galat = RwSignal::new(String::new());

    let kirim = {
        let nav = use_navigate();
        move |_| {
            if mengirim.get_untracked() {
                return;
            }
            let isi = draft.get_untracked().trim().to_string();
            if isi.is_empty() {
                return;
            }

            let judul = barang.get().flatten().map(|p| p.title).unwrap_or_default();
            let pid = post_id();
            let pesan = if judul.is_empty() {
                isi
            } else if pid.is_empty() {
                format!("[{judul}]\n{isi}")
            } else {
                format!("[{judul}] /marketplace/{pid}\n{isi}")
            };

            mengirim.set(true);
            galat.set(String::new());
            let uid = other_user_id();
            let nav = nav.clone();
            leptos::task::spawn_local(async move {
                match crate::web::api::send_first_dm(uid, pesan).await {
                    Ok(room_id) => {
                        draft.set(String::new());
                        nav(
                            &format!("/pulse/{room_id}"),
                            leptos_router::NavigateOptions { replace: true, ..Default::default() },
                        );
                    }
                    Err(e) => galat.set(format!("Gagal mengirim: {e}")),
                }
                mengirim.set(false);
            });
        }
    };

    view! {
        <div class="min-h-screen bg-page flex flex-col">
            <header class="sticky top-0 z-40 flex items-center gap-3 \
                           px-4 py-3 bg-page border-b border-solid border-line-soft">
                <A
                    href="/marketplace"
                    attr:class="flex items-center justify-center w-9 h-9 shrink-0 rounded-full \
                                bg-card border border-solid border-line text-content no-underline \
                                transition-colors hover:bg-card-hover active:scale-95"
                    attr:aria-label="Kembali"
                >
                    <IconBack />
                </A>
                <Suspense fallback=|| view! {
                    <div class="flex flex-1 items-center gap-2.5 min-w-0">
                        <div class="w-9 h-9 shrink-0 rounded-full bg-elevated animate-pulse"/>
                        <div class="h-3.5 w-28 rounded-md bg-elevated animate-pulse"/>
                    </div>
                }>
                    {move || {
                        profil.get().map(|hasil| match hasil {
                            Err(_) => view! {
                                <span class="flex-1 min-w-0 truncate font-title text-base \
                                             tracking-[0.06em] text-content">"Hubungi Penjual"</span>
                            }.into_any(),
                            Ok(u) => {
                                let href = format!("/u/{}", u.user_id);
                                let nama = u.name.clone();
                                view! {
                                    <A href=href attr:class="flex flex-1 items-center gap-2.5 min-w-0 no-underline">
                                        <div class="w-9 h-9 shrink-0 rounded-full bg-elevated"/>
                                        <span class="truncate font-sans text-sm font-bold text-content">{nama}</span>
                                    </A>
                                }.into_any()
                            }
                        })
                    }}
                </Suspense>
            </header>

            {move || {
                barang.get().flatten().map(|p| {
                    let cover = p.images.first().cloned().unwrap_or_default();
                    let href = format!("/marketplace/{}", p.id);
                    view! {
                        <A href=href attr:class="mx-5 mt-4 flex items-center gap-3 p-3 rounded-2xl \
                                    bg-card border border-solid border-line-soft no-underline \
                                    transition-colors hover:bg-card-hover">
                            {if cover.is_empty() {
                                view! { <div class="w-14 h-14 shrink-0 rounded-xl bg-elevated"/> }.into_any()
                            } else {
                                view! { <img src=cover alt=p.title.clone() loading="lazy" decoding="async"
                                             class="w-14 h-14 shrink-0 rounded-xl object-cover"/> }.into_any()
                            }}
                            <span class="flex flex-col min-w-0">
                                <span class="text-[10px] tracking-[0.08em] text-content-muted">"MENANYAKAN"</span>
                                <span class="truncate text-[13px] font-semibold text-content">{p.title.clone()}</span>
                            </span>
                        </A>
                    }
                })
            }}

            <div class="flex-1 px-5 pt-4">
                <Suspense fallback=|| view! {
                    <p class="text-[12px] text-content-muted">"Tanya kondisi, harga, atau janji ketemu COD."</p>
                }>
                    {move || match riwayat.get().flatten() {
                        None => view! {
                            <p class="text-[12px] text-content-muted">"Tanya kondisi, harga, atau janji ketemu COD."</p>
                        }.into_any(),
                        Some((room_id, pesan)) => {
                            let me = current_user_id().unwrap_or_default();
                            view! {
                                <div class="mb-3 flex items-center justify-between gap-3">
                                    <span class="text-[10px] tracking-[0.08em] text-content-muted">"PERCAKAPAN SEBELUMNYA"</span>
                                    <A href=format!("/pulse/{room_id}") attr:class="text-[11px] font-semibold text-brand no-underline">"Buka semua"</A>
                                </div>
                                <div class="flex flex-col gap-1.5">
                                    {pesan.into_iter().map(|m| {
                                        let sendiri = m.sender_id == me;
                                        let kelas = if sendiri {
                                            "self-end max-w-[85%] rounded-2xl px-3 py-2 bg-brand text-white text-[13px] leading-snug"
                                        } else {
                                            "self-start max-w-[85%] rounded-2xl px-3 py-2 bg-card border border-solid border-line-soft text-content text-[13px] leading-snug"
                                        };
                                        view! { <span class=kelas>{m.content.clone()}</span> }
                                    }).collect_view()}
                                </div>
                            }.into_any()
                        }
                    }}
                </Suspense>
            </div>

            {move || {
                let pesan = galat.get();
                (!pesan.is_empty()).then(|| view! { <p class="px-5 pb-2 text-[12px] text-danger">{pesan}</p> })
            }}

            <div class="sticky bottom-0 flex items-end gap-2 px-4 \
                        pt-3 pb-[calc(12px+env(safe-area-inset-bottom,8px))] \
                        bg-page border-t border-solid border-line-soft">
                <textarea
                    class="flex-1 min-h-11 max-h-32 px-3.5 py-2.5 rounded-2xl resize-none \
                           bg-card border border-solid border-line text-content \
                           text-sm placeholder:text-content-muted"
                    rows="1"
                    placeholder="Tulis pesan..."
                    prop:value=move || draft.get()
                    on:input=move |e| draft.set(event_target_value(&e))
                />
                <button
                    class="inline-flex items-center justify-center w-11 h-11 shrink-0 rounded-full \
                           bg-brand text-on-brand border-0 cursor-pointer \
                           transition-opacity hover:opacity-90 disabled:opacity-50"
                    disabled=move || mengirim.get() || draft.get().trim().is_empty()
                    aria-label="Kirim"
                    on:click=kirim
                >
                    {move || if mengirim.get() {
                        view! {
                            <svg class="animate-spin w-5 h-5" viewBox="0 0 24 24" fill="none" aria-hidden="true">
                                <circle cx="12" cy="12" r="10" stroke="currentColor" stroke-width="3" opacity="0.3" />
                                <path d="M22 12a10 10 0 0 0-10-10" stroke="currentColor" stroke-width="3" stroke-linecap="round" />
                            </svg>
                        }.into_any()
                    } else {
                        view! {
                            <svg width="19" height="19" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                                <line x1="22" y1="2" x2="11" y2="13" />
                                <polygon points="22 2 15 22 11 13 2 9 22 2" />
                            </svg>
                        }.into_any()
                    }}
                </button>
            </div>
        </div>
    }
}
