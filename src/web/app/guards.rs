//! web/app/guards.rs — Route guards berbasis role (client-side UX).
//!
//! CATATAN KEAMANAN: guard ini hanya menyembunyikan UI & redirect — bukan
//! batas keamanan. Otorisasi sebenarnya ditegakkan di server function
//! (`require_role` / `require_roles` di web/api/server_fns).

use leptos::prelude::*;

use super::contexts::AuthResource;

/// Alihkan halaman TANPA menavigasi di tengah pembangunan view.
///
/// ── KENAPA BUKAN `<Redirect/>` ────────────────────────────────────────────
/// Ini akar "klik navigasi, URL berganti, tapi layar tetap di halaman lama
/// sampai di-refresh".
///
/// `leptos_router::components::Redirect` memanggil `navigate(...)` LANGSUNG di
/// badan komponennya (leptos_router 0.8.15, `components.rs:601`) — bukan di
/// dalam efek. Badan komponen itu dijalankan saat router sedang membangun view
/// rute baru, di dalam `view.choose().await`. Jadi navigasi kedua terjadi
/// SEBELUM rute pertama sempat dipasang ke DOM.
///
/// Yang terjadi berikutnya ada di `flat_router.rs:314`:
///
/// ```text
/// if current_url.read_untracked().path() == spawned_path {
///     rebuild();          // ← DILEWATI kalau URL sudah berubah lagi
/// }
/// location.ready_to_complete();   // ← TETAP dijalankan
/// ```
///
/// Router sudah lebih dulu menimpa pembukuannya sendiri (`initial_state.path`,
/// `id`, dan `owner`) di awal `rebuild`, lalu `ready_to_complete()` menyetujui
/// `pushState`. Hasilnya tiga hal yang saling bertentangan: URL menunjuk
/// halaman baru, pembukuan router mengira halaman baru sudah terpasang, tetapi
/// DOM masih halaman lama. Dan karena pembukuannya sudah "benar", navigasi
/// berikutnya ke alamat itu berhenti di penjaga paling atas —
/// `if url_snapshot.path() == initial_state.path { return; }` — sehingga layar
/// TIDAK PERNAH menyusul sampai halaman dimuat ulang. Persis gejalanya.
///
/// `Effect` menutup celah itu: efek berjalan SESUDAH render selesai, jadi rute
/// yang sekarang sudah benar-benar terpasang sebelum navigasi berikutnya
/// dimulai. `replace: true` dipakai supaya halaman yang ditolak tak menumpuk di
/// riwayat — kalau tidak, tombol Back memantul bolak-balik ke halaman yang
/// memang tak boleh dibuka.
///
/// Di SSR `<Redirect/>` tetap yang benar dan tetap dipakai: di sana ia memasang
/// status 302 lewat `ServerRedirectFunction`, sehingga permintaan langsung dan
/// perayap menerima pengalihan sungguhan, bukan halaman kerangka kosong.
#[component]
fn GuardRedirect(path: &'static str) -> impl IntoView {
    #[cfg(feature = "ssr")]
    {
        view! { <leptos_router::components::Redirect path=path /> }.into_any()
    }
    #[cfg(not(feature = "ssr"))]
    {
        let navigate = leptos_router::hooks::use_navigate();
        Effect::new(move |sudah: Option<()>| {
            // Efek ini tak melacak sinyal apa pun, jadi normalnya hanya sekali
            // jalan. Penjaga ini tetap dipasang supaya satu re-run tak berarti
            // dua entri navigasi.
            if sudah.is_none() {
                navigate(
                    path,
                    leptos_router::NavigateOptions {
                        replace: true,
                        ..Default::default()
                    },
                );
            }
        });
        // Kerangka, bukan layar kosong: pengalihan baru terjadi satu frame lagi
        // dan halaman tak boleh berkedip putih selama jeda itu.
        guard_skeleton().into_any()
    }
}

/// Skeleton fallback saat AuthResource masih loading.
fn guard_skeleton() -> impl IntoView {
    view! {
        <div class="page">
            <div style="display:flex;align-items:center;justify-content:space-between;
             padding:14px 16px;border-bottom:1px solid var(--border-soft);
             background:var(--bg-page);position:sticky;top:0;z-index:40">
                <div class="shim" style="width:36px;height:36px;border-radius:50%"></div>
                <div class="shim" style="width:72px;height:18px;border-radius:4px"></div>
                <div class="shim" style="width:36px;height:36px;border-radius:50%"></div>
            </div>
            <div style="padding:20px 16px;display:flex;flex-direction:column;gap:16px;flex:1">
                {(0..6u32)
                    .map(|_| {
                        view! {
                            <div style="display:flex;align-items:center;gap:12px">
                                <div
                                    class="shim"
                                    style="width:56px;height:56px;border-radius:12px;flex-shrink:0"
                                ></div>
                                <div style="flex:1;display:flex;flex-direction:column;gap:8px">
                                    <div
                                        class="shim"
                                        style="height:15px;border-radius:6px;width:75%"
                                    ></div>
                                    <div
                                        class="shim"
                                        style="height:12px;border-radius:6px;width:50%"
                                    ></div>
                                </div>
                            </div>
                        }
                    })
                    .collect_view()}
            </div>
        </div>
    }
}

/// Guard: user harus login.
#[component]
pub(crate) fn AuthGuard(children: ChildrenFn) -> impl IntoView {
    let auth = use_context::<AuthResource>().expect("AuthResource not provided");
    let children = StoredValue::new(children);
    view! {
        <Suspense fallback=guard_skeleton>
            {move || {
                auth
                    .get()
                    .map(|result| match result {
                        Ok(Some(_)) => children.with_value(|c| c()).into_any(),
                        _ => {
                            view! { <GuardRedirect path="/login" /> }
                                .into_any()
                        }
                    })
            }}
        </Suspense>
    }
}

/// Guard: user harus punya role "admin".
#[component]
pub(crate) fn AdminGuard(children: ChildrenFn) -> impl IntoView {
    let auth = use_context::<AuthResource>().expect("AuthResource not provided");
    let children = StoredValue::new(children);
    view! {
        <Suspense fallback=guard_skeleton>
            {move || {
                auth
                    .get()
                    .map(|result| match result {
                        Ok(Some(user)) if user.role == "admin" => {
                            children.with_value(|c| c()).into_any()
                        }
                        Ok(Some(_)) => {
                            view! { <GuardRedirect path="/explore" /> }
                                .into_any()
                        }
                        _ => {
                            view! { <GuardRedirect path="/login" /> }
                                .into_any()
                        }
                    })
            }}
        </Suspense>
    }
}

/// Guard: user harus punya role "merchant" atau "admin".
#[component]
pub(crate) fn MerchantGuard(children: ChildrenFn) -> impl IntoView {
    let auth = use_context::<AuthResource>().expect("AuthResource not provided");
    let children = StoredValue::new(children);
    view! {
        <Suspense fallback=guard_skeleton>
            {move || {
                auth
                    .get()
                    .map(|result| match result {
                        Ok(Some(user)) if user.role == "merchant" || user.role == "admin" => {
                            children.with_value(|c| c()).into_any()
                        }
                        Ok(Some(_)) => {
                            view! { <GuardRedirect path="/explore" /> }
                                .into_any()
                        }
                        _ => {
                            view! { <GuardRedirect path="/login" /> }
                                .into_any()
                        }
                    })
            }}
        </Suspense>
    }
}
