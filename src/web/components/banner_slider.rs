//! banner_slider.rs — Slider banner (tabel `banners`, dikelola admin).
//!
//! Dulu markup ini hidup langsung di `pages/explore/mod.rs`. Diekstrak karena
//! `/pulse` menampilkannya juga, dan menyalin enam puluh baris markup + logika
//! putar-otomatis ke halaman kedua berarti setiap perbaikan berikutnya harus
//! diingat dua kali — bentuk duplikasi yang paling sering menua jadi dua
//! komponen yang diam-diam berbeda perilaku.
//!
//! Digeser lewat `translateX` pada track, bukan scroll-snap: indeksnya perlu
//! dikendalikan dari luar (putar-otomatis, titik indikator, panah) dan
//! scroll-snap tak memberi kendali itu tanpa JS tambahan.

use leptos::prelude::*;

/// Jeda putar-otomatis (milidetik). Hanya dipakai di jalur hidrasi.
#[cfg(feature = "hydrate")]
const AUTO_MS: u32 = 5_000;

/// Jarak minimum satu gesekan dianggap perpindahan banner (piksel CSS).
///
/// Lebih pendek dari ambang geser tab (48px): banner selebar layar, dan yang
/// digeser adalah satu gambar besar — bukan panel berisi daftar yang juga
/// digulir vertikal. Terlalu panjang di sini membuat geseran wajar terasa
/// tak direspons.
const GESER_MIN: f64 = 40.0;

/// Banner tujuan, MELINGKAR.
///
/// Berbeda dari geser tab (`swipe_tabs::tujuan_geser`) yang sengaja mentok di
/// ujung: di sana ujung menandakan batas daftar yang bermakna. Banner memang
/// berputar — putar-otomatisnya sudah melingkar — jadi geseran yang mentok
/// justru bertentangan dengan yang sudah dilihat orang.
fn tujuan_banner(kini: usize, n: usize, maju: bool) -> Option<usize> {
    if n <= 1 {
        return None;
    }
    Some(if maju { (kini + 1) % n } else { (kini + n - 1) % n })
}

#[component]
pub fn BannerSlider(
    /// Dirender saat belum ada banner aktif. `/explore` memakai kartu
    /// "SPONSORED" lamanya; halaman lain umumnya tak menampilkan apa pun
    /// (bawaan `ViewFn` = kosong).
    #[prop(optional, into)]
    fallback: ViewFn,
) -> impl IntoView {
    let banners = RwSignal::new(Vec::<crate::web::models::Banner>::new());
    let idx = RwSignal::new(0usize);
    // Begitu orang menggeser sendiri, putar-otomatis BERHENTI untuk selamanya.
    // Tanpa ini panah dan timer saling berebut: orang menekan "berikutnya",
    // lalu dua detik kemudian slidenya berpindah sendiri ke tempat lain.
    let manual = RwSignal::new(false);

    #[cfg(feature = "hydrate")]
    {
        use leptos::task::spawn_local;

        Effect::new(move |prev: Option<()>| {
            if prev.is_some() {
                return;
            }
            spawn_local(async move {
                if let Ok(list) = crate::web::api::get_banners().await {
                    banners.set(list);
                }
            });
        });

        // Interval DIPEGANG lalu di-drop saat unmount — tidak bocor.
        let auto = send_wrapper::SendWrapper::new(gloo_timers::callback::Interval::new(
            AUTO_MS,
            move || {
                if manual.get_untracked() {
                    return;
                }
                let n = banners.with_untracked(|b| b.len());
                if n > 1 {
                    idx.update(|i| *i = (*i + 1) % n);
                }
            },
        ));
        let cell: StoredValue<
            Option<send_wrapper::SendWrapper<gloo_timers::callback::Interval>>,
        > = StoredValue::new(Some(auto));
        on_cleanup(move || {
            if let Some(Some(int)) = cell.try_update_value(|o| o.take()) {
                drop(int);
            }
        });
    }

    // Melingkar: dari slide terakhir "berikutnya" kembali ke awal. Berbeda dari
    // geser tab (`swipe_tabs.rs`) yang sengaja mentok di ujung — di sana ujung
    // menandakan batas daftar yang bermakna, di sini banner memang berputar
    // dan putar-otomatisnya sudah melingkar, jadi panah yang mentok justru
    // bertentangan dengan yang sudah dilihat orang.
    let geser = move |maju: bool| {
        let n = banners.with_untracked(|b| b.len());
        let kini = idx.get_untracked();
        let Some(tujuan) = tujuan_banner(kini, n, maju) else {
            return;
        };
        manual.set(true);
        idx.set(tujuan);
    };

    // ── Geser dengan jari ────────────────────────────────────────────────────
    // Trek ini digerakkan `translateX` dari sinyal, BUKAN gulir — jadi tak ada
    // satu pun perilaku bawaan peramban yang bisa menggesernya. Tanpa penangan
    // di bawah, banner di ponsel hanya bisa dipindah lewat titik indikator
    // sebesar 7px atau dengan menunggu putar-otomatis; panahnya sendiri sengaja
    // disembunyikan di layar sentuh (lihat `45-carousel-nav.css`).
    let awal = StoredValue::new((0.0f64, 0.0f64));
    let sumbu: StoredValue<Option<bool>> = StoredValue::new(None);
    // Menandai bahwa gesekan BARU SAJA terjadi, supaya ketukan palsu yang
    // menyusulnya tak ikut membuka tautan slide. Sebagian peramban tetap
    // menembakkan `click` sesudah seret mendatar di atas `<a>`.
    let baru_geser = StoredValue::new(false);

    let on_mulai = move |e: web_sys::TouchEvent| {
        let Some(t) = e.touches().get(0) else { return };
        awal.set_value((t.client_x() as f64, t.client_y() as f64));
        sumbu.set_value(None);
        baru_geser.set_value(false);
    };

    let on_gerak = move |e: web_sys::TouchEvent| {
        if sumbu.get_value().is_some() {
            return;
        }
        let Some(t) = e.touches().get(0) else { return };
        let (ax, ay) = awal.get_value();
        // Sumbu diputuskan SEKALI. Fungsinya dipakai bersama bilah tab
        // (`swipe_tabs::sumbu_horizontal`) supaya ambang "kapan gulir vertikal
        // tak boleh tercuri" hanya hidup di satu tempat — dan sudah teruji.
        sumbu.set_value(crate::web::components::swipe_tabs::sumbu_horizontal(
            t.client_x() as f64 - ax,
            t.client_y() as f64 - ay,
        ));
    };

    let on_selesai = move |e: web_sys::TouchEvent| {
        if sumbu.get_value() != Some(true) {
            return;
        }
        let Some(t) = e.changed_touches().get(0) else { return };
        let dx = t.client_x() as f64 - awal.get_value().0;
        if dx.abs() < GESER_MIN {
            return;
        }
        baru_geser.set_value(true);
        // Geser ke KIRI (dx negatif) = banner berikutnya, mengikuti arah
        // kertas yang ditarik.
        geser(dx < 0.0);
    };

    let on_klik_slide = move |e: web_sys::MouseEvent| {
        if baru_geser.get_value() {
            e.prevent_default();
            baru_geser.set_value(false);
        }
    };

    view! {
        {move || {
            let list = banners.get();
            if list.is_empty() {
                return fallback.run();
            }
            let n = list.len();
            view! {
                <div
                    class="exp-bnr"
                    on:touchstart=on_mulai
                    on:touchmove=on_gerak
                    on:touchend=on_selesai
                    on:touchcancel=on_selesai
                >
                    <div
                        class="exp-bnr-track"
                        style=move || {
                            format!("transform:translateX(-{}%)", idx.get().min(n - 1) * 100)
                        }
                    >
                        {list
                            .iter()
                            .map(|b| {
                                let img = b.image_url.clone();
                                let link = b.link_url.clone().unwrap_or_default();
                                let title = b.title.clone().unwrap_or_default();
                                view! {
                                    <a
                                        class="exp-bnr-slide"
                                        href=if link.is_empty() { "#".into() } else { link }
                                        on:click=on_klik_slide
                                    >
                                        <img src=img alt=title loading="lazy" class="exp-bnr-img" />
                                    </a>
                                }
                            })
                            .collect_view()}
                    </div>

                    {(n > 1)
                        .then(|| {
                            view! {
                                // Panah hanya tampil di perangkat berpenunjuk
                                // presisi (lihat `@media (hover:hover) and
                                // (pointer:fine)` di CSS). Di ponsel jarinya
                                // sudah bisa menggeser dan panah cuma menutupi
                                // gambar; di laptop tak ada yang bisa digeser
                                // sama sekali tanpa ini — track-nya digerakkan
                                // transform, bukan gulir, jadi roda mouse pun
                                // tak berpengaruh.
                                <button
                                    class="exp-bnr-nav exp-bnr-nav--prev"
                                    aria-label="Banner sebelumnya"
                                    on:click=move |_| geser(false)
                                >
                                    <svg
                                        width="18"
                                        height="18"
                                        viewBox="0 0 24 24"
                                        fill="none"
                                        stroke="currentColor"
                                        stroke-width="2.5"
                                        stroke-linecap="round"
                                    >
                                        <polyline points="15 18 9 12 15 6" />
                                    </svg>
                                </button>
                                <button
                                    class="exp-bnr-nav exp-bnr-nav--next"
                                    aria-label="Banner berikutnya"
                                    on:click=move |_| geser(true)
                                >
                                    <svg
                                        width="18"
                                        height="18"
                                        viewBox="0 0 24 24"
                                        fill="none"
                                        stroke="currentColor"
                                        stroke-width="2.5"
                                        stroke-linecap="round"
                                    >
                                        <polyline points="9 18 15 12 9 6" />
                                    </svg>
                                </button>

                                <div class="exp-bnr-dots">
                                    {(0..n)
                                        .map(|i| {
                                            view! {
                                                <button
                                                    class=move || {
                                                        if idx.get() == i {
                                                            "exp-bnr-dot exp-bnr-dot--on"
                                                        } else {
                                                            "exp-bnr-dot"
                                                        }
                                                    }
                                                    aria-label=format!("Banner {}", i + 1)
                                                    on:click=move |_| {
                                                        manual.set(true);
                                                        idx.set(i);
                                                    }
                                                ></button>
                                            }
                                        })
                                        .collect_view()}
                                </div>
                            }
                        })}
                </div>
            }
                .into_any()
        }}
    }
}

// ─── Uji perpindahan banner ───────────────────────────────────────────────────
#[cfg(test)]
mod tests_banner {
    use super::*;

    /// Melingkar ke depan, termasuk dari slide terakhir kembali ke awal —
    /// menyamai putar-otomatis, yang memang sudah melingkar. Kalau geseran
    /// mentok di ujung sementara timer terus berputar, keduanya bertentangan.
    #[test]
    fn maju_melingkar() {
        assert_eq!(tujuan_banner(0, 3, true), Some(1));
        assert_eq!(tujuan_banner(1, 3, true), Some(2));
        assert_eq!(tujuan_banner(2, 3, true), Some(0), "dari terakhir kembali ke awal");
    }

    /// Melingkar ke belakang juga, termasuk dari slide pertama ke terakhir.
    #[test]
    fn mundur_melingkar() {
        assert_eq!(tujuan_banner(2, 3, false), Some(1));
        assert_eq!(tujuan_banner(0, 3, false), Some(2), "dari awal ke terakhir");
    }

    /// Satu banner saja: tak ada tujuan. Tanpa penjagaan ini, geseran akan
    /// "berpindah" ke slide yang sama dan mematikan putar-otomatis
    /// (`manual = true`) tanpa satu pun perubahan yang terlihat.
    #[test]
    fn satu_banner_tak_berpindah() {
        assert_eq!(tujuan_banner(0, 1, true), None);
        assert_eq!(tujuan_banner(0, 1, false), None);
    }

    /// Tanpa banner sama sekali tak boleh meluap saat menghitung `n - 1`.
    #[test]
    fn tanpa_banner_tak_meluap() {
        assert_eq!(tujuan_banner(0, 0, true), None);
        assert_eq!(tujuan_banner(0, 0, false), None);
    }
}
