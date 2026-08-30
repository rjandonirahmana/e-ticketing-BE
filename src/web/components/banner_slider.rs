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
        if n == 0 {
            return;
        }
        manual.set(true);
        idx.update(|i| {
            *i = if maju { (*i + 1) % n } else { (*i + n - 1) % n };
        });
    };

    view! {
        {move || {
            let list = banners.get();
            if list.is_empty() {
                return fallback.run();
            }
            let n = list.len();
            view! {
                <div class="exp-bnr">
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
