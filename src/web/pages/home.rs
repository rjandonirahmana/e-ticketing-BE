//! web/pages/home.rs — Beranda (`/`): tab "Event" (ExplorePage, tak berubah)
//! berdampingan dengan tab "Hub" (MarketplaceFeed, C2C jual-beli COD).
//!
//! Isi berkas ini SEBELUMNYA adalah landing page lama (hero + `get_products`/
//! `ProductCard`) yang sudah tak terpasang di router mana pun sejak lama —
//! kode mati memakai API yang sudah kalah dari `ExploreProduct`/`ProductCardPub`.
//! Ditulis ulang total, bukan diperluas.
//!
//! Tab 0 (Event) default & dirender lewat `<Show>` — SSR halaman `/` tetap
//! identik perilakunya hari ini (ExplorePage penuh di HTML pertama). Tab 1
//! (Hub) baru mount saat dipilih, murni klien.

use leptos::prelude::*;

use crate::web::components::{SwipeTabBar, TabItem, TabSwipe};
use crate::web::pages::explore::ExplorePage;
use crate::web::pages::marketplace::MarketplaceFeed;

#[component]
pub fn HomePage() -> impl IntoView {
    let swipe = TabSwipe::new(2);
    let tabs = vec![TabItem::new("Event"), TabItem::new("Hub")];

    view! {
        <div class="home-tabs-wrap">
            <SwipeTabBar swipe=swipe tabs=tabs />
        </div>
        <Show when=move || swipe.index() == 0 fallback=|| view! { <MarketplaceFeed /> }>
            <ExplorePage />
        </Show>
    }
}
