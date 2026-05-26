use leptos::prelude::*;
use leptos_router::components::A;

use crate::web::models::{format_date, format_price, Event};

#[component]
pub fn EventCard(event: Event) -> impl IntoView {
    let slug = event.slug.clone();
    let name = event.name.clone();
    let cats = event.category.clone();
    let city = event.city.clone();
    let venue = event.venue.clone();
    let date = format_date(&event.event_date);
    let price = format_price(event.display_price);
    let orig_price = event.sale_price.map(|_| format_price(event.price));
    let cover = event.cover_url.clone();

    view! {
        <A href=format!("/events/{slug}") attr:class="event-card fade-in">
            {if let Some(url) = cover {
                view! { <img class="event-card__img" src=url alt=name.clone() loading="lazy" /> }.into_any()
            } else {
                view! { <div class="event-card__img-placeholder">"🎪"</div> }.into_any()
            }}
            <div class="event-card__body">
                <div class="event-card__cats">
                    {cats.iter().map(|c| {
                        let c = c.clone();
                        view! { <span class="event-card__cat">{c}</span> }
                    }).collect_view()}
                </div>
                <div class="event-card__name">{name}</div>
                <div class="event-card__meta">
                    <span>"📅 " {date}</span>
                    {venue.map(|v| view! { <span>"📍 " {v}</span> })}
                    {city.map(|c| view! { <span>"🏙 " {c}</span> })}
                </div>
                <div class="event-card__footer">
                    <div>
                        {orig_price.map(|op| view! {
                            <div class="event-card__price-original">{op}</div>
                        })}
                        <div class="event-card__price">{price}</div>
                    </div>
                    <span class="badge badge--accent">"Beli Tiket"</span>
                </div>
            </div>
        </A>
    }
}
