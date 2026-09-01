//! Ditambah: `ProductStatus`, lencana status produk yang dipakai dasbor
//! merchant DAN dasbor admin. Keduanya dulu menyimpan salinan enum + tiga
//! metodenya sendiri-sendiri — termasuk aturan "habis terjual" (`total_sold >=
//! total_quota`) yang menentukan apa yang boleh dibeli orang. Aturan seperti
//! itu yang paling mahal bila dua salinannya menyimpang: satu dasbor akan
//! menyatakan habis sementara yang lain masih menawarkannya.

use crate::web::models::Product;

#[derive(Clone, PartialEq)]
pub enum ProductStatus {
    OnSale,
    SoldOut,
    Presale,
}

impl ProductStatus {
    pub fn from_product(e: &Product) -> Self {
        if e.total_quota > 0 && e.total_sold >= e.total_quota {
            Self::SoldOut
        } else if e.status == "active" {
            Self::OnSale
        } else {
            Self::Presale
        }
    }

    pub fn css_mod(&self) -> &'static str {
        match self {
            Self::OnSale => "mhub-product-status mhub-product-status--sale",
            Self::SoldOut => "mhub-product-status mhub-product-status--sold",
            Self::Presale => "mhub-product-status mhub-product-status--presale",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::OnSale => "Dijual",
            Self::SoldOut => "Habis Terjual",
            Self::Presale => "Pre-order",
        }
    }
}

// ─── Skeleton Component ─────────────────────────────────────────────────────

use leptos::prelude::*;

#[component]
pub fn MerchantProductSkeleton() -> impl IntoView {
    view! {
        <div class="medit-container">
            // INFO DASAR
            <div class="medit-section-header">
                <div class="medit-shimmer medit-shimmer--section"></div>
            </div>
            <div class="medit-shimmer medit-shimmer--input"></div>
            <div class="medit-shimmer medit-shimmer--textarea"></div>

            // KATEGORI
            <div class="medit-section-header" style="margin-top:4px">
                <div class="medit-shimmer medit-shimmer--section"></div>
            </div>
            <div class="medit-shimmer-grid">
                <div class="medit-shimmer medit-shimmer--chip"></div>
                <div class="medit-shimmer medit-shimmer--chip"></div>
                <div class="medit-shimmer medit-shimmer--chip"></div>
                <div class="medit-shimmer medit-shimmer--chip"></div>
                <div class="medit-shimmer medit-shimmer--chip"></div>
            </div>

            // FOTO COVER
            <div class="medit-section-header" style="margin-top:4px">
                <div class="medit-shimmer medit-shimmer--section"></div>
            </div>
            <div class="medit-shimmer medit-shimmer--cover"></div>

            // TANGGAL & WAKTU
            <div class="medit-shimmer medit-shimmer--input"></div>
            <div class="medit-shimmer-grid-2">
                <div class="medit-shimmer medit-shimmer--input"></div>
                <div class="medit-shimmer medit-shimmer--input"></div>
            </div>

            // VENUE & KOTA
            <div class="medit-shimmer medit-shimmer--input"></div>
            <div class="medit-shimmer medit-shimmer--input"></div>

            // FOTO DETAIL
            <div class="medit-section-header" style="margin-top:4px">
                <div class="medit-shimmer medit-shimmer--section"></div>
            </div>
            <div class="medit-shimmer-grid-3">
                <div class="medit-shimmer medit-shimmer--square"></div>
                <div class="medit-shimmer medit-shimmer--square"></div>
                <div class="medit-shimmer medit-shimmer--square"></div>
            </div>

            // TICKET VARIANTS
            <div class="medit-section-header" style="margin-top:4px">
                <div class="medit-shimmer medit-shimmer--section"></div>
            </div>
            <div class="medit-shimmer medit-shimmer--card"></div>

            // Spacer buat sticky footer
            <div style="height:90px"></div>
        </div>

        // Sticky footer skeleton
        <div class="medit-sticky-footer">
            <div class="medit-shimmer medit-shimmer--btn"></div>
        </div>
    }
}
