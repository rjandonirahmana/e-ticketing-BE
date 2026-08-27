use leptos::prelude::*;
use leptos_router::components::A;

use crate::web::hooks::ThemeToggle;

#[component]
pub fn NotFoundPage() -> impl IntoView {
    view! {
        <div class="page nf-page">
            <div class="nf-grid-bg">
                <div class="nf-grid-line nf-grid-h" style="top:20%"></div>
                <div class="nf-grid-line nf-grid-h" style="top:40%"></div>
                <div class="nf-grid-line nf-grid-h" style="top:60%"></div>
                <div class="nf-grid-line nf-grid-h" style="top:80%"></div>
                <div class="nf-grid-line nf-grid-v" style="left:25%"></div>
                <div class="nf-grid-line nf-grid-v" style="left:50%"></div>
                <div class="nf-grid-line nf-grid-v" style="left:75%"></div>
            </div>
            <div class="nf-orb nf-orb-1"></div>
            <div class="nf-orb nf-orb-2"></div>

            <header class="page-header" style="position:relative;z-index:10;">
                <A href="/" attr:class="back-btn">
                    <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                        <polyline points="15 18 9 12 15 6"/>
                    </svg>
                </A>
                <span class="page-logo">"KINETIC"</span>
                <ThemeToggle/>
            </header>

            <div class="nf-content">
                <div class="nf-number-wrap">
                    <div class="nf-number">"404"</div>
                    <div class="nf-number nf-number-glitch" aria-hidden="true">"404"</div>
                </div>
                <div class="nf-status-tag">
                    <span class="nf-status-dot"></span>
                    "SIGNAL LOST"
                </div>
                <h1 class="nf-title">"STAGE NOT FOUND"</h1>
                <p class="nf-desc">
                    "This page doesn't exist or has been taken offline. "
                    "The show must go on — head back to the main stage."
                </p>
                <div class="nf-actions">
                    <A href="/" attr:class="nf-btn-primary">
                        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                            <path d="M3 9l9-7 9 7v11a2 2 0 01-2 2H5a2 2 0 01-2-2z"/>
                        </svg>
                        "BACK TO STAGE"
                    </A>
                    <A href="/explore" attr:class="nf-btn-secondary">"JELAJAHI PRODUK"</A>
                </div>
                <div class="nf-suggestions">
                    <span class="nf-suggestions-label">"QUICK LINKS"</span>
                    <div class="nf-suggestion-links">
                        <A href="/tickets" attr:class="nf-suggestion-link">
                            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                <path d="M2 9a3 3 0 110 6v2a2 2 0 002 2h16a2 2 0 002-2v-2a3 3 0 110-6V7a2 2 0 00-2-2H4a2 2 0 00-2 2v2z"/>
                            </svg>
                            "Pengambilan Saya"
                        </A>
                        <A href="/cart" attr:class="nf-suggestion-link">
                            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                <circle cx="9" cy="21" r="1"/><circle cx="20" cy="21" r="1"/>
                                <path d="M1 1h4l2.68 13.39a2 2 0 002 1.61h9.72a2 2 0 002-1.61L23 6H6"/>
                            </svg>
                            "Cart"
                        </A>
                        <A href="/profile" attr:class="nf-suggestion-link">
                            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                                <path d="M20 21v-2a4 4 0 00-4-4H8a4 4 0 00-4 4v2"/>
                                <circle cx="12" cy="7" r="4"/>
                            </svg>
                            "Profile"
                        </A>
                    </div>
                </div>
            </div>

            <style>
                "
                .nf-page { min-height:100vh; position:relative; overflow:hidden; display:flex; flex-direction:column; }
                .nf-grid-bg { position:absolute; inset:0; pointer-products:none; z-index:0; }
                .nf-grid-line { position:absolute; background:rgba(200,255,94,0.04); }
                .nf-grid-h { left:0; right:0; height:1px; }
                .nf-grid-v { top:0; bottom:0; width:1px; }
                .nf-orb { position:absolute; border-radius:50%; pointer-products:none; filter:blur(80px); }
                .nf-orb-1 { width:300px; height:300px; background:rgba(200,255,94,0.07); top:-80px; right:-80px; z-index:0; }
                .nf-orb-2 { width:250px; height:250px; background:rgba(100,80,255,0.08); bottom:80px; left:-60px; z-index:0; }
                .nf-content { position:relative; z-index:10; flex:1; display:flex; flex-direction:column; align-items:center; justify-content:center; padding:40px 24px 60px; text-align:center; }
                .nf-number-wrap { position:relative; margin-bottom:16px; }
                .nf-number { font-family:'Bebas Neue',sans-serif; font-size:128px; line-height:1; color:#c8ff5e; letter-spacing:-4px; text-shadow:0 0 40px rgba(200,255,94,0.4),0 0 80px rgba(200,255,94,0.15); }
                .nf-number-glitch { position:absolute; top:0; left:0; right:0; color:rgba(200,255,94,0.15); animation:nf-glitch 3s infinite; clip-path:polygon(0 30%,100% 30%,100% 50%,0 50%); }
                @keyframes nf-glitch { 0%,90%,100%{transform:translate(0);opacity:0} 92%{transform:translate(-4px,2px);opacity:1} 94%{transform:translate(4px,-2px);opacity:0.8} 96%{transform:translate(-2px,1px);opacity:1} 98%{transform:translate(0);opacity:0} }
                .nf-status-tag { display:inline-flex; align-items:center; gap:6px; background:rgba(200,255,94,0.08); border:1px solid rgba(200,255,94,0.2); border-radius:100px; padding:4px 14px; font-size:11px; font-weight:600; letter-spacing:2px; color:#c8ff5e; margin-bottom:20px; }
                .nf-status-dot { width:6px; height:6px; background:#c8ff5e; border-radius:50%; animation:nf-blink 1.2s infinite; }
                @keyframes nf-blink { 0%,100%{opacity:1} 50%{opacity:0.2} }
                .nf-title { font-family:'Bebas Neue',sans-serif; font-size:32px; letter-spacing:3px; color:#fff; margin:0 0 12px; }
                .nf-desc { color:#8888aa; font-size:14px; line-height:1.7; max-width:280px; margin:0 auto 32px; }
                .nf-actions { display:flex; flex-direction:column; gap:12px; width:100%; max-width:280px; margin-bottom:40px; }
                .nf-btn-primary { display:flex; align-items:center; justify-content:center; gap:8px; background:#c8ff5e; color:#0d0d1a; border-radius:14px; padding:16px 24px; font-weight:700; font-size:13px; letter-spacing:1.5px; text-decoration:none; }
                .nf-btn-secondary { display:flex; align-items:center; justify-content:center; background:rgba(255,255,255,0.05); border:1px solid rgba(255,255,255,0.1); color:#aaaacc; border-radius:14px; padding:15px 24px; font-weight:600; font-size:13px; letter-spacing:1.5px; text-decoration:none; }
                .nf-suggestions { display:flex; flex-direction:column; align-items:center; gap:12px; }
                .nf-suggestions-label { font-size:10px; letter-spacing:2px; color:#444466; font-weight:600; }
                .nf-suggestion-links { display:flex; gap:8px; flex-wrap:wrap; justify-content:center; }
                .nf-suggestion-link { display:flex; align-items:center; gap:6px; background:rgba(255,255,255,0.04); border:1px solid rgba(255,255,255,0.07); border-radius:100px; padding:8px 14px; font-size:12px; color:#6666aa; text-decoration:none; }
                "
            </style>
        </div>
    }
}
