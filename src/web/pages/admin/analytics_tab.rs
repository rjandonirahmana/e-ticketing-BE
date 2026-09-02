use leptos::prelude::*;

use crate::web::models::{format_price, AdminStats, Product};
use crate::web::status::{fmt_bytes, peringatan_disk, tingkat_pakai, StatusServer};

pub(super) fn view_analytics_admin(evs: Vec<Product>, stats: Option<AdminStats>) -> impl IntoView {
    let total        = evs.len();
    let active_count = evs.iter().filter(|e| e.status == "active").count();
    let top          = evs.iter().max_by_key(|e| e.total_sold).cloned();

    view! {
        {stats.map(|s| view! {
            <section class="merchant-stats">
                <div class="merchant-tile-row" style="padding:0 16px;margin-bottom:12px">
                    <div class="merchant-tile">
                        <span class="merchant-label">"TOTAL USER"</span>
                        <span class="merchant-tile-value">{s.total_users}</span>
                    </div>
                    <div class="merchant-tile merchant-tile--accent">
                        <span class="merchant-label">"TOTAL PRODUCT"</span>
                        <span class="merchant-tile-value">{s.total_products}</span>
                    </div>
                </div>
                <div class="merchant-tile-row" style="padding:0 16px">
                    <div class="merchant-tile">
                        <span class="merchant-label">"TOTAL ORDER"</span>
                        <span class="merchant-tile-value">{s.total_orders}</span>
                    </div>
                    <div class="merchant-tile merchant-tile--accent">
                        <span class="merchant-label">"REVENUE"</span>
                        <span class="merchant-tile-value">{format_price(s.total_revenue)}</span>
                    </div>
                </div>
            </section>
        })}
        <section class="merchant-stats">
            <div class="merchant-card merchant-velocity" style="margin-bottom:12px">
                <h3 class="merchant-section-title">"Product Terlaris (Platform)"</h3>
                {if let Some(t) = top {
                    let pct = if t.total_quota > 0 {
                        ((t.total_sold as f64 / t.total_quota as f64) * 100.0).round() as u32
                    } else { 0 };
                    let title = t.name.clone();
                    let sold  = t.total_sold;
                    let quota = t.total_quota;
                    view! {
                        <div style="margin-top:10px">
                            <p style="font-size:13px;font-weight:600;margin-bottom:6px">{title}</p>
                            <div style="display:flex;justify-content:space-between;margin-bottom:4px">
                                <span class="merchant-label">{format!("{sold} terjual")}</span>
                                <span class="merchant-label">{format!("{pct}%")}</span>
                            </div>
                            <div style="background:var(--bg-elevated);height:6px;border-radius:3px;overflow:hidden">
                                <div style=format!(
                                    "width:{pct}%;background:var(--accent-lime);height:6px;border-radius:3px"
                                )></div>
                            </div>
                            <span class="merchant-label" style="margin-top:4px;display:block">
                                {format!("{quota} total kuota")}
                            </span>
                        </div>
                    }.into_any()
                } else {
                    view! {
                        <p class="merchant-label" style="margin-top:8px">"Belum ada data."</p>
                    }.into_any()
                }}
            </div>
            <div class="merchant-tile-row" style="padding:0 16px">
                <div class="merchant-tile">
                    <span class="merchant-label">"TOTAL PRODUCT"</span>
                    <span class="merchant-tile-value">{total}</span>
                </div>
                <div class="merchant-tile merchant-tile--accent">
                    <span class="merchant-label">"PRODUCT AKTIF"</span>
                    <span class="merchant-tile-value">{active_count}</span>
                </div>
            </div>
        </section>

        <StatusServerCard />
    }
}

pub(super) fn view_finance_admin(evs: Vec<Product>) -> impl IntoView {
    let total_sold  = evs.iter().map(|e| e.total_sold).sum::<i32>();
    let total_quota = evs.iter().map(|e| e.total_quota).sum::<i32>();
    let remaining   = (total_quota - total_sold).max(0);
    let total_products = evs.len();
    let live_count  = evs.iter().filter(|e| e.status == "active").count();

    view! {
        <section class="merchant-stats">
            <div class="merchant-card merchant-card--earnings">
                <span class="merchant-label">"BARANG TERJUAL (PLATFORM)"</span>
                <h2 class="merchant-amount">{total_sold}</h2>
                <div class="merchant-trend-row">
                    <span class="merchant-trend-meta">"Sisa: "{remaining}</span>
                    <span class="merchant-trend-meta merchant-trend-meta--right">
                        "Kuota: "{total_quota}
                    </span>
                </div>
            </div>
        </section>
        <section class="merchant-card merchant-velocity">
            <h3 class="merchant-section-title">"Ringkasan Platform"</h3>
            <div class="merchant-tile-row" style="margin-top:12px">
                <div class="merchant-tile">
                    <span class="merchant-label">"TOTAL PRODUCT"</span>
                    <span class="merchant-tile-value">{total_products}</span>
                </div>
                <div class="merchant-tile merchant-tile--accent">
                    <span class="merchant-label">"LANGSUNG"</span>
                    <span class="merchant-tile-value">{live_count}</span>
                </div>
            </div>
        </section>
    }
}

pub(super) fn view_settings_admin() -> impl IntoView {
    view! {
        <section class="merchant-card merchant-velocity">
            <h3 class="merchant-section-title">"Keamanan Akun"</h3>
            <div class="mhub-security-actions">
                <button class="mhub-security-btn">"🔒  Ganti Kata Sandi"</button>
                <button class="mhub-security-btn">
                    "📱  Aktifkan 2FA  "
                    <span class="mhub-security-badge mhub-security-badge--off">"MATI"</span>
                </button>
            </div>
        </section>
        <section class="merchant-card merchant-velocity">
            <h3 class="merchant-section-title">"Notifikasi Admin"</h3>
            {[
                ("Product Baru Didaftarkan", true),
                ("Transaksi Platform", true),
                ("Laporan Harian", false),
            ]
            .iter()
            .map(|(l, c)| {
                view! {
                    <div class="mhub-toggle-row">
                        <span class="mhub-toggle-label">{*l}</span>
                        <label class="mhub-toggle-switch">
                            <input type="checkbox" prop:checked=*c/>
                            <span class="mhub-toggle-track"></span>
                        </label>
                    </div>
                }
            })
            .collect_view()}
        </section>
    }
}


// ── Status server ─────────────────────────────────────────────────────────────

/// Kartu kesehatan mesin, dimuat SAAT DIMINTA.
///
/// Bukan ikut dimuat bersama tab: pembacaan CPU menuntut dua cuplikan
/// `/proc/stat` berjarak 300 ms, dan membebankan jeda itu pada setiap orang
/// yang kebetulan membuka Analitik berarti membayar mahal untuk angka yang
/// jarang ditanyakan. Ia ditanyakan justru saat ada yang dicurigai.
#[component]
pub(super) fn StatusServerCard() -> impl IntoView {
    let data: RwSignal<Option<StatusServer>> = RwSignal::new(None);
    let memuat = RwSignal::new(false);
    let galat: RwSignal<Option<String>> = RwSignal::new(None);

    let ambil = move |_| {
        if memuat.get_untracked() {
            return;
        }
        memuat.set(true);
        galat.set(None);
        leptos::task::spawn_local(async move {
            match crate::web::api::status_server_admin().await {
                Ok(s) => data.set(Some(s)),
                Err(e) => galat.set(Some(e.to_string())),
            }
            memuat.set(false);
        });
    };

    view! {
        <section class="srv-status">
            <div class="srv-head">
                <h3 class="merchant-label">"STATUS SERVER"</h3>
                <button class="srv-btn" disabled=move || memuat.get() on:click=ambil>
                    {move || if memuat.get() {
                        "MEMBACA…"
                    } else if data.get().is_some() {
                        "SEGARKAN"
                    } else {
                        "CEK STATUS"
                    }}
                </button>
            </div>

            {move || galat.get().map(|e| view! {
                <p class="srv-galat">{e}</p>
            })}

            {move || data.get().map(|s| {
                let (cpu_label, cpu_kelas) = tingkat_pakai(s.cpu_pct);
                let (mem_label, mem_kelas) = tingkat_pakai(s.mem_pct);
                // Kolam koneksi: nol menganggur terus-menerus adalah gejala
                // paling awal halaman lambat — dan persis keadaan yang pernah
                // menjatuhkan situs ini.
                let pool_kritis = s.pool_idle == 0 && s.pool_size >= s.pool_max;
                view! {
                    {(!s.tersedia).then(|| view! {
                        <p class="srv-catatan">{s.catatan.clone()}</p>
                    })}

                    <div class="srv-grid">
                        <div class="srv-kartu">
                            <span class="srv-judul">"CPU"</span>
                            <span class=format!("srv-nilai {cpu_kelas}")>
                                {format!("{:.0}%", s.cpu_pct)}
                            </span>
                            <span class="srv-sub">
                                {format!("{cpu_label} · {} inti", s.cpu_cores)}
                            </span>
                            // Beban BUKAN persen: 2,0 pada mesin 2 inti berarti
                            // antrean tepat penuh, pada 4 inti berarti setengah.
                            <span class="srv-sub">
                                {format!("beban {:.2} / {:.2} / {:.2}", s.load1, s.load5, s.load15)}
                            </span>
                        </div>

                        <div class="srv-kartu">
                            <span class="srv-judul">"MEMORI"</span>
                            <span class=format!("srv-nilai {mem_kelas}")>
                                {format!("{:.0}%", s.mem_pct)}
                            </span>
                            <span class="srv-sub">
                                {format!("{mem_label} · {} / {}",
                                    fmt_bytes(s.mem_terpakai), fmt_bytes(s.mem_total))}
                            </span>
                            <span class="srv-sub">{s.mem_sumber.clone()}</span>
                            <span class="srv-sub">
                                {format!("aplikasi ini {}", fmt_bytes(s.app_rss))}
                            </span>
                            {(s.swap_total > 0).then(|| view! {
                                <span class="srv-sub">
                                    {format!("swap {} / {}",
                                        fmt_bytes(s.swap_terpakai), fmt_bytes(s.swap_total))}
                                </span>
                            })}
                        </div>

                        <div class=move || if pool_kritis { "srv-kartu srv-kartu--awas" } else { "srv-kartu" }>
                            <span class="srv-judul">"KOLAM DB"</span>
                            <span class="srv-nilai">
                                {format!("{} / {}", s.pool_size, s.pool_max)}
                            </span>
                            <span class="srv-sub">{format!("{} menganggur", s.pool_idle)}</span>
                            {pool_kritis.then(|| view! {
                                <span class="srv-awas">
                                    "Tak ada koneksi menganggur — permintaan sedang mengantre."
                                </span>
                            })}
                        </div>

                        <div class="srv-kartu">
                            <span class="srv-judul">"WAKTU HIDUP"</span>
                            <span class="srv-nilai srv-nilai--kecil">{s.uptime_app.clone()}</span>
                            <span class="srv-sub">"aplikasi"</span>
                            <span class="srv-sub">
                                {format!("mesin {}", s.uptime_mesin)}
                            </span>
                        </div>
                    </div>

                    // Latensi jalur panas. Persentil, bukan rata-rata: saat
                    // situs ini jatuh, rata-ratanya tetap tampak wajar karena
                    // sebagian besar permintaan adalah aset statis yang cepat.
                    // Yang rusak adalah ekornya, dan hanya p95/p99 yang
                    // memperlihatkannya.
                    <div class="srv-kartu srv-kartu--lebar">
                        <span class="srv-judul">"LATENSI (p50 / p95 / p99)"</span>
                        {s.latensi.iter().map(|l| {
                            let angka = match (l.p50, l.p95, l.p99) {
                                (Some(a), Some(b), Some(c)) => {
                                    format!("≤{a} / ≤{b} / ≤{c} ms · {} kali", l.jumlah)
                                }
                                // "Belum ada data" BUKAN "nol milidetik".
                                _ => "belum ada data".to_string(),
                            };
                            view! {
                                <span class="srv-sub">
                                    {format!("{}: {angka}", l.nama)}
                                </span>
                            }
                        }).collect_view()}
                        <span class="srv-sub">
                            {format!("pesan dibuang {} · sesi diganti {}",
                                s.pesan_dibuang, s.sesi_diganti)}
                        </span>
                    </div>

                    // Penyimpanan: angka besarnya SISA, bukan yang terpakai.
                    // Disk penuh membuat Postgres berhenti menerima tulisan, dan
                    // itu tak pulih sendiri setelah restart.
                    {s.disk.iter().map(|d| {
                        let awas = peringatan_disk(d.tersedia, d.pct);
                        let (_, kelas) = tingkat_pakai(d.pct);
                        view! {
                            <div class="srv-kartu srv-kartu--lebar">
                                <span class="srv-judul">{d.label.clone()}</span>
                                <span class=format!("srv-nilai {kelas}")>
                                    {format!("{} sisa", fmt_bytes(d.tersedia))}
                                </span>
                                <span class="srv-sub">
                                    {format!("{} dari {} terpakai ({:.0}%)",
                                        fmt_bytes(d.terpakai), fmt_bytes(d.total), d.pct)}
                                </span>
                                <span class="srv-sub">{d.path.clone()}</span>
                                {awas.map(|a| view! { <span class="srv-awas">{a}</span> })}
                            </div>
                        }
                    }).collect_view()}
                }
            })}
        </section>
    }
}
