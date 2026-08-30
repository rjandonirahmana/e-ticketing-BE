//! web/api/server_fns/cart.rs — keranjang & pembayaran untuk halaman Leptos.
//!
//! Setiap fungsi di sini mengembalikan SELURUH isi keranjang, bukan sekadar
//! "berhasil". Itu disengaja: satu perjalanan bolak-balik sudah cukup untuk
//! menaikkan jumlah tiket sekaligus memperbarui subtotal, potongan, lencana
//! navigasi, dan peringatan stok — halaman tak perlu memanggil `get_cart()`
//! lagi sesudahnya, dan tak pernah ada jendela waktu ketika angka di layar
//! berasal dari dua keadaan yang berbeda.

use leptos::prelude::*;

#[cfg_attr(not(feature = "ssr"), allow(unused_imports))]
use crate::web::models::{CartView, PaymentOptions};
#[cfg(feature = "ssr")]
use super::helpers::*;

// ── Konversi model server → model web ────────────────────────────────────────

#[cfg(feature = "ssr")]
pub(super) fn srv_cart_to_web(c: crate::models::cart::CartView) -> CartView {
    use crate::web::models::CartItemView;
    use rust_decimal::prelude::ToPrimitive;

    CartView {
        cart_id: c.cart_id,
        items: c
            .items
            .into_iter()
            .map(|i| CartItemView {
                id: i.id,
                tier_id: i.ticket_variant_id,
                event_id: i.event_id,
                event_slug: i.event_slug,
                event_title: i.event_name,
                tier_name: i.variant_name,
                venue_name: i.venue,
                event_cover: i.cover_url,
                event_date: i.event_date,
                merchant_id: i.merchant_id,
                merchant_name: i.merchant_name,
                quantity: i.quantity,
                unit_price: i.unit_price.to_i64().unwrap_or(0),
                unit_price_snapshot: i.unit_price_snapshot.to_i64().unwrap_or(0),
                subtotal: i.subtotal.to_i64().unwrap_or(0),
                available: i.available,
                max_per_order: i.max_per_order,
                exceeds_stock: i.exceeds_stock,
                price_changed: i.price_changed,
                selected: i.selected,
            })
            .collect(),
        subtotal: c.subtotal.to_i64().unwrap_or(0),
        discount: c.discount.to_i64().unwrap_or(0),
        promo_code: c.promo_code,
        promo_message: c.promo_message,
        payment_code: c.payment_code,
        total_quantity: c.total_quantity,
        cart_quantity: c.cart_quantity,
        total: c.total.to_i64().unwrap_or(0),
        notif: c.notif,
    }
}

/// Status premium ikut menentukan promo mana yang berlaku, jadi ia dibaca sekali
/// di sini alih-alih diulang di setiap fungsi.
#[cfg(feature = "ssr")]
async fn premium_of(
    state: &std::sync::Arc<crate::state::AppState>,
    user_id: &str,
) -> bool {
    state.story_svc.is_premium(user_id).await.unwrap_or(false)
}

// ── Baca ─────────────────────────────────────────────────────────────────────

#[server(GetCart, "/api-fn")]
pub async fn get_cart() -> Result<CartView, ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    let premium = premium_of(&state, &claims.user_id).await;
    let cart = state
        .cart_svc
        .view(&claims.user_id, premium)
        .await
        .map_err(map_app_error)?;
    return Ok(srv_cart_to_web(cart));
}

// ── Tulis ────────────────────────────────────────────────────────────────────

#[server(AddToCart, "/api-fn")]
pub async fn add_to_cart(tier_id: String, quantity: i32) -> Result<CartView, ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    let premium = premium_of(&state, &claims.user_id).await;
    let cart = state
        .cart_svc
        .add(&claims.user_id, &tier_id, quantity, premium)
        .await
        .map_err(map_app_error)?;
    return Ok(srv_cart_to_web(cart));
}

/// Tetapkan jumlah sebuah baris; `quantity = 0` menghapusnya.
#[server(UpdateCartQuantity, "/api-fn")]
pub async fn update_cart_quantity(
    tier_id: String,
    quantity: i32,
) -> Result<CartView, ServerFnError> {
    use crate::models::cart::UpdateCartItemRequest;

    let claims = auth_claims().await?;
    let state = app_state().await?;
    let premium = premium_of(&state, &claims.user_id).await;
    let cart = state
        .cart_svc
        .update_quantity(
            &claims.user_id,
            UpdateCartItemRequest {
                ticket_variant_id: tier_id,
                quantity,
            },
            premium,
        )
        .await
        .map_err(map_app_error)?;
    return Ok(srv_cart_to_web(cart));
}

/// Centang / lepas centang satu baris. `tier_id` kosong berarti seluruh isi
/// keranjang sekaligus — dipakai kotak "pilih semua".
#[server(SelectCartItem, "/api-fn")]
pub async fn select_cart_item(
    tier_id: Option<String>,
    selected: bool,
) -> Result<CartView, ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    let premium = premium_of(&state, &claims.user_id).await;
    let cart = state
        .cart_svc
        .set_selected(
            &claims.user_id,
            tier_id.as_deref().filter(|s| !s.is_empty()),
            selected,
            premium,
        )
        .await
        .map_err(map_app_error)?;
    return Ok(srv_cart_to_web(cart));
}

/// Centang/lepas seluruh barang milik satu toko dalam SATU permintaan.
#[server(SelectCartItems, "/api-fn")]
pub async fn select_cart_items(
    tier_ids: Vec<String>,
    selected: bool,
) -> Result<CartView, ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    let premium = premium_of(&state, &claims.user_id).await;
    let cart = state
        .cart_svc
        .set_selected_many(&claims.user_id, &tier_ids, selected, premium)
        .await
        .map_err(map_app_error)?;
    return Ok(srv_cart_to_web(cart));
}

#[server(ClearCart, "/api-fn")]
pub async fn clear_cart() -> Result<CartView, ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    let premium = premium_of(&state, &claims.user_id).await;
    let cart = state
        .cart_svc
        .clear(&claims.user_id, premium)
        .await
        .map_err(map_app_error)?;
    return Ok(srv_cart_to_web(cart));
}

/// Tuang keranjang tamu (localStorage) ke keranjang milik user setelah login.
///
/// `items_json` berbentuk `[{"tier_id": "...", "quantity": 2}, …]` — bentuk yang
/// sama dengan yang selama ini disimpan browser, jadi data lama pengguna tetap
/// terbaca. Digabung, bukan menimpa: keranjang dari perangkat lain tak hilang
/// hanya karena seseorang membuka situs di komputer baru.
#[server(SyncGuestCart, "/api-fn")]
pub async fn sync_guest_cart(items_json: String) -> Result<CartView, ServerFnError> {
    use crate::models::cart::{CartItemInput, SaveCartRequest};

    #[derive(serde::Deserialize)]
    struct GuestLine {
        tier_id: String,
        #[serde(default = "one")]
        quantity: i32,
    }
    fn one() -> i32 {
        1
    }

    let claims = auth_claims().await?;
    let state = app_state().await?;
    let premium = premium_of(&state, &claims.user_id).await;

    let lines: Vec<GuestLine> = serde_json::from_str(&items_json).unwrap_or_default();
    let items: Vec<CartItemInput> = lines
        .into_iter()
        .filter(|l| l.quantity > 0)
        .map(|l| CartItemInput {
            ticket_variant_id: l.tier_id,
            quantity: l.quantity.clamp(1, 100),
        })
        .collect();

    let cart = state
        .cart_svc
        .save(
            &claims.user_id,
            SaveCartRequest {
                items,
                replace: false,
                ..Default::default()
            },
            premium,
        )
        .await
        .map_err(map_app_error)?;
    return Ok(srv_cart_to_web(cart));
}

/// Pasang kode promo; `code = None` melepasnya.
#[server(ApplyCartPromo, "/api-fn")]
pub async fn apply_cart_promo(code: Option<String>) -> Result<CartView, ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    let premium = premium_of(&state, &claims.user_id).await;
    let cart = state
        .cart_svc
        .set_promo(&claims.user_id, code.as_deref(), premium)
        .await
        .map_err(map_app_error)?;
    return Ok(srv_cart_to_web(cart));
}

/// Simpan kanal pembayaran pilihan user pada keranjangnya.
#[server(SelectPaymentMethod, "/api-fn")]
pub async fn select_payment_method(code: String) -> Result<CartView, ServerFnError> {
    let claims = auth_claims().await?;
    let state = app_state().await?;
    let premium = premium_of(&state, &claims.user_id).await;
    let cart = state
        .cart_svc
        .set_payment(&claims.user_id, Some(&code), premium)
        .await
        .map_err(map_app_error)?;
    return Ok(srv_cart_to_web(cart));
}

// ── Kanal pembayaran ─────────────────────────────────────────────────────────

/// Kanal yang tersedia untuk isi keranjang saat ini, lengkap dengan biaya admin
/// dan total akhir per kanal.
///
/// Nominalnya dihitung server dari keranjang — halaman tidak mengirim angka apa
/// pun, sehingga tak ada cara bagi klien untuk meminta biaya yang lebih murah.
#[server(GetPaymentOptions, "/api-fn")]
pub async fn get_payment_options() -> Result<PaymentOptions, ServerFnError> {
    use crate::web::models::PaymentMethodView;
    use rust_decimal::prelude::ToPrimitive;

    let claims = auth_claims().await?;
    let state = app_state().await?;
    let premium = premium_of(&state, &claims.user_id).await;

    let cart = state
        .cart_svc
        .view(&claims.user_id, premium)
        .await
        .map_err(map_app_error)?;

    let amount = cart.total;
    let methods = state
        .payment_svc
        .list_for(amount, cart.promo_code.is_some())
        .await
        .map_err(map_app_error)?;

    return Ok(PaymentOptions {
        methods: methods
            .into_iter()
            .map(|m| {
                let charge = m.charge_for(amount);
                PaymentMethodView {
                    code: m.code,
                    name: m.name,
                    vendor: m.vendor,
                    category: m.category,
                    image_url: m.image_url,
                    description: m.description,
                    charge: charge.to_i64().unwrap_or(0),
                    total: (amount + charge).to_i64().unwrap_or(0),
                    is_instant: m.is_instant,
                    instruction: m.instruction,
                }
            })
            .collect(),
        amount: amount.to_i64().unwrap_or(0),
        selected: cart.payment_code,
    });
}
