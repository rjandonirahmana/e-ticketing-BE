//! edit_profile.rs — Ubah data profil sendiri.
//!
//! Nama diubah langsung. NOMOR HP TIDAK: ia identitas login, penerima OTP, dan
//! penerima reset sandi, jadi memindahkannya menuntut bukti bahwa nomor barunya
//! benar-benar dipegang. Kodenya dikirim ke NOMOR BARU — bahwa orang ini
//! pemilik akunnya sudah dibuktikan oleh sesi yang sedang berjalan; yang belum
//! terbukti adalah "saya memegang nomor ini".
//!
//! Tanpa itu, salah ketik satu digit sudah cukup untuk mengunci seseorang
//! keluar dari akunnya sendiri: sandi pemulihannya akan dikirim ke nomor yang
//! tak pernah ia pegang.

use leptos::prelude::*;
use leptos_router::components::A;

use crate::web::api::{
    mulai_ganti_nomor_action, update_my_profile, verifikasi_ganti_nomor_action,
};
use crate::web::app::AuthResource;
use crate::web::components::ThemeToggle;

const CARD: &str = "bg-card border border-solid border-line-soft rounded-2xl";
const FIELD: &str = "w-full h-11 px-3.5 rounded-xl bg-surface border border-solid \
                     border-line text-content text-sm placeholder:text-content-muted";
const BTN: &str = "inline-flex items-center justify-center gap-2 min-h-11 px-5 \
                   rounded-full cursor-pointer border-0 bg-brand text-on-brand \
                   font-sans text-xs font-bold tracking-[0.08em] \
                   transition-opacity hover:opacity-90 disabled:opacity-50";
const BTN_GHOST: &str = "inline-flex items-center justify-center min-h-11 px-5 \
                         rounded-full cursor-pointer bg-transparent text-content \
                         border border-solid border-line font-sans text-xs font-bold";

#[component]
pub fn EditProfilePage() -> impl IntoView {
    let auth = use_context::<AuthResource>().expect("AuthResource missing");

    let nama = RwSignal::new(String::new());
    let nama_awal = RwSignal::new(String::new());
    let email = RwSignal::new(String::new());
    let email_awal = RwSignal::new(String::new());
    // Sesi terisi = formulir sudah pernah diisi dari server. Tanpa penanda ini,
    // `nama_awal` yang kosong tak bisa dibedakan dari "user memang belum punya
    // nama", dan formulirnya akan terus ditimpa ulang tiap kali sesi berubah.
    let terisi = RwSignal::new(false);
    let hp_baru = RwSignal::new(String::new());
    let otp = RwSignal::new(String::new());
    // Pengajuan ganti nomor sedang menunggu kode.
    let menunggu_otp = RwSignal::new(false);
    let sibuk = RwSignal::new(false);
    let pesan = RwSignal::new(Option::<String>::None);
    let galat = RwSignal::new(Option::<String>::None);

    // Isi formulir dari sesi, sekali, saat datanya tiba.
    Effect::new(move |_| {
        if let Some(Ok(Some(u))) = auth.get() {
            if !terisi.get_untracked() {
                terisi.set(true);
                nama.set(u.name.clone());
                nama_awal.set(u.name);
                let e = u.email.unwrap_or_default();
                email.set(e.clone());
                email_awal.set(e);
            }
        }
    });

    let simpan_profil = move |_| {
        let n = nama.get_untracked().trim().to_string();
        if n.is_empty() {
            galat.set(Some("Nama tidak boleh kosong.".into()));
            return;
        }
        let e = email.get_untracked().trim().to_string();
        sibuk.set(true);
        galat.set(None);
        pesan.set(None);
        leptos::task::spawn_local(async move {
            // Email SELALU dikirim (walau kosong) supaya "hapus email saya"
            // bisa dinyatakan. Server membedakan kosong dari tak-dikirim.
            match update_my_profile(n.clone(), Some(e.clone())).await {
                Ok(_) => {
                    nama_awal.set(n);
                    email_awal.set(e);
                    pesan.set(Some("Profil tersimpan.".into()));
                    // Sesi memuat nama; tanpa refetch, header dan halaman lain
                    // masih menampilkan nama lama sampai halaman dimuat ulang.
                    auth.refetch();
                }
                Err(e) => galat.set(Some(e.to_string())),
            }
            sibuk.set(false);
        });
    };

    let kirim_kode = move |_| {
        let hp = hp_baru.get_untracked().trim().to_string();
        if hp.is_empty() {
            galat.set(Some("Masukkan nomor HP baru.".into()));
            return;
        }
        sibuk.set(true);
        galat.set(None);
        pesan.set(None);
        leptos::task::spawn_local(async move {
            match mulai_ganti_nomor_action(hp).await {
                Ok(m) => {
                    menunggu_otp.set(true);
                    pesan.set(Some(m));
                }
                Err(e) => galat.set(Some(e.to_string())),
            }
            sibuk.set(false);
        });
    };

    let verifikasi = move |_| {
        let kode = otp.get_untracked().trim().to_string();
        if kode.is_empty() {
            galat.set(Some("Masukkan kode dari WhatsApp.".into()));
            return;
        }
        sibuk.set(true);
        galat.set(None);
        leptos::task::spawn_local(async move {
            match verifikasi_ganti_nomor_action(kode).await {
                Ok(m) => {
                    menunggu_otp.set(false);
                    otp.set(String::new());
                    hp_baru.set(String::new());
                    pesan.set(Some(m));
                    // Nomor ikut di dalam JWT; tanpa refetch, aplikasi masih
                    // memegang nomor lama sampai sesi berikutnya.
                    auth.refetch();
                }
                Err(e) => galat.set(Some(e.to_string())),
            }
            sibuk.set(false);
        });
    };

    view! {
        <div class="page">
            <header class="page-header">
                <A href="/profile" attr:class="back-btn" attr:aria-label="Kembali">
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none"
                         stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                        <polyline points="15 18 9 12 15 6" />
                    </svg>
                </A>
                <span class="page-logo">"EDIT PROFIL"</span>
                <ThemeToggle />
            </header>

            <div class="flex flex-col gap-4 px-5 py-5">

                {move || galat.get().map(|e| view! {
                    <div class="flex items-center gap-2 px-4 py-3 rounded-xl \
                                bg-[color-mix(in_srgb,var(--danger)_10%,transparent)] \
                                border border-solid border-[color-mix(in_srgb,var(--danger)_30%,transparent)] \
                                text-danger text-[12px]">
                        <span>{e}</span>
                    </div>
                })}
                {move || pesan.get().map(|m| view! {
                    <div class="flex items-center gap-2 px-4 py-3 rounded-xl \
                                bg-[var(--color-primary-soft)] \
                                border border-solid border-[var(--color-primary-border)] \
                                text-content text-[12px]">
                        <span>{m}</span>
                    </div>
                })}

                // ── Nama ──────────────────────────────────────────────────────
                <section class=format!("{CARD} p-4 flex flex-col gap-3")>
                    <span class="font-sans text-[10px] tracking-[0.12em] text-content-muted">
                        "NAMA"
                    </span>
                    <input
                        class=FIELD
                        r#type="text"
                        placeholder="Nama kamu"
                        disabled=move || sibuk.get()
                        prop:value=move || nama.get()
                        on:input=move |e| { nama.set(event_target_value(&e)); galat.set(None); }
                    />

                    <span class="font-sans text-[10px] tracking-[0.12em] text-content-muted mt-1">
                        "EMAIL (OPSIONAL)"
                    </span>
                    <input
                        class=FIELD
                        r#type="email"
                        autocomplete="email"
                        placeholder="kamu@example.com"
                        disabled=move || sibuk.get()
                        prop:value=move || email.get()
                        on:input=move |e| { email.set(event_target_value(&e)); galat.set(None); }
                    />
                    // Email bukan identitas login di sini — nomor HP yang
                    // memegang peran itu. Dikosongkan pun akun tetap bisa masuk.
                    <p class="text-[11px] text-content-muted">
                        "Email hanya untuk kontak. Masuk tetap memakai nomor HP."
                    </p>

                    <button
                        class=BTN
                        disabled=move || {
                            sibuk.get()
                                || (nama.get() == nama_awal.get() && email.get() == email_awal.get())
                        }
                        on:click=simpan_profil
                    >
                        "SIMPAN PERUBAHAN"
                    </button>
                </section>

                // ── Nomor HP ──────────────────────────────────────────────────
                <section class=format!("{CARD} p-4 flex flex-col gap-3")>
                    <span class="font-sans text-[10px] tracking-[0.12em] text-content-muted">
                        "NOMOR HP"
                    </span>
                    <p class="text-[12px] text-content-soft">
                        "Nomor sekarang: "
                        <strong>
                            {move || {
                                auth.get()
                                    .and_then(|r| r.ok())
                                    .flatten()
                                    .map(|u| u.phone)
                                    .unwrap_or_default()
                            }}
                        </strong>
                    </p>
                    // Kalimat ini bukan basa-basi: ia menjelaskan kenapa ada
                    // langkah tambahan yang tak ada di kolom nama.
                    <p class="text-[11px] text-content-muted leading-relaxed">
                        "Nomor HP dipakai untuk masuk dan menerima reset password, jadi \
                         penggantiannya butuh kode. Kode dikirim ke NOMOR BARU — nomor \
                         lama tetap berlaku sampai kodenya benar."
                    </p>

                    {move || if menunggu_otp.get() {
                        view! {
                            <input
                                class=FIELD
                                r#type="text"
                                inputmode="numeric"
                                placeholder="6 digit kode dari WhatsApp"
                                disabled=move || sibuk.get()
                                prop:value=move || otp.get()
                                on:input=move |e| { otp.set(event_target_value(&e)); galat.set(None); }
                            />
                            <div class="flex gap-2">
                                <button class=BTN disabled=move || sibuk.get() on:click=verifikasi>
                                    "VERIFIKASI & GANTI"
                                </button>
                                <button
                                    class=BTN_GHOST
                                    disabled=move || sibuk.get()
                                    on:click=move |_| {
                                        menunggu_otp.set(false);
                                        otp.set(String::new());
                                        pesan.set(None);
                                    }
                                >
                                    "Batal"
                                </button>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <input
                                class=FIELD
                                r#type="tel"
                                inputmode="numeric"
                                autocomplete="tel"
                                placeholder="08xxxxxxxxxx"
                                disabled=move || sibuk.get()
                                prop:value=move || hp_baru.get()
                                on:input=move |e| { hp_baru.set(event_target_value(&e)); galat.set(None); }
                            />
                            <button class=BTN disabled=move || sibuk.get() on:click=kirim_kode>
                                "KIRIM KODE KE NOMOR BARU"
                            </button>
                        }.into_any()
                    }}
                </section>
            </div>
        </div>
    }
}



