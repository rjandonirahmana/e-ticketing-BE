use leptos::prelude::*;
use leptos_router::components::A;

use crate::web::api::{delete_product_admin, update_product_status_admin};
use crate::web::models::{format_date, format_price, Product};

pub(super) use crate::web::components::merchant_dashboard_product::ProductStatus;

/// Daftar seluruh produk platform, dengan kendali terbit/tahan per produk.
///
/// ── KENAPA TOMBOLNYA DI SINI, BUKAN DI TAB REVIEW ────────────────────────
/// Tab Review hanya memuat antrean `edited` — produk yang sedang menunggu
/// tinjauan. Itu menjawab satu arah saja: menyetujui yang masuk antrean.
///
/// Yang tak punya tempat sebelumnya adalah arah sebaliknya: menahan produk yang
/// SUDAH terbit. Backend sebenarnya sudah mengizinkannya sejak lama
/// (`admin_update_status` menerima `edited`), tetapi tak ada satu pun tombol
/// yang memanggilnya, jadi admin hanya bisa membatalkan produk — tindakan yang
/// jauh lebih keras dan sulit ditarik kembali.
///
/// Daftar inilah satu-satunya tempat yang memuat produk pada SEMUA status,
/// sehingga hanya di sini kedua arah itu bisa berdampingan.
pub(super) fn view_all_products(
    evs: Vec<Product>,
    all_products: RwSignal<Vec<Product>>,
    pending_products: RwSignal<Vec<Product>>,
    toast: RwSignal<Option<(String, bool)>>,
) -> impl IntoView {
    let processing: RwSignal<Option<String>> = RwSignal::new(None);

    // Antrean review ikut dijaga di sini, bukan dibiarkan menyusul lewat
    // pemuatan ulang. Menahan produk berarti ia masuk antrean, menerbitkan
    // berarti ia keluar — kalau lencana "Menunggu" di tab sebelah tidak ikut
    // berubah seketika, angkanya berselisih dengan daftar yang baru saja
    // disunting admin, dan yang lebih dipercaya justru yang salah.
    let do_update = move |produk: Product, status: &'static str| {
        let id = produk.id.clone();
        processing.set(Some(id.clone()));
        leptos::task::spawn_local(async move {
            match update_product_status_admin(id.clone(), status.to_string()).await {
                Ok(_) => {
                    all_products.update(|v| {
                        if let Some(e) = v.iter_mut().find(|e| e.id == id) {
                            e.status = status.to_string();
                        }
                    });
                    pending_products.update(|v| {
                        v.retain(|e| e.id != id);
                        if status == "edited" {
                            let mut ditahan = produk.clone();
                            ditahan.status = status.to_string();
                            v.insert(0, ditahan);
                        }
                    });
                    toast.set(Some((
                        match status {
                            "active" => "✅ Product diterbitkan.".to_string(),
                            "edited" => "⏸ Product ditahan — tidak lagi tampil ke pembeli.".to_string(),
                            _ => "Status diperbarui.".to_string(),
                        },
                        false,
                    )));
                }
                Err(e) => toast.set(Some((format!("Gagal: {e}"), true))),
            }
            processing.set(None);
        });
    };

    // Hapus: dikonfirmasi lebih dulu, dan konfirmasinya menyebut NAMA produknya.
    //
    // Dialog `Yakin?` yang polos praktis selalu dijawab OK — ia tak memberi
    // pembacanya satu pun cara memeriksa bahwa yang akan hilang memang yang ia
    // maksud. Di daftar panjang berisi kartu yang mirip satu sama lain, salah
    // tekan satu baris adalah kekeliruan yang paling mudah terjadi dan paling
    // sulit disadari sesudahnya.
    let do_delete = move |slug: String, nama: String| {
        if let Some(win) = web_sys::window() {
            let pesan = format!(
                "Hapus \"{nama}\" dari etalase?\n\nProduct hilang dari pencarian dan \
                 dikeluarkan dari keranjang siapa pun yang sudah memasukkannya. \
                 Pesanan dan tiket yang sudah dibayar TIDAK terpengaruh."
            );
            if !win.confirm_with_message(&pesan).unwrap_or(false) {
                return;
            }
        }
        processing.set(Some(slug.clone()));
        leptos::task::spawn_local(async move {
            match delete_product_admin(slug.clone()).await {
                Ok(_) => {
                    all_products.update(|v| v.retain(|e| e.slug != slug));
                    pending_products.update(|v| v.retain(|e| e.slug != slug));
                    toast.set(Some((format!("🗑 \"{nama}\" dihapus dari etalase."), false)));
                }
                Err(e) => toast.set(Some((format!("Gagal menghapus: {e}"), true))),
            }
            processing.set(None);
        });
    };

    view! {
        <section class="mhub-products-section">
            <div class="mhub-products-header">
                <h3 class="mhub-products-title">"Semua Product Platform"</h3>
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
                        <p class="mhub-empty-title">"Belum Ada Product"</p>
                        <p class="mhub-empty-body">"Belum ada product yang terdaftar di platform."</p>
                    </div>
                }.into_any()
            } else {
                evs.into_iter().map(|ev| {
                    let cover = ev.cover_url.as_deref()
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .unwrap_or_else(|| crate::web::utils::gambar_pengganti(600));
                    let status     = ProductStatus::from_product(&ev);
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
                    let (val_text, val_cls) = if status == ProductStatus::SoldOut {
                        ("100% Habis Terjual".to_string(), "mhub-product-progress-val mhub-product-progress-val--sold")
                    } else if quota == 0 {
                        ("—".to_string(), "mhub-product-progress-val")
                    } else {
                        (format!("{sold}/{quota} Terjual"), "mhub-product-progress-val")
                    };
                    let remaining_text = if quota == 0 { String::new() } else { format!("{avail} sisa") };
                    let fill_cls = match &status {
                        ProductStatus::SoldOut => "mhub-product-progress-fill mhub-product-progress-fill--sold",
                        ProductStatus::Presale => "mhub-product-progress-fill mhub-product-progress-fill--lime",
                        _                    => "mhub-product-progress-fill",
                    };
                    let price      = format_price(ev.display_price);
                    let slug       = ev.slug.clone();

                    // Terbit ⇄ tahan. Tombolnya menunjukkan TINDAKAN yang akan
                    // terjadi, bukan keadaan sekarang — keadaan sudah dibaca
                    // dari lencana di atas gambar, dan tombol yang mengulang
                    // keadaan selalu ambigu soal apa yang terjadi bila ditekan.
                    let sedang_terbit = ev.status == "active";
                    let (aksi, aksi_label, aksi_style) = if sedang_terbit {
                        ("edited", "⏸ Tahan", "background:var(--bg-elevated);color:var(--text-muted)")
                    } else {
                        ("active", "▶ Terbitkan", "background:var(--accent-lime);color:#000;font-weight:700")
                    };
                    let ev_aksi   = ev.clone();
                    let id_proc   = ev.id.clone();
                    let id_lbl    = ev.id.clone();
                    let del_slug  = ev.slug.clone();
                    let del_nama  = ev.name.clone();
                    let del_proc  = ev.slug.clone();
                    let status_css = status.css_mod();
                    let status_lbl = status.label();

                    view! {
                        <div class="mhub-product-card">
                            <div class="mhub-product-card-img-wrap">
                                <img src=cover alt=title.clone() class="mhub-product-card-img"/>
                                <span class=status_css>{status_lbl}</span>
                            </div>
                            <div class="mhub-product-card-body">
                                <div class="mhub-product-card-top-row">
                                    <p class="mhub-product-card-title">{title}</p>
                                    <div class="mhub-product-card-price-block">
                                        <span class="mhub-product-price-label">"Mulai dari"</span>
                                        <span class="mhub-product-price-value">{price}</span>
                                    </div>
                                </div>
                                <p class="mhub-product-card-meta">{date}" • "{venue_str}</p>
                                <div class="mhub-product-progress-section">
                                    <div class="mhub-product-progress-row">
                                        <span class="mhub-product-progress-key">"Penjualan"</span>
                                        <span class=val_cls>{val_text}</span>
                                    </div>
                                    <div class="mhub-product-progress-bar">
                                        <div class=fill_cls style=fill_style></div>
                                    </div>
                                    {(!remaining_text.is_empty()).then(|| view! {
                                        <div class="mhub-product-remaining-row">
                                            <span class="mhub-product-remaining-badge">
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
                                <div class="mhub-product-card-actions">
                                    <A href=format!("/admin/products/{slug}/edit")
                                       attr:class="mhub-product-manage-btn">
                                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none"
                                             stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                            <path d="M11 4H4a2 2 0 00-2 2v14a2 2 0 002 2h14a2 2 0 002-2v-7"/>
                                            <path d="M18.5 2.5a2.121 2.121 0 013 3L12 15l-4 1 1-4 9.5-9.5z"/>
                                        </svg>
                                        "Sunting Product"
                                    </A>
                                    <button
                                        class="mhub-product-manage-btn"
                                        style=aksi_style
                                        disabled=move || processing.with(|p| p.as_deref() == Some(&id_proc))
                                        on:click=move |_| do_update(ev_aksi.clone(), aksi)>
                                        {move || if processing.with(|p| p.as_deref() == Some(&id_lbl)) {
                                            "Memproses..."
                                        } else { aksi_label }}
                                    </button>
                                    <button
                                        class="mhub-product-manage-btn"
                                        style="flex:0 0 auto;padding-inline:12px;color:var(--danger)"
                                        title="Hapus product"
                                        aria-label="Hapus product"
                                        disabled=move || processing.with(|p| p.as_deref() == Some(&del_proc))
                                        on:click=move |_| do_delete(del_slug.clone(), del_nama.clone())>
                                        <svg width="14" height="14" viewBox="0 0 24 24" fill="none"
                                             stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                            <polyline points="3 6 5 6 21 6"/>
                                            <path d="M19 6l-1 14a2 2 0 01-2 2H8a2 2 0 01-2-2L5 6"/>
                                            <path d="M10 11v6M14 11v6"/>
                                            <path d="M9 6V4a1 1 0 011-1h4a1 1 0 011 1v2"/>
                                        </svg>
                                    </button>
                                </div>
                            </div>
                        </div>
                    }
                }).collect_view().into_any()
            }}
        </section>
    }
}
