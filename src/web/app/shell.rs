//! web/app/shell.rs — Shell HTML SSR (server-only).
//!
//! `shell()` dipanggil Axum untuk setiap SSR request; menghasilkan HTML penuh
//! lalu me-mount `<App/>` yang sama untuk hydration (zero DOM mismatch → no FOUC).

use leptos::prelude::*;
use leptos_meta::*;

use super::router::App;

/// Shell HTML — dipanggil Axum untuk setiap SSR request.
///
/// Fix FOUC yang diterapkan:
/// 1. `data-theme="dark"` pada `<html>` sebagai SSR default.
/// 2. Inline blocking `<script>` — baca localStorage sebelum CSS di-parse,
///    override data-theme ke "light" jika user memilihnya.
/// 3. Inline ALL CSS langsung ke `<head>` via include_str! — tidak ada
///    HTTP round-trip untuk CSS, tidak pernah 404, tidak perlu assets router.
///    Browser tetap bisa cache via ETag pada full-page response.
pub fn shell(options: leptos::config::LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="id" data-theme="dark">
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                <meta name="theme-color" content="#050814" />

                // ── Fix FOUC #1: Inline theme script ────────────────────────
                // Synchronous/blocking — eksekusi sebelum CSS apapun di-parse.
                <script inner_html=r#"(function(){try{var t=localStorage.getItem('kinetic.theme');if(t==='light'||t==='dark'){document.documentElement.setAttribute('data-theme',t);}}catch(e){}})();"# />

                // ── CSS: single cached external file ────────────────────────
                // Render-blocking <link rel="stylesheet"> in <head> keeps the
                // same zero-FOUC guarantee as inline CSS (browser pauses paint
                // until the file arrives). Benefit over inline: after the first
                // visit the browser caches the file for 24 h — every subsequent
                // load costs 0 CSS bytes, making the HTML response ~120 KB
                // smaller. The preload hint starts the fetch during HTML parsing,
                // before the stylesheet link is reached, minimising block time.
                <link rel="preload" href="/styles/app.css" attr:as="style" />
                <link rel="stylesheet" href="/styles/app.css" />

                // ── Fonts ────────────────────────────────────────────────────
                <link rel="preconnect" href="https://fonts.googleapis.com" />
                <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin="" />
                <link
                    href="https://fonts.googleapis.com/css2?family=Bebas+Neue&family=Space+Mono:ital,wght@0,400;0,700;1,400&display=swap"
                    rel="stylesheet"
                />

                // ── Leaflet (OpenStreetMap) untuk peta lokasi event ──────────
                <link rel="stylesheet" href="https://unpkg.com/leaflet@1.9.4/dist/leaflet.css" />
                <script src="https://unpkg.com/leaflet@1.9.4/dist/leaflet.js"></script>
                <script inner_html=r#"
                (function(){
                  window.__pulseMaps = window.__pulseMaps || {};
                  function tile(m){
                    L.tileLayer('https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png',
                      {maxZoom:19, attribution:'&copy; OpenStreetMap'}).addTo(m);
                  }
                  window.pulseMapDestroy = function(id){
                    var m = window.__pulseMaps[id];
                    if(m){ try{ m.remove(); }catch(e){} delete window.__pulseMaps[id]; }
                  };
                  window.pulseMapPicker = function(mapId, latId, lngId){
                    if(!window.L || !document.getElementById(mapId)){
                      return setTimeout(function(){ window.pulseMapPicker(mapId, latId, lngId); }, 200);
                    }
                    var latEl = document.getElementById(latId), lngEl = document.getElementById(lngId);
                    var lat = parseFloat(latEl && latEl.value), lng = parseFloat(lngEl && lngEl.value);
                    if(isNaN(lat)) lat = -6.2088;
                    if(isNaN(lng)) lng = 106.8456;
                    window.pulseMapDestroy(mapId);
                    var m = L.map(mapId).setView([lat,lng], 13); tile(m);
                    var mk = L.marker([lat,lng], {draggable:true}).addTo(m);
                    function emit(p){
                      if(latEl){ latEl.value = p.lat.toFixed(6); latEl.dispatchEvent(new Event('input',{bubbles:true})); }
                      if(lngEl){ lngEl.value = p.lng.toFixed(6); lngEl.dispatchEvent(new Event('input',{bubbles:true})); }
                    }
                    m.on('click', function(e){ mk.setLatLng(e.latlng); emit(e.latlng); });
                    mk.on('dragend', function(){ emit(mk.getLatLng()); });
                    m.__mk = mk; window.__pulseMaps[mapId] = m;
                    setTimeout(function(){ m.invalidateSize(); }, 150);
                  };
                  window.pulseMapSet = function(mapId, lat, lng){
                    var m = window.__pulseMaps[mapId];
                    if(!m || isNaN(lat) || isNaN(lng)) return;
                    m.setView([lat,lng]); if(m.__mk) m.__mk.setLatLng([lat,lng]);
                  };
                  window.pulseMapViewer = function(mapId, lat, lng, label){
                    if(!window.L || !document.getElementById(mapId)){
                      return setTimeout(function(){ window.pulseMapViewer(mapId, lat, lng, label); }, 200);
                    }
                    if(isNaN(lat) || isNaN(lng)) return;
                    window.pulseMapDestroy(mapId);
                    var m = L.map(mapId, {scrollWheelZoom:false}).setView([lat,lng], 15); tile(m);
                    L.marker([lat,lng]).addTo(m).bindPopup(label || 'Lokasi').openPopup();
                    window.__pulseMaps[mapId] = m;
                    setTimeout(function(){ m.invalidateSize(); }, 150);
                  };
                })();
                "# />

                // ── WASM + JS preloads ───────────────────────────────────────
                // Start downloading the WASM binary and its JS loader during
                // HTML head parsing — before HydrationScripts script tags tell
                // the browser to fetch them. Overlaps WASM download with CSS
                // render + remaining HTML parse, cutting time-to-interactive
                // on first visit. crossorigin="" is required so the preloaded
                // response is reusable by the fetch() inside the loader.
                <link
                    rel="preload"
                    href="/pkg/e-ticketing_bg.wasm"
                    attr:as="fetch"
                    attr:type="application/wasm"
                    crossorigin=""
                />
                <link rel="modulepreload" href="/pkg/e-ticketing.js" />

                // ── Leptos infrastructure ────────────────────────────────────
                <AutoReload options=options.clone() />
                <HydrationScripts options=options.clone() />
                <MetaTags />

                // ── Hydration loading indicator ──────────────────────────────
                // Hilang secara otomatis setelah WASM hydration selesai karena
                // Leptos menggantikan/update DOM. Script inline ini jauh lebih
                // cepat dari polling JS — tidak ada delay tambahan.
                <style inner_html=r#"
                #hydration-loader{
                position:fixed;top:0;left:0;right:0;height:2px;
                background:linear-gradient(90deg,#c8ff5e,#4f6bff);
                z-index:9999;animation:hloader 1.4s ease-in-out infinite;
                transform-origin:left;
                }
                @keyframes hloader{
                0%{transform:scaleX(0) translateX(0)}
                50%{transform:scaleX(0.7) translateX(40%)}
                100%{transform:scaleX(0) translateX(100%)}
                }
                "# />
                <script inner_html=r#"
                (function(){
                var bar = document.createElement('div');
                bar.id = 'hydration-loader';
                document.head.appendChild(bar);
                // Leptos fires 'leptos:hydrated' atau kita poll sampai klik jalan
                var rm = function(){ var b=document.getElementById('hydration-loader'); if(b) b.remove(); };
                document.addEventListener('leptos:hydrated', rm, {once:true});
                // Fallback: hapus setelah 8 detik walau event tidak fire
                setTimeout(rm, 8000);
                })();
                "# />
            </head>
            <body>
                <App />
            </body>
        </html>
    }
}
