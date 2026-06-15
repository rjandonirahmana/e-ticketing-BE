use leptos::prelude::*;
use leptos_router::components::A;

#[component]
pub fn EventCard(
    href: String,
    img: String,
    #[prop(into)] alt: String,
    #[prop(into)] badge: String,
    #[prop(into)] title: String,
    #[prop(optional)] date: Option<String>,
    #[prop(into)] venue: String,
    #[prop(into)] price: String,
) -> impl IntoView {
    view! {
        <A href=href attr:class="explore-event-card-v2">
            <div class="explore-ecard-img-wrap">
                <img src=img alt=alt class="explore-ecard-img" />
                <span class="explore-ecard-cat">{badge}</span>
            </div>
            <div class="explore-ecard-body">
                <h3 class="explore-ecard-title">{title}</h3>
                {date
                    .map(|d| {
                        view! {
                            <div class="explore-ecard-meta">
                                <svg
                                    width="11"
                                    height="11"
                                    viewBox="0 0 24 24"
                                    fill="none"
                                    stroke="currentColor"
                                    stroke-width="2"
                                    stroke-linecap="round"
                                >
                                    <rect x="3" y="4" width="18" height="18" rx="2" />
                                    <line x1="16" y1="2" x2="16" y2="6" />
                                    <line x1="8" y1="2" x2="8" y2="6" />
                                    <line x1="3" y1="10" x2="21" y2="10" />
                                </svg>
                                <span>{d}</span>
                            </div>
                        }
                    })}
                <div class="explore-ecard-meta">
                    <svg
                        width="11"
                        height="11"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="2"
                        stroke-linecap="round"
                    >
                        <path d="M21 10c0 7-9 13-9 13s-9-6-9-13a9 9 0 0118 0z" />
                        <circle cx="12" cy="10" r="3" />
                    </svg>
                    <span>{venue}</span>
                </div>
                <div class="explore-ecard-footer">
                    <span class="explore-ecard-price">{price}</span>
                    <span class="explore-ecard-arrow">
                        <svg
                            width="14"
                            height="14"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            stroke-width="2.5"
                            stroke-linecap="round"
                        >
                            <line x1="5" y1="12" x2="19" y2="12" />
                            <polyline points="12 5 19 12 12 19" />
                        </svg>
                    </span>
                </div>
            </div>
        </A>
    }
}

#[component]
pub fn EventCardShimmer() -> impl IntoView {
    view! {
        <div class="shim-event-card">
            <div class="shim shim-img"></div>
            <div class="shim-body">
                <div class="shim shim-cat"></div>
                <div class="shim shim-title"></div>
                <div class="shim shim-date"></div>
                <div class="shim-foot">
                    <div class="shim shim-venue"></div>
                    <div class="shim shim-price"></div>
                </div>
            </div>
        </div>
    }
}

#[component]
pub fn TicketCardShimmer() -> impl IntoView {
    view! {
        <div class="shim-ticket-card">
            <div class="shim shim-tkt-title"></div>
            <div class="shim shim-tkt-venue"></div>
            <div class="shim shim-tkt-date"></div>
            <div class="shim shim-tkt-badge"></div>
        </div>
    }
}

#[component]
pub fn OrderCardShimmer() -> impl IntoView {
    view! {
        <div class="shim-order-card">
            <div class="shim-order-card-top">
                <div class="shim shim-ord-thumb"></div>
                <div class="shim-ord-info">
                    <div class="shim shim-ord-name"></div>
                    <div class="shim shim-ord-date"></div>
                </div>
            </div>
            <div class="shim shim-ord-divider"></div>
            <div class="shim-foot">
                <div class="shim shim-ord-amount"></div>
                <div class="shim shim-ord-btn"></div>
            </div>
        </div>
    }
}

#[component]
pub fn MerchantRowShimmer() -> impl IntoView {
    view! {
        <div class="shim-merchant-row">
            <div class="shim-mr-info">
                <div class="shim shim-mr-name"></div>
                <div class="shim shim-mr-venue"></div>
            </div>
            <div class="shim shim-mr-badge"></div>
        </div>
    }
}

#[component]
pub fn MessageRowShimmer() -> impl IntoView {
    view! {
        <div class="shim-msg-row">
            <div class="shim shim-avatar"></div>
            <div class="shim-msg-info">
                <div class="shim shim-msg-name"></div>
                <div class="shim shim-msg-preview"></div>
            </div>
        </div>
    }
}
