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
