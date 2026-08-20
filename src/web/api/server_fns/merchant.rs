use crate::web::models::*;
use leptos::prelude::*;
#[cfg_attr(not(feature = "ssr"), allow(unused_imports))]
use super::helpers::*;

#[server(GetMerchantEvents, "/api-fn")]
pub async fn get_merchant_events(page: Option<i64>) -> Result<PaginatedEvents, ServerFnError> {
    use crate::models::events::EventListQuery;
    let claims = require_roles(&["merchant", "admin"]).await?;
    let state = app_state().await?;
    let q = EventListQuery {
        page,
        per_page: Some(20),
        city: None,
        category: None,
        search: None,
        status: None,
    };
    let result = state
        .event_svc
        .list(q, Some(&claims.user_id))
        .await
        .map_err(map_app_error)?;
    return Ok(srv_paginated_events_to_web(result));
}

#[server(GetMerchantEventDetail, "/api-fn")]
pub async fn get_merchant_event_detail(slug: String) -> Result<EventWithVariants, ServerFnError> {
    let _claims = require_roles(&["merchant", "admin"]).await?;
    let state = app_state().await?;
    let result = state
        .event_svc
        .get(&slug)
        .await
        .map_err(map_app_error)?;
    return Ok(srv_event_with_variants_to_web(result));
}

/// Batas jumlah varian per event (samakan dengan `MAX_VARIANTS` di
/// `web/components/variant_editor.rs`) — jangan percaya klien.
#[cfg(feature = "ssr")]
const MAX_VARIANTS_SRV: usize = 20;

/// Parse JSON varian dari form. String kosong → None (create pakai default,
/// update tidak menyentuh varian). JSON rusak/kebanyakan → Err(pesan).
#[cfg(feature = "ssr")]
fn parse_variants_json(raw: &str) -> Result<Option<Vec<crate::web::models::VariantForm>>, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    let forms: Vec<crate::web::models::VariantForm> =
        serde_json::from_str(raw).map_err(|e| format!("Data varian tidak valid: {e}"))?;
    if forms.len() > MAX_VARIANTS_SRV * 2 {
        return Err("Terlalu banyak varian.".into());
    }
    Ok(Some(forms))
}

/// Batas foto detail per event (samakan dengan cap 6 di `detail_image_section.rs`).
#[cfg(feature = "ssr")]
const MAX_DETAIL_IMAGES_SRV: usize = 6;

/// Parse JSON foto detail dari form → `Vec<DetailImageEntry>` (urutan
/// dipertahankan sesuai array = urutan tampil). String kosong → `None`:
///   - create: tak ada foto detail;
///   - update: field `detail_images` tak disentuh (COALESCE di repo).
/// `Some(vec![])` (array kosong "[]") = user menghapus semua foto → di-clear.
/// URL divalidasi hanya sebatas berasal dari public_url storage kita.
#[cfg(feature = "ssr")]
fn parse_detail_images_json(
    raw: &str,
    public_url: &str,
) -> Result<Option<Vec<crate::models::events::DetailImageEntry>>, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    let mut items: Vec<crate::models::events::DetailImageEntry> =
        serde_json::from_str(raw).map_err(|e| format!("Data foto tidak valid: {e}"))?;
    if items.len() > MAX_DETAIL_IMAGES_SRV {
        return Err(format!("Maksimal {MAX_DETAIL_IMAGES_SRV} foto detail."));
    }
    let base = public_url.trim_end_matches('/');
    for it in &mut items {
        // Terima URL absolut milik storage kita, atau path relatif — tolak URL
        // eksternal agar tak menyimpan tautan sembarangan sebagai "foto event".
        if !(it.url.starts_with(base) || it.url.starts_with('/')) {
            return Err("URL foto tidak dikenal.".into());
        }
        if !matches!(it.image_type.as_str(), "map" | "seat" | "price" | "other") {
            it.image_type = "other".into();
        }
        it.caption.truncate(500);
        // Nilai ini berakhir di atribut `style`, jadi ia dibersihkan di sini —
        // satu-satunya pintu masuknya dari luar. Yang tak berbentuk "X% Y%"
        // jatuh ke tengah, bukan diteruskan apa adanya.
        it.focus = crate::models::events::normalisasi_fokus(&it.focus);
    }
    Ok(Some(items))
}

#[server(CreateMerchantEvent, "/api-fn")]
pub async fn create_merchant_event(
    name: String,
    description: String,
    venue: String,
    city: String,
    event_date: String,
    start_time: String,
    categories: String,
    latitude: Option<f64>,
    longitude: Option<f64>,
    variants: String,
    cover_url: String,
    detail_images: String,
) -> Result<String, ServerFnError> {
    use crate::models::events::{CreateEventRequest, CreateVariantInline};
    let claims = require_roles(&["merchant", "admin"]).await?;
    let state = app_state().await?;

    // Foto: FE meng-upload file ke /upload/merchant-image lalu mengirim URL-nya
    // di sini (cover = string tunggal, detail = JSON array terurut). String
    // kosong = tak ada foto.
    let cover = { let c = cover_url.trim(); (!c.is_empty()).then(|| c.to_string()) };
    let detail_imgs = parse_detail_images_json(&detail_images, &state.storage.public_url)
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e) })?
        .unwrap_or_default();

    // Varian dari form (JSON) → CreateVariantInline. Kosong → default lama
    // ("Umum", gratis, kuota 100) agar kompatibel dengan alur minimal.
    let variants_inline: Vec<CreateVariantInline> = match parse_variants_json(&variants)
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e) })?
    {
        Some(forms) => forms
            .into_iter()
            // Baris "hapus" (is_active=false) tak bermakna saat create — buang.
            .filter(|f| f.is_active != Some(false))
            .enumerate()
            .map(|(i, f)| CreateVariantInline {
                name: f.name,
                description: None,
                price: f.price,
                sale_price: None,
                sale_price_start_date: None,
                sale_price_end_date: None,
                quota: f.quota,
                max_per_order: None,
                sort_order: Some(i as i32),
            })
            .collect(),
        None => vec![],
    };
    let variants_inline = if variants_inline.is_empty() {
        vec![CreateVariantInline {
            name: "Umum".into(),
            description: None,
            price: 0.0,
            sale_price: None,
            sale_price_start_date: None,
            sale_price_end_date: None,
            quota: 100,
            max_per_order: None,
            sort_order: None,
        }]
    } else {
        variants_inline
    };

    let cats: Vec<String> = categories
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let event_date_dt: chrono::DateTime<chrono::Utc> = event_date
        .parse()
        .map_err(|e: chrono::ParseError| -> ServerFnError {
            ServerFnError::ServerError(e.to_string())
        })?;
    let start_time_dt: Option<chrono::DateTime<chrono::Utc>> = if start_time.is_empty() {
        None
    } else {
        Some(start_time.parse().map_err(|e: chrono::ParseError| -> ServerFnError {
            ServerFnError::ServerError(e.to_string())
        })?)
    };

    // Get merchant name from merchant profile
    let merchant_name = state
        .merchant_svc
        .get_profile(&claims.user_id)
        .await
        .map(|m| m.store_name)
        .unwrap_or_else(|_| claims.name.clone());

    let req = CreateEventRequest {
        merchant_name: merchant_name.clone(),
        name,
        description: if description.is_empty() {
            None
        } else {
            Some(description)
        },
        venue: if venue.is_empty() { None } else { Some(venue) },
        city: if city.is_empty() { None } else { Some(city) },
        latitude,
        longitude,
        category: cats,
        event_date: event_date_dt,
        start_time: start_time_dt,
        end_time: None,
        variants: variants_inline,
        detail_images: detail_imgs,
    };

    let result = state
        .event_svc
        .create(&claims.user_id, &merchant_name, req, cover.as_deref())
        .await
        .map_err(map_app_error)?;
    return Ok(result.slug);
}

#[server(UpdateMerchantEvent, "/api-fn")]
pub async fn update_merchant_event(
    slug: String,
    name: String,
    description: String,
    venue: String,
    city: String,
    event_date: String,
    start_time: String,
    categories: String,
    latitude: Option<f64>,
    longitude: Option<f64>,
    variants: String,
    cover_url: String,
    detail_images: String,
) -> Result<(), ServerFnError> {
    use crate::models::events::{UpdateEventRequest, UpdateVariantInline};
    let claims = require_roles(&["merchant", "admin"]).await?;
    let state = app_state().await?;

    // Foto: cover kosong = pertahankan cover lama (None = COALESCE di repo);
    // detail_images kosong = tak disentuh, "[]" = hapus semua. FE mengirim URL
    // hasil upload ke /upload/merchant-image.
    let cover_new = { let c = cover_url.trim(); (!c.is_empty()).then(|| c.to_string()) };
    // Salinan untuk perbandingan foto lama vs baru di bawah — `cover_new` sendiri
    // ikut berpindah ke dalam `UpdateEventRequest`.
    let cover_baru = cover_new.clone();
    let detail_imgs = parse_detail_images_json(&detail_images, &state.storage.public_url)
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e) })?;

    // Varian dari form (JSON). String kosong → None (varian tidak disentuh).
    // id Some = update varian lama, None = tambah baru; is_active=false =
    // "dihapus" dari form → dinonaktifkan (COALESCE di repo: field None = tetap).
    let variants_update: Option<Vec<UpdateVariantInline>> =
        parse_variants_json(&variants)
        .map_err(|e| -> ServerFnError { ServerFnError::ServerError(e) })?
        .map(|forms| {
            forms
                .into_iter()
                .enumerate()
                .map(|(i, f)| {
                    let deactivate = f.is_active == Some(false);
                    UpdateVariantInline {
                        id: f.id,
                        // Baris nonaktif hanya membawa id + is_active=false;
                        // field lain None agar nilai lama tak tertimpa kosong.
                        name: (!deactivate).then_some(f.name),
                        description: None,
                        price: (!deactivate).then_some(f.price),
                        sale_price: None,
                        sale_price_start_date: None,
                        sale_price_end_date: None,
                        quota: (!deactivate).then_some(f.quota),
                        max_per_order: None,
                        is_active: if deactivate { Some(false) } else { Some(true) },
                        sort_order: (!deactivate).then_some(i as i32),
                    }
                })
                .collect()
        });

    let cats: Vec<String> = categories
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let event_date_dt: Option<chrono::DateTime<chrono::Utc>> = if event_date.is_empty() {
        None
    } else {
        Some(event_date.parse().map_err(|e: chrono::ParseError| -> ServerFnError {
            ServerFnError::ServerError(e.to_string())
        })?)
    };
    let start_time_dt: Option<chrono::DateTime<chrono::Utc>> = if start_time.is_empty() {
        None
    } else {
        Some(start_time.parse().map_err(|e: chrono::ParseError| -> ServerFnError {
            ServerFnError::ServerError(e.to_string())
        })?)
    };

    // Find the event by slug to get its id and owner.
    let event = state
        .event_svc
        .get(&slug)
        .await
        .map_err(map_app_error)?;

    // Admin can update any event; merchant can only update their own.
    let effective_merchant_id = if claims.role == "admin" {
        event.merchant_id.clone()
    } else {
        claims.user_id.clone()
    };

    // ── Jejak diagnosis ──────────────────────────────────────────────────────
    // Baris ini menjawab satu-satunya pertanyaan yang tak bisa dijawab dengan
    // membaca kode: apakah `merchant_id` yang dipakai untuk mencocokkan baris
    // benar-benar sama dengan pemilik event-nya.
    //
    // `UPDATE … WHERE id = $1 AND merchant_id = $2` mencocokkan nol baris bila
    // keduanya berbeda — dan keduanya BISA berbeda tanpa ada yang salah ketik:
    // event menyimpan pemiliknya sebagai id user merchant, tapi bila di suatu
    // pemasangan kolom itu ternyata merujuk baris tabel `merchants`, setiap
    // penyimpanan oleh merchant akan diam-diam tak mengenai apa pun.
    //
    // Ditulis pada level INFO, bukan DEBUG: yang membutuhkannya adalah orang
    // yang sedang mengejar laporan "simpan tak bisa" di server produksi, tempat
    // DEBUG hampir selalu dimatikan.
    tracing::info!(
        slug = %slug,
        event_id = %event.id,
        pemilik_event = %event.merchant_id,
        pemakai = %claims.user_id,
        peran = %claims.role,
        merchant_id_dipakai = %effective_merchant_id,
        cocok = event.merchant_id == effective_merchant_id,
        "edit event: mulai menyimpan"
    );

    let req = UpdateEventRequest {
        name: if name.is_empty() { None } else { Some(name) },
        description: if description.is_empty() {
            None
        } else {
            Some(description)
        },
        cover_url: cover_new,
        // Form belum mengirim titik fokus cover → None = pertahankan yang lama.
        // Diisi begitu editor titik fokus terpasang di halaman buat/edit.
        cover_focus: None,
        venue: if venue.is_empty() { None } else { Some(venue) },
        city: if city.is_empty() { None } else { Some(city) },
        latitude,
        longitude,
        event_date: event_date_dt,
        category: cats,
        start_time: start_time_dt,
        end_time: None,
        status: Some("edited".into()),
        detail_images: detail_imgs,
        variants: variants_update,
    };

    // Foto lama yang akan ditinggalkan — dikumpulkan SEBELUM update, karena
    // sesudahnya baris lamanya sudah tertimpa dan tak ada lagi yang tahu URL
    // mana yang dulu dipakai.
    let yatim = kumpulkan_foto_yatim(
        event.cover_url.as_deref(),
        &event.detail_images,
        cover_baru.as_deref(),
        req.detail_images.as_deref(),
    );

    if let Err(e) = state.event_svc.update(&event.id, &effective_merchant_id, req).await {
        // Digemakan ke log SEBELUM diteruskan ke klien: pesan yang sampai ke
        // layar merchant sering tak pernah dilaporkan ulang apa adanya, dan
        // tanpa salinan di server tak ada yang bisa ditelusuri sesudahnya.
        tracing::error!(
            slug = %slug,
            event_id = %event.id,
            merchant_id_dipakai = %effective_merchant_id,
            error = %e,
            "edit event: GAGAL menyimpan"
        );
        return Err(map_app_error(e));
    }
    tracing::info!(slug = %slug, "edit event: tersimpan");

    // Baru SESUDAH database menerima perubahannya, objek lamanya dibuang.
    //
    // Urutannya sengaja begini, bukan "hapus dulu baru unggah" seperti bunyi
    // permintaannya: berkas BARU-nya memang sudah diunggah lebih dulu (front-end
    // mengirim URL hasil unggah, bukan berkasnya), jadi yang tersisa hanyalah
    // membuang yang lama. Kalau dibuang sebelum update tersimpan dan update itu
    // gagal, event tetap menunjuk ke objek yang sudah tak ada — foto hilang dari
    // halaman padahal tak ada yang berubah. Hasil akhirnya sama (tak ada objek
    // yatim), tanpa jendela kehilangan foto.
    hapus_objek(&state, yatim).await;
    return Ok(());
}

/// Perbarui profil merchant (nama, deskripsi, logo, header) — sisi merchant hub.
/// String kosong utk logo/header = "jangan ubah" (pertahankan yang lama); untuk
/// mengganti, unggah dulu via POST /upload/merchant-image lalu kirim URL-nya.
#[server(UpdateMerchantProfile, "/api-fn")]
pub async fn update_merchant_profile(
    store_name: String,
    description: String,
    logo_url: String,
    header_url: String,
) -> Result<(), ServerFnError> {
    use crate::models::merchant::UpdateMerchantDetailRequest;
    let claims = require_roles(&["merchant", "admin"]).await?;
    let state = app_state().await?;
    let store_name = store_name.trim().to_string();
    if store_name.len() < 2 {
        return Err(ServerFnError::ServerError(
            "Nama bisnis minimal 2 karakter.".into(),
        ));
    }
    let req = UpdateMerchantDetailRequest {
        store_name: Some(store_name),
        // Deskripsi dikirim apa adanya (boleh dikosongkan).
        description: Some(description.trim().to_string()),
        // Kosong → None → pertahankan aset lama (tidak menghapus).
        logo_url: (!logo_url.is_empty()).then_some(logo_url),
        header_url: (!header_url.is_empty()).then_some(header_url),
    };
    state
        .merchant_svc
        .update_profile(&claims.user_id, req)
        .await
        .map(|_| ())
        .map_err(map_app_error)
}

// ── Pembersihan objek foto ────────────────────────────────────────────────────

/// Kumpulkan URL foto yang TAK LAGI dipakai event sesudah pembaruan ini.
///
/// Dipisah dari pemanggilnya supaya bisa diuji tanpa database maupun object
/// storage — inti dari benar-tidaknya pembersihan ada di sini, bukan di panggilan
/// hapusnya.
///
/// Aturannya mengikuti makna field yang sudah berlaku di server function:
///   • `cover_baru = None` → kolom cover tak disentuh (COALESCE), jadi foto
///     lamanya MASIH dipakai dan tak boleh dihapus.
///   • `detail_baru = None` → daftar foto detail tak disentuh, seluruhnya tetap.
///   • `detail_baru = Some(daftar)` → daftar itu menggantikan yang lama; yang
///     tak ada di dalamnya jadi yatim.
///
/// Foto yang dipakai ulang (URL sama muncul di daftar baru) TIDAK ikut dihapus —
/// termasuk saat sebuah foto hanya dipindah urutannya atau diganti keterangannya.
///
/// Menerima DUA potong data lama, bukan seluruh `EventWithVariants`: hanya itu
/// yang dipakai, dan struct penuhnya tak bisa dibangun di dalam uji (ia hanya
/// `Serialize`, bukan `Deserialize`). Parameter sempit = uji tanpa perancah.
///
/// `ssr` saja: `crate::models` tak dikompilasi untuk wasm32, dan fungsi ini
/// hanya dipanggil dari badan server function yang juga server-only.
#[cfg(feature = "ssr")]
fn kumpulkan_foto_yatim(
    cover_lama: Option<&str>,
    detail_lama: &[crate::models::events::DetailImageEntry],
    cover_baru: Option<&str>,
    detail_baru: Option<&[crate::models::events::DetailImageEntry]>,
) -> Vec<String> {
    let mut yatim = Vec::new();

    if let (Some(baru), Some(lama_cover)) = (cover_baru, cover_lama) {
        if baru != lama_cover && !lama_cover.is_empty() {
            yatim.push(lama_cover.to_string());
        }
    }

    if let Some(baru) = detail_baru {
        for it in detail_lama {
            let masih_dipakai = baru.iter().any(|b| b.url == it.url);
            // Cover ikut diperiksa: satu foto bisa dipakai sebagai cover DAN
            // sebagai foto detail. Menghapusnya karena hilang dari daftar detail
            // akan mematikan cover yang masih tampil.
            let jadi_cover = cover_baru == Some(it.url.as_str())
                || (cover_baru.is_none() && cover_lama == Some(it.url.as_str()));
            if !masih_dipakai && !jadi_cover {
                yatim.push(it.url.clone());
            }
        }
    }

    yatim.sort();
    yatim.dedup();
    yatim
}

/// Buang objek dari storage. Kegagalan DICATAT, bukan digagalkan ke pengguna:
/// perubahannya sudah tersimpan, dan objek yatim yang tertinggal adalah masalah
/// tagihan penyimpanan — bukan alasan memberi tahu merchant bahwa penyimpanannya
/// gagal padahal berhasil.
#[cfg(feature = "ssr")]
async fn hapus_objek(state: &std::sync::Arc<crate::state::AppState>, urls: Vec<String>) {
    for url in urls {
        if let Err(e) = state.storage.delete_by_url(&url).await {
            tracing::warn!(url = %url, error = %e, "gagal menghapus objek foto lama");
        } else {
            tracing::debug!(url = %url, "objek foto lama dihapus");
        }
    }
}

#[cfg(all(test, feature = "ssr"))]
mod tests_foto {
    use super::kumpulkan_foto_yatim;
    use crate::models::events::DetailImageEntry;

    fn foto(url: &str) -> DetailImageEntry {
        DetailImageEntry {
            url: url.into(),
            image_type: "other".into(),
            caption: String::new(),
            focus: crate::models::events::fokus_tengah(),
        }
    }
    fn daftar(urls: &[&str]) -> Vec<DetailImageEntry> {
        urls.iter().map(|u| foto(u)).collect()
    }

    /// Cover diganti → yang lama jadi yatim.
    #[test]
    fn cover_diganti_dihapus() {
        let y = kumpulkan_foto_yatim(Some("/a.jpg"), &[], Some("/b.jpg"), None);
        assert_eq!(y, vec!["/a.jpg"]);
    }

    /// Cover tak dikirim = tak disentuh → JANGAN dihapus. Ini kasus yang paling
    /// mudah salah: form yang tak mengubah cover mengirim string kosong, dan
    /// string kosong di server function berarti "pertahankan yang lama".
    #[test]
    fn cover_tak_dikirim_dipertahankan() {
        assert!(kumpulkan_foto_yatim(Some("/a.jpg"), &[], None, None).is_empty());
    }

    /// Cover diganti dengan nilai yang sama persis bukan penggantian.
    #[test]
    fn cover_sama_tak_dihapus() {
        assert!(kumpulkan_foto_yatim(Some("/a.jpg"), &[], Some("/a.jpg"), None).is_empty());
    }

    /// Satu foto detail dibuang dari daftar → objeknya ikut dibuang.
    #[test]
    fn detail_dihapus_jadi_yatim() {
        let lama = daftar(&["/1.jpg", "/2.jpg"]);
        let baru = daftar(&["/1.jpg"]);
        assert_eq!(kumpulkan_foto_yatim(None, &lama, None, Some(&baru)), vec!["/2.jpg"]);
    }

    /// Mengosongkan galeri membuang semuanya.
    #[test]
    fn semua_detail_dihapus() {
        let lama = daftar(&["/1.jpg", "/2.jpg"]);
        let y = kumpulkan_foto_yatim(None, &lama, None, Some(&[]));
        assert_eq!(y, vec!["/1.jpg", "/2.jpg"]);
    }

    /// Urutan berubah / keterangan disunting bukan penghapusan.
    #[test]
    fn urutan_berubah_tak_menghapus() {
        let lama = daftar(&["/1.jpg", "/2.jpg"]);
        let baru = daftar(&["/2.jpg", "/1.jpg"]);
        assert!(kumpulkan_foto_yatim(None, &lama, None, Some(&baru)).is_empty());
    }

    /// Foto yang juga dipakai sebagai cover tak boleh terhapus hanya karena
    /// keluar dari daftar detail — cover-nya masih tampil di halaman.
    #[test]
    fn foto_yang_jadi_cover_dipertahankan() {
        let lama = daftar(&["/1.jpg", "/2.jpg"]);
        let y = kumpulkan_foto_yatim(Some("/1.jpg"), &lama, None, Some(&[]));
        assert_eq!(y, vec!["/2.jpg"]);
    }

    /// Daftar detail tak dikirim sama sekali → tak ada yang dihapus.
    #[test]
    fn detail_tak_dikirim_dipertahankan() {
        let lama = daftar(&["/1.jpg", "/2.jpg"]);
        assert!(kumpulkan_foto_yatim(None, &lama, None, None).is_empty());
    }
}
