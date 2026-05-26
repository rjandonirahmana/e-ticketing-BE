//! ticket_detail.rs — Halaman Detail Tiket dengan QR Code (SSR).

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;

use crate::web::api::get_ticket_detail;
use crate::web::app::AuthResource;
use crate::web::models::format_price;
use crate::web::pages::tickets::qr_svg_html;

#[component]
pub fn TicketDetailPage() -> impl IntoView {
    let params = use_params_map();
    let ticket_id = move || params.read().get("id").unwrap_or_default();

    let auth = use_context::<AuthResource>().expect("AuthResource missing");
    let is_logged_in = move || auth.get().and_then(|r| r.ok()).flatten().is_some();

    let ticket = Resource::new(
        move || (ticket_id(), is_logged_in()),
        |(id, logged_in)| async move {
            if logged_in && !id.is_empty() {
                get_ticket_detail(id).await
            } else {
                Err(ServerFnError::ServerError("not_ready".into()))
            }
        },
    );

    view! {
        <div class="container" style="padding-top:2rem;padding-bottom:4rem;max-width:520px">
            <div style="margin-bottom:1.5rem">
                <A href="/tickets" attr:class="btn btn--ghost btn--sm">"← Tiket Ku"</A>
            </div>

            <Suspense fallback=|| view! {
                <div class="loading" style="min-height:60vh">
                    <div class="loading__spinner"/>
                    <span>"Memuat tiket..."</span>
                </div>
            }>
                {move || {
                    if !is_logged_in() && auth.get().is_some() {
                        return view! {
                            <div style="text-align:center;padding:4rem 0">
                                <A href="/login" attr:class="btn btn--accent">"Masuk"</A>
                            </div>
                        }.into_any();
                    }

                    ticket.get().map(|res| match res {
                        Err(e) if e.to_string().contains("not_ready") => view! { <div/> }.into_any(),
                        Err(_) => view! {
                            <div class="empty" style="min-height:40vh">
                                <div class="empty__icon">"😕"</div>
                                <div class="empty__title">"Tiket tidak ditemukan"</div>
                                <A href="/tickets" attr:class="btn btn--ghost" attr:style="margin-top:1rem">"Kembali"</A>
                            </div>
                        }.into_any(),
                        Ok(t) => {
                            use chrono::Timelike;
                            let (date_str, time_str) = {
                                let wib_offset = chrono::FixedOffset::east_opt(7 * 3600).unwrap();
                                let wib = t.event_date.with_timezone(&wib_offset);
                                let months = ["Jan","Feb","Mar","Apr","Mei","Jun","Jul","Agu","Sep","Okt","Nov","Des"];
                                use chrono::Datelike;
                                let d = format!("{} {} {}", wib.day(), months[wib.month() as usize - 1], wib.year());
                                let ti = format!("{:02}:{:02} WIB", wib.hour(), wib.minute());
                                (d, ti)
                            };

                            let venue = match (&t.event_venue, &t.event_city) {
                                (Some(v), Some(c)) => format!("{}, {}", v, c),
                                (Some(v), None) => v.clone(),
                                (None, Some(c)) => c.clone(),
                                _ => "TBA".to_string(),
                            };

                            let cover = t.cover_url.clone().unwrap_or_else(||
                                "https://images.unsplash.com/photo-1470225620780-dba8ba36b745?w=800&q=80".into()
                            );

                            let status_badge = t.status.to_uppercase();
                            let status_cls = if t.status == "used" { "badge badge--muted" } else { "badge badge--success" };
                            let qr_html = qr_svg_html(&t.ticket_code, 180);
                            let code = t.ticket_code.clone();
                            let event_name = t.event_name.clone();
                            let var = t.variant_name.clone();
                            let price = format_price(t.unit_price);

                            view! {
                                <div style="background:var(--clr-surface);border:1px solid var(--clr-border);border-radius:16px;overflow:hidden">
                                    // Hero
                                    <div style="position:relative;height:200px;overflow:hidden">
                                        <img src=cover alt=event_name.clone()
                                            style="width:100%;height:100%;object-fit:cover"/>
                                        <div style="position:absolute;inset:0;background:linear-gradient(to bottom,transparent 30%,rgba(5,8,20,.9))"/>
                                        <div style="position:absolute;bottom:1rem;left:1.25rem;right:1.25rem">
                                            <span class=status_cls style="margin-bottom:.5rem">{status_badge}</span>
                                            <h1 style="font-size:1.1rem;font-weight:700;color:white;margin-top:.4rem">{event_name}</h1>
                                        </div>
                                    </div>

                                    // Stub top
                                    <div style="padding:1.25rem;border-bottom:1px dashed var(--clr-border)">
                                        <div style="display:grid;grid-template-columns:1fr 1fr;gap:.75rem;margin-bottom:.75rem">
                                            <div>
                                                <p style="font-size:.65rem;color:var(--clr-muted);text-transform:uppercase;letter-spacing:.06em">"TICKET REF"</p>
                                                <p style="font-family:var(--font-mono);font-size:.875rem;font-weight:700">{code.clone()}</p>
                                            </div>
                                            <div style="text-align:right">
                                                <p style="font-size:.65rem;color:var(--clr-muted);text-transform:uppercase;letter-spacing:.06em">"PRICE PAID"</p>
                                                <p style="font-family:var(--font-display);color:var(--clr-accent)">{price}</p>
                                            </div>
                                            <div>
                                                <p style="font-size:.65rem;color:var(--clr-muted);text-transform:uppercase;letter-spacing:.06em">"DATE"</p>
                                                <p style="font-size:.875rem">{date_str}</p>
                                            </div>
                                            <div style="text-align:right">
                                                <p style="font-size:.65rem;color:var(--clr-muted);text-transform:uppercase;letter-spacing:.06em">"TIME"</p>
                                                <p style="font-size:.875rem">{time_str}</p>
                                            </div>
                                            <div style="grid-column:1/-1">
                                                <p style="font-size:.65rem;color:var(--clr-muted);text-transform:uppercase;letter-spacing:.06em">"VENUE"</p>
                                                <p style="font-size:.875rem">{venue}</p>
                                            </div>
                                        </div>
                                    </div>

                                    // QR + section
                                    <div style="padding:1.5rem;display:flex;flex-direction:column;align-items:center;gap:1rem">
                                        <div style="background:white;border-radius:12px;padding:8px" inner_html=qr_html/>
                                        <p style="font-size:.75rem;color:var(--clr-muted);font-family:var(--font-mono)">
                                            {"TICKET#"}{code}
                                        </p>
                                        <div style="display:grid;grid-template-columns:1fr 1fr;gap:.75rem;width:100%">
                                            <div style="text-align:center;background:var(--clr-bg);border-radius:8px;padding:.75rem">
                                                <p style="font-size:.65rem;color:var(--clr-muted);text-transform:uppercase;letter-spacing:.06em">"SECTION"</p>
                                                <p style="font-weight:700;font-size:.9rem">{var}</p>
                                            </div>
                                            <div style="text-align:center;background:var(--clr-bg);border-radius:8px;padding:.75rem">
                                                <p style="font-size:.65rem;color:var(--clr-muted);text-transform:uppercase;letter-spacing:.06em">"ROW/SEAT"</p>
                                                <p style="font-weight:700;font-size:.9rem">"-"</p>
                                            </div>
                                        </div>

                                        // Security notice
                                        <div style="background:rgba(200,255,94,.05);border:1px solid rgba(200,255,94,.2);border-radius:10px;padding:1rem;width:100%">
                                            <p style="font-size:.8rem;color:var(--clr-muted);line-height:1.5">
                                                "🛡 Tunjukkan QR code ini di pintu masuk. "
                                                "⚠ Jangan bagikan kode ini ke orang lain."
                                            </p>
                                        </div>
                                    </div>
                                </div>
                            }.into_any()
                        }
                    }).unwrap_or_else(|| view! { <div/> }.into_any())
                }}
            </Suspense>
        </div>
    }
}
