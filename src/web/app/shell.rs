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
                // Leaflet di-self-host dari binary (bukan CDN unpkg) agar peta
                // selalu ter-load — lihat web/assets.rs (serve_leaflet_js/css).
                <link rel="stylesheet" href="/vendor/leaflet.css" />
                <script src="/vendor/leaflet.js"></script>
                // Style kustom: pin SVG (tanpa gambar eksternal) + tombol locate.
                <style inner_html=r#"
                .leaflet-div-icon.pulse-pin{background:transparent;border:none;}
                .pulse-pin svg{display:block;filter:drop-shadow(0 3px 4px rgba(0,0,0,.35));}
                .pulse-locate{font-size:17px;font-weight:700;line-height:30px;text-align:center;}
                .leaflet-container{font-family:var(--font-body,sans-serif);}
                "# />
                <script inner_html=r##"
                (function(){
                  window.__pulseMaps = window.__pulseMaps || {};
                  // Tiles CartoDB Voyager (data OpenStreetMap, tampilan lebih bersih).
                  function tile(m){
                    L.tileLayer('https://{s}.basemaps.cartocdn.com/rastertiles/voyager/{z}/{x}/{y}{r}.png',
                      {maxZoom:20, subdomains:'abcd', attribution:'&copy; OpenStreetMap, &copy; CARTO'}).addTo(m);
                  }
                  // Pin SVG kustom — tidak memuat gambar marker default Leaflet
                  // (yang sering 404 di setup SPA → marker tak terlihat).
                  function pin(){
                    return L.divIcon({
                      className:'pulse-pin',
                      html:'<svg width="36" height="46" viewBox="0 0 36 46" xmlns="http://www.w3.org/2000/svg"><path d="M18 0C8 0 0 8 0 18c0 13 18 28 18 28s18-15 18-28C36 8 28 0 18 0z" fill="#4f6bff"/><circle cx="18" cy="18" r="7" fill="#ffffff"/></svg>',
                      iconSize:[36,46], iconAnchor:[18,44], popupAnchor:[0,-40]
                    });
                  }
                  // Muat Leaflet on-demand bila <script> di <head> gagal/terblok.
                  // Coba self-host dulu (/vendor), fallback ke CDN unpkg.
                  function loadLeaflet(){
                    if(window.L || window.__leafletLoading) return;
                    window.__leafletLoading = true;
                    if(!document.querySelector('link[data-leaflet]')){
                      var lk=document.createElement('link'); lk.rel='stylesheet';
                      lk.href='/vendor/leaflet.css'; lk.setAttribute('data-leaflet','1');
                      document.head.appendChild(lk);
                    }
                    var s=document.createElement('script'); s.setAttribute('data-leaflet','1');
                    s.src='/vendor/leaflet.js';
                    s.onerror=function(){
                      var s2=document.createElement('script');
                      s2.src='https://unpkg.com/leaflet@1.9.4/dist/leaflet.js';
                      document.head.appendChild(s2);
                    };
                    document.head.appendChild(s);
                  }
                  // Tunggu Leaflet (L) + elemen peta siap (maks ~6 dtk) lalu jalankan cb.
                  function waitFor(mapId, cb, tries){
                    if(window.L && document.getElementById(mapId)) return cb();
                    if(!window.L) loadLeaflet();
                    if(tries <= 0) return;
                    setTimeout(function(){ waitFor(mapId, cb, tries-1); }, 100);
                  }
                  // Penyebab umum "peta abu-abu": peta di-init sebelum container
                  // punya ukuran final (umum di SPA: layout/font/animasi belum
                  // settle). invalidateSize() memaksa Leaflet menghitung ulang
                  // ukuran & memuat ulang tile. Dipanggil bertubi: rAF, beberapa
                  // timer, whenReady, plus ResizeObserver agar setiap perubahan
                  // ukuran container otomatis memicu recalculation.
                  function refresh(m){
                    var inv = function(){ try{ m.invalidateSize(false); }catch(e){} };
                    [0,80,250,500,1000,2000].forEach(function(t){ setTimeout(inv,t); });
                    if(window.requestAnimationFrame) requestAnimationFrame(inv);
                    try{ m.whenReady(inv); }catch(e){}
                    var el = (function(){ try{ return m.getContainer(); }catch(e){ return null; } })();
                    if(window.ResizeObserver && el){
                      try{ var ro = new ResizeObserver(function(){ inv(); }); ro.observe(el); m.__ro = ro; }catch(e){}
                    }
                    window.addEventListener('resize', inv);
                    m.__inv = inv;
                  }
                  window.pulseMapDestroy = function(id){
                    var m = window.__pulseMaps[id];
                    if(m){
                      try{ if(m.__ro) m.__ro.disconnect(); }catch(e){}
                      try{ if(m.__inv) window.removeEventListener('resize', m.__inv); }catch(e){}
                      try{ m.remove(); }catch(e){}
                      delete window.__pulseMaps[id];
                    }
                  };
                  window.pulseMapPicker = function(mapId, latId, lngId, lat0, lng0){
                    waitFor(mapId, function(){
                      var latEl = document.getElementById(latId), lngEl = document.getElementById(lngId);
                      // Prioritas: koordinat eksplisit dari Rust → value input → default Jakarta.
                      var lat = parseFloat(lat0), lng = parseFloat(lng0);
                      if(isNaN(lat)) lat = parseFloat(latEl && latEl.value);
                      if(isNaN(lng)) lng = parseFloat(lngEl && lngEl.value);
                      if(isNaN(lat)) lat = -6.2088;
                      if(isNaN(lng)) lng = 106.8456;
                      // Sinkronkan value input awal supaya konsisten dgn pin.
                      if(latEl) latEl.value = lat.toFixed(6);
                      if(lngEl) lngEl.value = lng.toFixed(6);
                      window.pulseMapDestroy(mapId);
                      var m = L.map(mapId).setView([lat,lng], 15); tile(m);
                      var mk = L.marker([lat,lng], {draggable:true, icon:pin()}).addTo(m);
                      function emit(p){
                        if(latEl){ latEl.value = p.lat.toFixed(6); latEl.dispatchEvent(new Event('input',{bubbles:true})); }
                        if(lngEl){ lngEl.value = p.lng.toFixed(6); lngEl.dispatchEvent(new Event('input',{bubbles:true})); }
                      }
                      m.on('click', function(e){ mk.setLatLng(e.latlng); emit(e.latlng); });
                      mk.on('dragend', function(){ emit(mk.getLatLng()); });
                      // Kontrol "Lokasi saya" (geolokasi browser).
                      var Locate = L.Control.extend({
                        options:{position:'topleft'},
                        onAdd:function(){
                          var c = L.DomUtil.create('div','leaflet-bar');
                          var b = L.DomUtil.create('a','pulse-locate', c);
                          b.href='#'; b.title='Lokasi saya'; b.setAttribute('role','button');
                          b.innerHTML='&#9678;';
                          L.DomEvent.on(b,'click',function(ev){
                            L.DomEvent.preventDefault(ev); L.DomEvent.stopPropagation(ev);
                            if(!navigator.geolocation) return;
                            navigator.geolocation.getCurrentPosition(function(pos){
                              var ll={lat:pos.coords.latitude,lng:pos.coords.longitude};
                              m.setView([ll.lat,ll.lng],16); mk.setLatLng([ll.lat,ll.lng]); emit(ll);
                            });
                          });
                          return c;
                        }
                      });
                      m.addControl(new Locate());
                      m.__mk = mk; window.__pulseMaps[mapId] = m;
                      refresh(m);
                    }, 60);
                  };
                  window.pulseMapSet = function(mapId, lat, lng){
                    var m = window.__pulseMaps[mapId];
                    if(!m || isNaN(lat) || isNaN(lng)) return;
                    m.setView([lat,lng]); if(m.__mk) m.__mk.setLatLng([lat,lng]);
                  };
                  window.pulseMapViewer = function(mapId, lat, lng, label){
                    waitFor(mapId, function(){
                      if(isNaN(lat) || isNaN(lng)) return;
                      window.pulseMapDestroy(mapId);
                      var m = L.map(mapId, {scrollWheelZoom:false}).setView([lat,lng], 16); tile(m);
                      L.marker([lat,lng], {icon:pin()}).addTo(m).bindPopup(label || 'Lokasi').openPopup();
                      window.__pulseMaps[mapId] = m;
                      refresh(m);
                    }, 60);
                  };

                  // ── Auto-init dari DOM (TIDAK bergantung hydration WASM) ──────
                  // Peta murni JS; cukup tandai container dengan data-attribute lalu
                  // skrip ini yang meng-init-nya. Jalan di SSR-only maupun hydrate,
                  // dan saat navigasi SPA (lewat MutationObserver). Guard
                  // `data-map-init` mencegah init ganda.
                  function autoInitMaps(){
                    var ps = document.querySelectorAll('[data-map-picker]:not([data-map-init])');
                    for(var i=0;i<ps.length;i++){ (function(el){
                      el.setAttribute('data-map-init','1');
                      window.pulseMapPicker(el.id, el.getAttribute('data-lat-input'), el.getAttribute('data-lng-input'));
                    })(ps[i]); }
                    var vs = document.querySelectorAll('[data-map-viewer]:not([data-map-init])');
                    for(var j=0;j<vs.length;j++){ (function(el){
                      el.setAttribute('data-map-init','1');
                      window.pulseMapViewer(el.id, parseFloat(el.getAttribute('data-lat')),
                        parseFloat(el.getAttribute('data-lng')), el.getAttribute('data-label')||'Lokasi');
                    })(vs[j]); }
                  }
                  if(document.readyState!=='loading'){ autoInitMaps(); }
                  else { document.addEventListener('DOMContentLoaded', autoInitMaps); }
                  try{
                    var __mo=new MutationObserver(function(){
                      if(window.__mapT) return;
                      window.__mapT=setTimeout(function(){ window.__mapT=null; autoInitMaps(); }, 100);
                    });
                    __mo.observe(document.documentElement, {childList:true, subtree:true});
                  }catch(e){}
                })();
                "## />

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
