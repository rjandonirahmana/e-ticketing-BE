//! admin.rs — Halaman Admin Panel (SSR).

use leptos::prelude::*;
use leptos_router::components::A;

use crate::web::api::{get_admin_stats, get_admin_users, get_admin_orders};
use crate::web::app::AuthResource;
use crate::web::models::format_price;

#[component]
pub fn AdminPage() -> impl IntoView {
    let auth = use_context::<AuthResource>().expect("AuthResource missing");

    let is_admin = move || {
        auth.get()
            .and_then(|r| r.ok())
            .flatten()
            .map(|u| u.role == "admin")
            .unwrap_or(false)
    };

    let stats = Resource::new(
        move || is_admin(),
        |ok| async move { if ok { get_admin_stats().await } else { Err(ServerFnError::ServerError("denied".into())) } },
    );

    let tab = RwSignal::new("users".to_string());
    let page = RwSignal::new(1i64);

    let users = Resource::new(
        move || (is_admin(), tab.get(), page.get()),
        |(ok, t, p)| async move {
            if ok && t == "users" { get_admin_users(Some(p)).await }
            else { Ok(serde_json::Value::Null) }
        },
    );

    let orders = Resource::new(
        move || (is_admin(), tab.get(), page.get()),
        |(ok, t, p)| async move {
            if ok && t == "orders" { get_admin_orders(Some(p)).await }
            else { Ok(serde_json::Value::Null) }
        },
    );

    view! {
        <div class="page-header">
            <div class="container">
                <p class="page-header__eyebrow">"// admin panel"</p>
                <h1 class="page-header__title">"Admin Dashboard"</h1>
            </div>
        </div>

        <div class="container" style="padding-bottom:4rem">
            <Suspense fallback=|| view! {
                <div class="loading"><div class="loading__spinner"/></div>
            }>
                {move || {
                    if !is_admin() && auth.get().is_some() {
                        return view! {
                            <div class="alert alert--error">
                                "Akses ditolak. Halaman ini hanya untuk administrator."
                            </div>
                        }.into_any();
                    }

                    view! {
                        <div>
                            // ── Stats ──────────────────────────────────────────
                            {move || stats.get().map(|res| match res {
                                Ok(s) => view! {
                                    <div class="merchant-stats" style="margin-bottom:2rem">
                                        <div class="stat-card">
                                            <div class="stat-card__label">"Total User"</div>
                                            <div class="stat-card__value">{s.total_users}</div>
                                        </div>
                                        <div class="stat-card">
                                            <div class="stat-card__label">"Total Event"</div>
                                            <div class="stat-card__value">{s.total_events}</div>
                                        </div>
                                        <div class="stat-card">
                                            <div class="stat-card__label">"Total Order"</div>
                                            <div class="stat-card__value">{s.total_orders}</div>
                                        </div>
                                        <div class="stat-card">
                                            <div class="stat-card__label">"Total Revenue"</div>
                                            <div class="stat-card__value">{format_price(s.total_revenue)}</div>
                                        </div>
                                    </div>
                                }.into_any(),
                                Err(_) => view! { <div/> }.into_any(),
                            }).unwrap_or_else(|| view!{<div/>}.into_any())}

                            // ── Tabs ───────────────────────────────────────────
                            <div style="display:flex;gap:.5rem;margin-bottom:1.5rem">
                                {[("users","Pengguna"), ("orders","Order"), ("events","Event")].iter().map(|(t, label)| {
                                    let t_val = *t;
                                    view! {
                                        <button
                                            class=move || if tab.get() == t_val { "btn btn--accent btn--sm" } else { "btn btn--ghost btn--sm" }
                                            on:click=move |_| { tab.set(t_val.to_string()); page.set(1); }
                                        >{*label}</button>
                                    }
                                }).collect_view()}
                            </div>

                            // ── Users Tab ─────────────────────────────────────
                            {move || (tab.get() == "users").then(|| view! {
                                <Suspense fallback=|| view!{<div class="loading"><div class="loading__spinner"/></div>}>
                                    {move || users.get().map(|res| match res {
                                        Ok(data) => {
                                            let rows = data["data"].as_array().cloned().unwrap_or_default();
                                            let rows_empty = rows.is_empty();
                                            view! {
                                                <div style="overflow-x:auto">
                                                    <table style="width:100%;border-collapse:collapse;font-size:.875rem">
                                                        <thead>
                                                            <tr style="border-bottom:1px solid var(--clr-border);color:var(--clr-muted);font-size:.75rem;text-transform:uppercase">
                                                                <th style="padding:.625rem;text-align:left">"Nama"</th>
                                                                <th style="padding:.625rem;text-align:left">"No. HP"</th>
                                                                <th style="padding:.625rem;text-align:left">"Role"</th>
                                                                <th style="padding:.625rem;text-align:left">"Terdaftar"</th>
                                                            </tr>
                                                        </thead>
                                                        <tbody>
                                                            {rows.into_iter().map(|u| {
                                                                let name = u["name"].as_str().unwrap_or("-").to_owned();
                                                                let phone = u["phone"].as_str().unwrap_or("-").to_owned();
                                                                let role_s = u["role"].as_str().unwrap_or("").to_owned();
                                                                let role_d = u["role"].as_str().unwrap_or("-").to_owned();
                                                                let created = u["created_at"].as_str().unwrap_or("-").get(..10).unwrap_or("-").to_owned();
                                                                let badge = if role_s == "admin" { "badge badge--accent".to_owned() }
                                                                    else if role_s == "merchant" { "badge badge--blue".to_owned() }
                                                                    else { "badge badge--muted".to_owned() };
                                                                view! {
                                                                    <tr style="border-bottom:1px solid var(--clr-border)">
                                                                        <td style="padding:.625rem">{name}</td>
                                                                        <td style="padding:.625rem;font-family:var(--font-mono)">{phone}</td>
                                                                        <td style="padding:.625rem"><span class=badge>{role_d}</span></td>
                                                                        <td style="padding:.625rem;color:var(--clr-muted)">{created}</td>
                                                                    </tr>
                                                                }
                                                            }).collect_view()}
                                                        </tbody>
                                                    </table>
                                                    {if rows_empty {
                                                        view! { <div class="empty"><div class="empty__icon">"👤"</div><div class="empty__title">"Tidak ada data"</div></div> }.into_any()
                                                    } else {
                                                        view! { <span/> }.into_any()
                                                    }}
                                                </div>
                                            }.into_any()
                                        }
                                        Err(_) => view! { <div class="alert alert--error">"Gagal memuat pengguna."</div> }.into_any(),
                                    }).unwrap_or_else(|| view!{<div/>}.into_any())}
                                </Suspense>
                            })}

                            // ── Orders Tab ────────────────────────────────────
                            {move || (tab.get() == "orders").then(|| view! {
                                <Suspense fallback=|| view!{<div class="loading"><div class="loading__spinner"/></div>}>
                                    {move || orders.get().map(|res| match res {
                                        Ok(data) => {
                                            let rows = data["data"].as_array().cloned().unwrap_or_default();
                                            let rows_empty = rows.is_empty();
                                            view! {
                                                <div style="overflow-x:auto">
                                                    <table style="width:100%;border-collapse:collapse;font-size:.875rem">
                                                        <thead>
                                                            <tr style="border-bottom:1px solid var(--clr-border);color:var(--clr-muted);font-size:.75rem;text-transform:uppercase">
                                                                <th style="padding:.625rem;text-align:left">"Kode"</th>
                                                                <th style="padding:.625rem;text-align:left">"Status"</th>
                                                                <th style="padding:.625rem;text-align:right">"Total"</th>
                                                                <th style="padding:.625rem;text-align:left">"Tanggal"</th>
                                                            </tr>
                                                        </thead>
                                                        <tbody>
                                                            {rows.into_iter().map(|o| {
                                                                let code = o["order_code"].as_str().unwrap_or("-").to_owned();
                                                                let total = o["total_amount"].as_f64().unwrap_or(0.0);
                                                                let created = o["created_at"].as_str().unwrap_or("-").get(..10).unwrap_or("-").to_owned();
                                                                let status_s = o["status"].as_str().unwrap_or("-").to_owned();
                                                                let status_cls = if status_s.eq_ignore_ascii_case("paid") { "badge badge--success".to_owned() }
                                                                    else if status_s.to_lowercase().contains("pending") || status_s.to_lowercase().contains("waiting") { "badge badge--accent".to_owned() }
                                                                    else { "badge badge--muted".to_owned() };
                                                                let total_fmt = format_price(total);
                                                                view! {
                                                                    <tr style="border-bottom:1px solid var(--clr-border)">
                                                                        <td style="padding:.625rem;font-family:var(--font-mono)">{code}</td>
                                                                        <td style="padding:.625rem"><span class=status_cls>{status_s}</span></td>
                                                                        <td style="padding:.625rem;text-align:right;color:var(--clr-accent)">{total_fmt}</td>
                                                                        <td style="padding:.625rem;color:var(--clr-muted)">{created}</td>
                                                                    </tr>
                                                                }
                                                            }).collect_view()}
                                                        </tbody>
                                                    </table>
                                                    {if rows_empty {
                                                        view! { <div class="empty"><div class="empty__icon">"📋"</div><div class="empty__title">"Tidak ada order"</div></div> }.into_any()
                                                    } else {
                                                        view! { <span/> }.into_any()
                                                    }}
                                                </div>
                                            }.into_any()
                                        }
                                        Err(_) => view! { <div class="alert alert--error">"Gagal memuat order."</div> }.into_any(),
                                    }).unwrap_or_else(|| view!{<div/>}.into_any())}
                                </Suspense>
                            })}

                            // ── Events Tab ────────────────────────────────────
                            {move || (tab.get() == "events").then(|| view! {
                                <div style="text-align:center;padding:2rem;color:var(--clr-muted)">
                                    "Manajemen event tersedia di halaman Merchant Hub."<br/>
                                    <A href="/merchant" attr:class="btn btn--ghost btn--sm" attr:style="margin-top:.75rem">"Ke Merchant Hub →"</A>
                                </div>
                            })}

                            // ── Pagination ────────────────────────────────────
                            <div style="display:flex;justify-content:center;gap:.75rem;padding:1.5rem 0">
                                <button
                                    class="btn btn--ghost btn--sm"
                                    on:click=move |_| { if page.get() > 1 { page.update(|p| *p -= 1); } }
                                    disabled=move || page.get() <= 1
                                >"← Prev"</button>
                                <span style="display:flex;align-items:center;color:var(--clr-muted);font-size:.875rem">
                                    "Hal " {move || page.get()}
                                </span>
                                <button
                                    class="btn btn--ghost btn--sm"
                                    on:click=move |_| page.update(|p| *p += 1)
                                >"Next →"</button>
                            </div>
                        </div>
                    }.into_any()
                }}
            </Suspense>
        </div>
    }
}
