use leptos::prelude::*;

use crate::web::models::Banner;

pub(super) fn view_banners(banners: Vec<Banner>) -> impl IntoView {
    view! {
        <section class="mhub-events-section">
            <div class="mhub-events-header">
                <h3 class="mhub-events-title">"Pengelolaan Spanduk"</h3>
            </div>
            {if banners.is_empty() {
                view! {
                    <div class="mhub-empty">
                        <div class="mhub-empty-icon-wrap">
                            <svg width="38" height="38" viewBox="0 0 24 24" fill="none"
                                 stroke="currentColor" stroke-width="1.5" stroke-linecap="round">
                                <rect x="3" y="3" width="18" height="18" rx="2"/>
                                <path d="M3 9h18M9 21V9"/>
                            </svg>
                        </div>
                        <p class="mhub-empty-title">"Belum Ada Spanduk"</p>
                        <p class="mhub-empty-body">"Belum ada banner yang dikonfigurasi."</p>
                    </div>
                }.into_any()
            } else {
                banners.into_iter().map(|b| {
                    let img  = b.image_url.clone();
                    let link = b.link_url.clone().unwrap_or_default();
                    view! {
                        <div class="admin-banner-row">
                            <div class="admin-banner-thumb">
                                {if img.is_empty() {
                                    view! { <div class="admin-banner-no-img">"🖼"</div> }.into_any()
                                } else {
                                    view! {
                                        <img src=img alt=format!("Banner #{}", b.id)
                                             class="admin-banner-img"/>
                                    }.into_any()
                                }}
                            </div>
                            <div class="admin-banner-info">
                                <p class="admin-banner-title">"Spanduk #"{b.id}</p>
                                {(!link.is_empty()).then(|| view! {
                                    <p class="admin-banner-link">{link}</p>
                                })}
                                {b.title.as_ref().map(|t| view! {
                                    <p class="admin-banner-order">{t.clone()}</p>
                                })}
                            </div>
                        </div>
                    }
                }).collect_view().into_any()
            }}
        </section>
    }
}
