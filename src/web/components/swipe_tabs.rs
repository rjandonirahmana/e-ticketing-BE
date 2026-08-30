//! swipe_tabs.rs — Bilah tab + dek konten yang berpindah dengan GESERAN.
//!
//! Dipakai bersama oleh Merchant Hub (`pages/merchant.rs`) dan Pusat Admin
//! (`pages/admin/mod.rs`). Keduanya dulu punya bilah tab yang identik tapi
//! hanya salah satunya (merchant) yang bisa digeser — dan gesekan di sana pun
//! TAK MENAMPAKKAN APA PUN sampai jari diangkat, sehingga terasa seperti tak
//! berfungsi. Satu tempat, satu perilaku.
//!
//! Tiga hal yang membuatnya terbaca sebagai slider, bukan sekadar pintasan:
//!
//!   * Panel mengikuti jari saat ditarik (`INTIP`, dengan tahanan karet), jadi
//!     ada balasan seketika bahwa geseran sedang dibaca.
//!   * Garis tab ikut bergerak sepanjang tarikan — itu yang memberi tahu KE
//!     MANA geseran ini akan mendarat sebelum jari diangkat.
//!   * Panel yang masuk meluncur dari sisi yang benar.

use leptos::prelude::*;
use wasm_bindgen::JsCast;

/// Jarak minimum satu gesekan dianggap perpindahan tab (piksel CSS).
const GESER_MIN: f64 = 48.0;

/// Seberapa dominan gerakan horizontal harus dibanding vertikal sebelum sumbu
/// dikunci mendatar. Isi tab adalah daftar yang digulir ke bawah; jari yang
/// menggulir hampir selalu sedikit miring, dan tanpa margin ini gulir biasa
/// akan sering tercuri jadi perpindahan tab.
const DOMINASI_H: f64 = 1.25;

/// Batas seret panel, sebagai pecahan lebar dek.
///
/// Sengaja kecil. Menyeret panel selebar penuh mengikuti jari akan memaksa
/// kedua panel dirender bersamaan — dan tinggi keduanya berbeda jauh (tab
/// "Pengaturan" seperempat tinggi tab "Produk"), sehingga halaman akan
/// melonjak tiap kali disentuh. Yang dibutuhkan cuma cukup gerakan untuk
/// terbaca sebagai "ada yang bisa digeser di sini".
const INTIP: f64 = 0.16;

/// Gerakan minimum sebelum sumbu diputuskan (piksel). Di bawah ini arah jari
/// masih derau.
pub(crate) const AMBANG_SUMBU: f64 = 10.0;

// ─── Keputusan gerakan (murni) ────────────────────────────────────────────────
//
// Dipisahkan dari sinyal supaya bisa diuji tanpa DOM maupun runtime reaktif.
// Dua keputusan di bawah inilah yang menentukan apakah geseran terasa benar
// atau justru mencuri gulir orang, dan keduanya mudah rusak diam-diam saat
// ambangnya disetel ulang.

/// Sumbu gerakan, bila sudah cukup jauh untuk diputuskan.
///
/// `None` = jarinya belum bergerak cukup jauh; arahnya masih derau dan
/// menguncinya sekarang akan salah separuh waktu.
/// `Some(true)` = horizontal (milik kita), `Some(false)` = vertikal (gulir).
pub(crate) fn sumbu_horizontal(dx: f64, dy: f64) -> Option<bool> {
    if dx.abs().max(dy.abs()) < AMBANG_SUMBU {
        return None;
    }
    Some(dx.abs() > dy.abs() * DOMINASI_H)
}

/// Tab tujuan setelah jari diangkat, atau `None` bila tak ada perpindahan.
///
/// `dx` negatif = geser ke KIRI = maju ke tab berikutnya, mengikuti arah kertas
/// yang ditarik.
fn tujuan_geser(kini: usize, len: usize, dx: f64) -> Option<usize> {
    if len == 0 {
        return None;
    }
    // Sentuhan pendek adalah KETUKAN, bukan geseran. Tanpa ambang ini, menekan
    // tombol di dalam daftar ikut memindahkan tab.
    if dx.abs() < GESER_MIN {
        return None;
    }
    let tujuan = if dx < 0.0 {
        (kini + 1).min(len - 1)
    } else {
        kini.saturating_sub(1)
    };
    // Di ujung, diam — bukan melingkar. Melompat dari tab terakhir ke tab
    // pertama terasa seperti tergelincir, bukan berpindah.
    (tujuan != kini).then_some(tujuan)
}

// ─── State ────────────────────────────────────────────────────────────────────

/// Satu tab pada bilah: label, plus lencana angka opsional (0 = tak tampil).
#[derive(Clone)]
pub struct TabItem {
    pub label: &'static str,
    pub badge: Option<Signal<usize>>,
}

impl TabItem {
    pub fn new(label: &'static str) -> Self {
        Self { label, badge: None }
    }
    pub fn with_badge(label: &'static str, badge: Signal<usize>) -> Self {
        Self { label, badge: Some(badge) }
    }
}

/// Kendali bersama bilah tab + dek konten.
///
/// `Copy` supaya bisa disalin bebas ke dalam penutup event tanpa `clone()`
/// bertaburan — semua isinya sudah berupa pegangan reaktif.
#[derive(Clone, Copy)]
pub struct TabSwipe {
    idx: RwSignal<usize>,
    /// Arah panel yang baru masuk: 1 dari kanan, -1 dari kiri, 0 tanpa animasi.
    dir: RwSignal<i8>,
    /// Pergeseran jari yang sudah diredam (px). 0 saat diam.
    drag: RwSignal<f64>,
    /// True selama jari masih menempel → transisi dimatikan agar panel benar-
    /// benar mengikuti jari, bukan mengejarnya dengan tundaan.
    live: RwSignal<bool>,
    len: usize,
    // Tiga nilai berikut dibaca-tulis puluhan kali per gesekan tetapi TIDAK ADA
    // satu pun bagian tampilan yang menontonnya, jadi `StoredValue` — bukan
    // `RwSignal`, yang akan memicu pembaruan reaktif sia-sia sepanjang tarikan.
    awal: StoredValue<(f64, f64)>,
    /// `None` = sumbu belum diputuskan; `Some(true)` = horizontal (milik kita).
    sumbu: StoredValue<Option<bool>>,
    /// Lebar dek terakhir yang terukur; jadi penyebut semua perhitungan pecahan.
    lebar: StoredValue<f64>,
}

impl TabSwipe {
    pub fn new(len: usize) -> Self {
        Self {
            idx: RwSignal::new(0),
            dir: RwSignal::new(0),
            drag: RwSignal::new(0.0),
            live: RwSignal::new(false),
            len: len.max(1),
            awal: StoredValue::new((0.0, 0.0)),
            sumbu: StoredValue::new(None),
            lebar: StoredValue::new(360.0),
        }
    }

    /// Indeks tab aktif — reaktif, ini yang dibaca `match` konten.
    pub fn index(self) -> usize {
        self.idx.get()
    }

    pub fn len(self) -> usize {
        self.len
    }

    /// Pindah tab, sekaligus memutuskan dari sisi mana panel baru meluncur.
    pub fn go(self, tujuan: usize) {
        let kini = self.idx.get_untracked();
        let tujuan = tujuan.min(self.len - 1);
        if tujuan == kini {
            return;
        }
        self.dir.set(if tujuan > kini { 1 } else { -1 });
        self.idx.set(tujuan);
    }

    // ── Penangan sentuh ──────────────────────────────────────────────────────

    pub fn on_start(self) -> impl Fn(web_sys::TouchEvent) + Clone + 'static {
        move |e: web_sys::TouchEvent| {
            let Some(t) = e.touches().get(0) else { return };
            self.awal.set_value((t.client_x() as f64, t.client_y() as f64));
            self.sumbu.set_value(None);
            self.drag.set(0.0);
            self.live.set(true);
            // Lebar diukur dari elemen dek itu sendiri, bukan dari lebar
            // jendela: kolom aplikasi ini dibatasi lebar maksimum, jadi di
            // layar lebar keduanya berbeda jauh dan seluruh pecahan (tahanan
            // karet, posisi garis tab) akan meleset kalau dipukul rata.
            if let Some(el) = e
                .current_target()
                .and_then(|t| t.dyn_into::<web_sys::HtmlElement>().ok())
            {
                let w = el.client_width() as f64;
                if w > 0.0 {
                    self.lebar.set_value(w);
                }
            }
        }
    }

    pub fn on_move(self) -> impl Fn(web_sys::TouchEvent) + Clone + 'static {
        move |e: web_sys::TouchEvent| {
            if !self.live.get_untracked() {
                return;
            }
            let Some(t) = e.touches().get(0) else { return };
            let (ax, ay) = self.awal.get_value();
            let dx = t.client_x() as f64 - ax;
            let dy = t.client_y() as f64 - ay;

            // Sumbu dikunci SEKALI di awal gerakan lalu tak pernah ditinjau
            // ulang. Kalau ditinjau tiap frame, gulir vertikal panjang yang
            // sedikit melenceng ke samping di tengah jalan akan mendadak
            // menggeser panel.
            if self.sumbu.get_value().is_none() {
                match sumbu_horizontal(dx, dy) {
                    None => return,
                    putusan => self.sumbu.set_value(putusan),
                }
            }
            if self.sumbu.get_value() != Some(true) {
                return;
            }

            let w = self.lebar.get_value().max(1.0);
            let i = self.idx.get_untracked();
            // Di ujung larik tak ada tab tujuan, jadi tahanannya diperketat:
            // panel tetap bergerak sedikit (geserannya terbaca), tapi jelas
            // menolak — bukan diam total, yang terbaca seperti macet.
            let mentok = (dx > 0.0 && i == 0) || (dx < 0.0 && i + 1 >= self.len);
            let batas = w * if mentok { INTIP * 0.3 } else { INTIP };
            // `tanh` memberi tahanan karet: linier di awal, mendekati `batas`
            // secara asimtotik. Pemotongan keras (`clamp`) terasa seperti
            // membentur dinding di tengah tarikan.
            self.drag.set(batas * (dx / batas).tanh());
        }
    }

    pub fn on_end(self) -> impl Fn(web_sys::TouchEvent) + Clone + 'static {
        move |e: web_sys::TouchEvent| {
            if !self.live.get_untracked() {
                return;
            }
            self.live.set(false);
            let horizontal = self.sumbu.get_value() == Some(true);
            self.drag.set(0.0);
            if !horizontal {
                return;
            }
            let Some(t) = e.changed_touches().get(0) else { return };
            let dx = t.client_x() as f64 - self.awal.get_value().0;
            let i = self.idx.get_untracked();
            if let Some(tujuan) = tujuan_geser(i, self.len, dx) {
                self.go(tujuan);
            }
        }
    }

    // ── Gaya turunan ─────────────────────────────────────────────────────────

    /// Gaya inline dek: mengikuti jari saat ditarik, memantul balik saat lepas.
    pub fn gaya_dek(self) -> String {
        let d = self.drag.get();
        let transisi = if self.live.get() {
            "none"
        } else {
            "transform .34s cubic-bezier(.32,.72,0,1)"
        };
        format!("transform:translate3d({d:.1}px,0,0);transition:{transisi}")
    }

    /// Kelas panel yang sedang tampil — memicu luncuran masuk dari sisi benar.
    ///
    /// Dipanggil DI DALAM penutup yang membaca `index()`, sehingga simpul
    /// pembungkusnya dibangun ulang tiap pindah tab dan animasinya berjalan
    /// lagi. Kalau pembungkusnya dibuat sekali di luar penutup, kelasnya
    /// berganti tapi animasi CSS-nya tak pernah diputar ulang.
    pub fn kelas_panel(self) -> &'static str {
        match self.dir.get_untracked() {
            1 => "tabdeck-panel tabdeck-panel--dari-kanan",
            -1 => "tabdeck-panel tabdeck-panel--dari-kiri",
            _ => "tabdeck-panel",
        }
    }

    /// Posisi garis penanda tab. Ikut bergerak sepanjang tarikan — inilah yang
    /// memberi tahu tujuan geseran SEBELUM jari diangkat.
    fn gaya_ink(self) -> String {
        let n = self.len as f64;
        let w = self.lebar.get_value().max(1.0);
        let i = self.idx.get() as f64;
        // Tarikan penuh (sampai `INTIP`) menggeser garis setengah lebar tab:
        // cukup untuk mencondong ke tetangga, tak cukup untuk berbohong bahwa
        // perpindahan sudah terjadi.
        let condong = (-self.drag.get() / (w * INTIP)).clamp(-1.0, 1.0) * 0.5;
        let p = (i + condong).clamp(0.0, n - 1.0);
        format!("left:{:.4}%;width:{:.4}%", p * 100.0 / n, 100.0 / n)
    }
}

// ─── Bilah tab ────────────────────────────────────────────────────────────────

#[component]
pub fn SwipeTabBar(swipe: TabSwipe, tabs: Vec<TabItem>) -> impl IntoView {
    let scroller: NodeRef<leptos::html::Div> = NodeRef::new();
    let n = tabs.len().max(1);

    // Dengan enam tab, bilahnya lebih lebar dari layar ponsel. Berpindah lewat
    // geseran karena itu bisa mendaratkan tab aktif di luar pandangan — dan
    // penandanya jadi tak terlihat justru pada saat ia paling perlu dilihat.
    // Gulirannya halus lewat `scroll-behavior: smooth` di CSS.
    Effect::new(move |_| {
        let i = swipe.index();
        let Some(el) = scroller.get() else { return };
        let tampak = el.client_width() as f64;
        let penuh = el.scroll_width() as f64;
        if penuh <= tampak {
            return;
        }
        let lebar_tab = penuh / n as f64;
        let target = (lebar_tab * (i as f64 + 0.5) - tampak / 2.0).clamp(0.0, penuh - tampak);
        el.set_scroll_left(target as i32);
    });

    view! {
        <div class="mhub-mobile-tabs" node_ref=scroller>
            // Trek terpisah dari pembungkus yang menggulir: garis penanda
            // diposisikan absolut dengan satuan persen, dan persen itu diukur
            // dari blok penampungnya. Kalau diletakkan langsung di pembungkus
            // yang menggulir, persennya diukur dari lebar YANG TAMPAK, bukan
            // lebar seluruh deretan tab — sehingga garisnya meleset makin jauh
            // tiap kali bilahnya digulir.
            <div class="mhub-mtab-track">
                {tabs
                    .into_iter()
                    .enumerate()
                    .map(|(i, tab)| {
                        let badge = tab.badge;
                        view! {
                            <button
                                type="button"
                                class=move || {
                                    if swipe.index() == i {
                                        "mhub-mtab mhub-mtab--active"
                                    } else {
                                        "mhub-mtab"
                                    }
                                }
                                on:click=move |_| swipe.go(i)
                            >
                                {tab.label}
                                {move || {
                                    let c = badge.map(|b| b.get()).unwrap_or(0);
                                    (c > 0)
                                        .then(|| view! { <span class="mhub-mtab-badge">{c}</span> })
                                }}
                            </button>
                        }
                    })
                    .collect_view()}
                <span class="mhub-mtab-ink" style=move || swipe.gaya_ink()></span>
            </div>
        </div>
    }
}

// ─── Uji keputusan gerakan ────────────────────────────────────────────────────
//
// Bilah tab ini dipakai Merchant Hub dan Pusat Admin, dan isinya daftar panjang
// yang digulir ke bawah. Dua kegagalan yang paling mudah terjadi dan paling
// menjengkelkan justru tak menghasilkan galat apa pun:
//
//   * ambang terlalu longgar → KETUKAN pada tombol di dalam daftar terbaca
//     sebagai geseran, dan tab berpindah saat orang menekan sesuatu;
//   * dominasi horizontal terlalu longgar → GULIR VERTIKAL yang sedikit miring
//     (yang normal; jari manusia tak pernah lurus) tercuri jadi pindah tab.
//
// Keduanya cuma soal angka, dan angka mudah disetel ulang tanpa sadar. Uji ini
// mengunci perilakunya, bukan angkanya.
#[cfg(test)]
mod tests_gerakan {
    use super::*;

    // ── Penguncian sumbu ────────────────────────────────────────────────────

    /// Gerakan sangat kecil belum boleh memutuskan apa pun: arahnya masih derau.
    #[test]
    fn gerakan_kecil_belum_memutuskan_sumbu() {
        assert_eq!(sumbu_horizontal(3.0, 2.0), None);
        assert_eq!(sumbu_horizontal(-4.0, 1.0), None);
    }

    /// Gulir vertikal yang MIRING tetap gulir — ini kasus yang paling sering
    /// terjadi di dunia nyata dan paling merusak bila salah dibaca.
    #[test]
    fn gulir_miring_tetap_vertikal() {
        // Turun 60px sambil melenceng 30px ke samping: jelas menggulir.
        assert_eq!(sumbu_horizontal(30.0, 60.0), Some(false));
        // Bahkan saat lencengnya cukup besar, selama vertikalnya masih dominan.
        assert_eq!(sumbu_horizontal(40.0, 50.0), Some(false));
    }

    /// Geseran mendatar yang jelas dikenali sebagai horizontal.
    #[test]
    fn geseran_mendatar_dikenali() {
        assert_eq!(sumbu_horizontal(80.0, 10.0), Some(true));
        assert_eq!(sumbu_horizontal(-80.0, 10.0), Some(true));
    }

    /// Horizontal harus DOMINAN, bukan sekadar lebih besar. Selisih tipis
    /// diserahkan ke gulir — salah menebak ke arah gulir hanya berarti "tak
    /// terjadi apa-apa", sedangkan salah ke arah tab memindahkan halaman orang.
    #[test]
    fn mendatar_tipis_diserahkan_ke_gulir() {
        assert_eq!(sumbu_horizontal(52.0, 50.0), Some(false));
    }

    // ── Tujuan geseran ──────────────────────────────────────────────────────

    /// Ketukan (jarak sangat pendek) tak boleh memindahkan tab.
    #[test]
    fn ketukan_bukan_geseran() {
        assert_eq!(tujuan_geser(1, 4, 5.0), None);
        assert_eq!(tujuan_geser(1, 4, -5.0), None);
        // Tepat di bawah ambang.
        assert_eq!(tujuan_geser(1, 4, -(GESER_MIN - 1.0)), None);
    }

    /// Geser ke kiri = maju; ke kanan = mundur.
    #[test]
    fn arah_geser_mengikuti_arah_kertas() {
        assert_eq!(tujuan_geser(1, 4, -100.0), Some(2), "geser kiri → tab berikutnya");
        assert_eq!(tujuan_geser(1, 4, 100.0), Some(0), "geser kanan → tab sebelumnya");
    }

    /// Di ujung, DIAM — tidak melingkar. Melompat dari tab terakhir ke tab
    /// pertama terasa seperti tergelincir, bukan berpindah.
    #[test]
    fn ujung_tak_melingkar() {
        assert_eq!(tujuan_geser(0, 4, 100.0), None, "sudah di tab pertama");
        assert_eq!(tujuan_geser(3, 4, -100.0), None, "sudah di tab terakhir");
    }

    /// Enam tab Pusat Admin: geseran menyusuri seluruhnya lalu berhenti.
    #[test]
    fn menyusuri_enam_tab_admin() {
        let len = 6;
        let mut i = 0;
        for harap in 1..=5 {
            i = tujuan_geser(i, len, -100.0).expect("harus maju");
            assert_eq!(i, harap);
        }
        assert_eq!(tujuan_geser(i, len, -100.0), None, "berhenti di tab terakhir");
    }

    /// Larik kosong tak boleh membuat indeks meluap.
    #[test]
    fn tanpa_tab_tak_meluap() {
        assert_eq!(tujuan_geser(0, 0, -100.0), None);
    }
}
