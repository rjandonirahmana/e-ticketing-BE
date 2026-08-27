//! variant_editor.rs — Editor varian tiket untuk form create/edit product.
//!
//! Desain: tiap baris memegang `RwSignal` sendiri (name/price/quota) sehingga
//! mengetik TIDAK me-render ulang daftar (fokus input tak hilang); hanya
//! tambah/hapus baris yang me-render ulang (`<For>` keyed by `key`).
//!
//! Hapus baris varian LAMA (punya `id`) tidak menghapus row DB — id-nya dicatat
//! di `removed_ids` dan server menonaktifkannya (`is_active=false`), karena
//! tiket terjual bisa masih mereferensikan varian tersebut.

use std::sync::atomic::{AtomicU64, Ordering};

use leptos::prelude::*;

use crate::web::models::{ProductVariant, VariantForm};

/// Penerbit key unik antar-baris (kebutuhan `<For>`; wasm single-thread, Relaxed cukup).
static NEXT_KEY: AtomicU64 = AtomicU64::new(0);

/// Maks varian per product — samakan dengan batas server fn.
pub const MAX_VARIANTS: usize = 20;

#[derive(Clone)]
pub struct VariantRow {
    pub key: u64,
    /// Some = varian lama (dari DB), None = varian baru.
    pub id: Option<String>,
    pub name: RwSignal<String>,
    /// String supaya bebas diketik; diparse & divalidasi saat submit.
    pub price: RwSignal<String>,
    pub quota: RwSignal<String>,
}

pub fn new_variant_row(id: Option<String>, name: &str, price: &str, quota: &str) -> VariantRow {
    VariantRow {
        key: NEXT_KEY.fetch_add(1, Ordering::Relaxed),
        id,
        name: RwSignal::new(name.to_string()),
        price: RwSignal::new(price.to_string()),
        quota: RwSignal::new(quota.to_string()),
    }
}

/// Prefill baris dari varian product tersimpan (halaman edit).
pub fn rows_from_product(variants: &[ProductVariant]) -> Vec<VariantRow> {
    variants
        .iter()
        .filter(|v| v.is_active)
        .map(|v| {
            new_variant_row(
                Some(v.id.clone()),
                &v.name,
                &format!("{}", v.price),
                &v.quota.to_string(),
            )
        })
        .collect()
}

/// Validasi + serialisasi baris (dan id yang dihapus) → JSON untuk server fn.
/// `Err(pesan)` bila ada input tak valid — tampilkan di banner error form.
pub fn rows_to_json(rows: &[VariantRow], removed_ids: &[String]) -> Result<String, String> {
    if rows.is_empty() {
        return Err("Minimal satu varian produk wajib diisi.".into());
    }
    let mut out: Vec<VariantForm> = Vec::with_capacity(rows.len() + removed_ids.len());
    for (i, r) in rows.iter().enumerate() {
        let name = r.name.get_untracked().trim().to_string();
        if name.is_empty() {
            return Err(format!("Nama varian #{} wajib diisi.", i + 1));
        }
        let price: f64 = r
            .price
            .get_untracked()
            .trim()
            .parse()
            .map_err(|_| format!("Harga varian \"{name}\" harus berupa angka."))?;
        if price < 0.0 {
            return Err(format!("Harga varian \"{name}\" tidak boleh negatif."));
        }
        let quota: i32 = r
            .quota
            .get_untracked()
            .trim()
            .parse()
            .map_err(|_| format!("Kuota varian \"{name}\" harus berupa angka bulat."))?;
        if quota < 1 {
            return Err(format!("Kuota varian \"{name}\" minimal 1."));
        }
        out.push(VariantForm {
            id: r.id.clone(),
            name,
            price,
            quota,
            is_active: None,
        });
    }
    for id in removed_ids {
        out.push(VariantForm {
            id: Some(id.clone()),
            name: String::new(),
            price: 0.0,
            quota: 0,
            is_active: Some(false),
        });
    }
    serde_json::to_string(&out).map_err(|e| format!("Gagal menyusun data varian: {e}"))
}

#[component]
pub fn VariantEditor(
    rows: RwSignal<Vec<VariantRow>>,
    /// Id varian lama yang dihapus dari form (server menonaktifkannya).
    removed_ids: RwSignal<Vec<String>>,
) -> impl IntoView {
    // PENTING: ekspresi berisi `>=`/`<=` TIDAK boleh ditulis langsung di atribut
    // `view!` — parser makro menganggap `>` sebagai penutup tag, markup sisanya
    // bocor jadi teks (bug "= MAX_VARIANTS ON : CLICK ..."). Definisikan di sini.
    let at_max = move || rows.get().len() >= MAX_VARIANTS;
    let is_only_row = move || rows.get().len() <= 1;

    view! {
        <div class="medit-section-header">
            <span class="medit-section-label">"VARIAN PRODUK"</span>
        </div>
        <p style="font-size:12px;color:var(--text-muted);margin:0 0 10px">
            "Tentukan varian produk beserta harga & stoknya (cth. Merah, Biru, Ukuran XL)."
        </p>
        <For each=move || rows.get() key=|r| r.key let:row>
            {
                let row_key = row.key;
                let row_id = row.id.clone();
                view! {
                    <div class="medit-field-group" style="border:1px solid var(--border);border-radius:12px;padding:12px;margin-bottom:10px">
                        <div class="medit-field-group">
                            <label class="medit-field-label">"NAMA VARIAN"</label>
                            <input
                                type="text"
                                class="medit-input"
                                placeholder="cth. Reguler / VIP"
                                prop:value=move || row.name.get()
                                on:input=move |e| row.name.set(event_target_value(&e))
                            />
                        </div>
                        <div class="medit-grid-2">
                            <div class="medit-field-group">
                                <label class="medit-field-label">"HARGA (RP)"</label>
                                <input
                                    type="number"
                                    min="0"
                                    step="any"
                                    class="medit-input"
                                    placeholder="cth. 150000"
                                    prop:value=move || row.price.get()
                                    on:input=move |e| row.price.set(event_target_value(&e))
                                />
                            </div>
                            <div class="medit-field-group">
                                <label class="medit-field-label">"KUOTA"</label>
                                <input
                                    type="number"
                                    min="1"
                                    step="1"
                                    class="medit-input"
                                    placeholder="cth. 100"
                                    prop:value=move || row.quota.get()
                                    on:input=move |e| row.quota.set(event_target_value(&e))
                                />
                            </div>
                        </div>
                        <button
                            type="button"
                            class="medit-cancel-btn"
                            style="width:100%;margin-top:2px"
                            // Nonaktif bila ini baris terakhir — product wajib
                            // punya minimal satu varian. (Predikat didefinisikan
                            // di luar view! — lihat catatan `is_only_row`.)
                            disabled=is_only_row
                            on:click=move |_| {
                                if let Some(id) = &row_id {
                                    removed_ids.update(|v| v.push(id.clone()));
                                }
                                rows.update(|v| v.retain(|r| r.key != row_key));
                            }
                        >
                            "HAPUS VARIAN"
                        </button>
                    </div>
                }
            }
        </For>
        <button
            type="button"
            class="medit-cancel-btn"
            style="width:100%;margin-bottom:14px"
            disabled=at_max
            on:click=move |_| {
                rows.update(|v| v.push(new_variant_row(None, "", "", "")));
            }
        >
            "+ TAMBAH VARIAN"
        </button>
    }
}
