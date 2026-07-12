//! web/app/shell.rs — Shell HTML SSR (server-only).
//!
//! `shell()` dipanggil Axum untuk setiap SSR request; menghasilkan HTML penuh
//! lalu me-mount `<App/>` yang sama untuk hydration (zero DOM mismatch → no FOUC).

use leptos::prelude::*;
use leptos::nonce::{provide_nonce, use_nonce};
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
    // CSP nonce: generate & provide via context. HydrationScripts/AutoReload &
    // leptos_meta otomatis memakainya; tag inline manual di bawah menambah
    // `nonce=use_nonce()`. CSP dikirim lewat <Meta http-equiv> — OPT-IN via env
    // `ENABLE_CSP=1` (default OFF) agar bisa dites tanpa risiko blank page.
    provide_nonce();
    let csp_on = std::env::var("ENABLE_CSP").ok().as_deref() == Some("1");
    view! {
        <!DOCTYPE html>
        <html lang="id" data-theme="dark">
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                <meta name="theme-color" content="#050814" />

                // ── Content-Security-Policy (opt-in, nonce-based) ────────────
                {csp_on.then(|| view! {
                    <Meta
                        http_equiv="Content-Security-Policy"
                        content=move || use_nonce().map(|n| format!(
                            "default-src 'self'; \
                             script-src 'strict-dynamic' 'nonce-{n}' 'wasm-unsafe-eval'; \
                             style-src 'self' 'nonce-{n}' https://fonts.googleapis.com; \
                             font-src 'self' https://fonts.gstatic.com; \
                             img-src * data: blob:; \
                             connect-src 'self' ws: wss:; \
                             base-uri 'self'; form-action 'self'; object-src 'none'"
                        )).unwrap_or_default()
                    />
                })}

                // ── Fix FOUC #1: Inline theme script ────────────────────────
                // Synchronous/blocking — eksekusi sebelum CSS apapun di-parse.
                <script nonce=use_nonce() inner_html=r#"(function(){try{var t=localStorage.getItem('kinetic.theme');var th=(t==='light'||t==='dark')?t:'dark';document.documentElement.setAttribute('data-theme',th);window.__setThemeColor=function(x){var m=document.querySelector('meta[name=theme-color]');if(m)m.setAttribute('content',x==='light'?'#ffffff':'#050814');};window.__setThemeColor(th);}catch(e){}})();"# />

                // ── Fix FOUC #1b: Critical CSS inline ────────────────────────
                // Cukup untuk cat pertama SEBELUM app.css turun: token latar +
                // skeleton shimmer. Tanpa ini, karena app.css di-load non-blocking
                // (di bawah), cat pertama pada kunjungan dingin/jaringan lambat
                // bisa PUTIH KOSONG (blank) — bukan shimmer, karena .shimmer-bg
                // sendiri ada di dalam app.css. Nilai token WAJIB identik dgn
                // 00-tokens.css agar tak ada pergeseran warna saat app.css apply.
                <style nonce=use_nonce() inner_html=r#"
                :root{--bg-page:#0d0d1a;--bg-elevated:#1e1e30;--bg-card-hover:#1a1a2e;--border-soft:rgba(255,255,255,.06);--text-primary:#f0f0f8;--font-body:"Space Mono",monospace}
                [data-theme="light"]{--bg-page:#f3f3fb;--bg-elevated:#e2e2f0;--bg-card-hover:#ebebf5;--border-soft:rgba(0,0,20,.06);--text-primary:#0c0c1a}
                *{box-sizing:border-box}
                html,body{margin:0;padding:0;background:var(--bg-page);color:var(--text-primary);font-family:var(--font-body),monospace;min-height:100vh}
                @keyframes shimmer{0%{background-position:-200% 0}100%{background-position:200% 0}}
                .shimmer-bg{background:linear-gradient(90deg,var(--bg-elevated) 25%,var(--bg-card-hover) 50%,var(--bg-elevated) 75%);background-size:200% 100%;animation:shimmer 1.6s ease-in-out infinite;border-radius:8px}
                "# />

                // ── CSS: single cached external file, NON-render-blocking ────
                // app.css di-load async (media=print → flip ke all saat onload)
                // supaya cat pertama TIDAK menunggu CSS turun (penyebab blank di
                // jaringan lambat). Skeleton di atas sudah menjamin no-FOUC untuk
                // above-the-fold; app.css menyusul (biasanya puluhan ms) untuk
                // seluruh gaya. Preload menjaga fetch prioritas tinggi; <noscript>
                // fallback untuk klien tanpa JS. Cache 24 h tetap berlaku.
                <link rel="preload" href="/styles/app.css" attr:as="style" />
                <link rel="stylesheet" href="/styles/app.css" attr:media="print" id="pulse-appcss" />
                <noscript>
                    <link rel="stylesheet" href="/styles/app.css" />
                </noscript>
                <script nonce=use_nonce() inner_html=r#"(function(){var l=document.getElementById('pulse-appcss');if(!l)return;var s=function(){l.media='all';};if(l.sheet){s();}else{l.addEventListener('load',s);setTimeout(s,4000);}})();"# />

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
                <script src="/vendor/leaflet.js" nonce=use_nonce()></script>
                // Style kustom: pin SVG (tanpa gambar eksternal) + tombol locate.
                <style nonce=use_nonce() inner_html=r#"
                .leaflet-div-icon.pulse-pin{background:transparent;border:none;}
                .pulse-pin svg{display:block;filter:drop-shadow(0 3px 4px rgba(0,0,0,.35));}
                .pulse-locate{font-size:17px;font-weight:700;line-height:30px;text-align:center;}
                .leaflet-container{font-family:var(--font-body,sans-serif);}
                "# />
                <script nonce=use_nonce() inner_html=r##"
                (function(){
                  window.__pulseMaps = window.__pulseMaps || {};
                  // Tiles OpenStreetMap resmi (paling andal & tak butuh API key).
                  function tile(m){
                    L.tileLayer('https://tile.openstreetmap.org/{z}/{x}/{y}.png',
                      {maxZoom:19, attribution:'&copy; OpenStreetMap contributors'}).addTo(m);
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
                  // Tunggu Leaflet (L) + elemen peta siap (maks ~8 dtk) lalu jalankan cb.
                  // onfail dipanggil bila Leaflet tak kunjung termuat (mis. /vendor &
                  // CDN dua-duanya gagal) supaya bisa menampilkan pesan ke user.
                  function waitFor(mapId, cb, tries, onfail){
                    if(window.L && document.getElementById(mapId)) return cb();
                    if(!window.L) loadLeaflet();
                    if(tries <= 0){ if(onfail) onfail(); return; }
                    setTimeout(function(){ waitFor(mapId, cb, tries-1, onfail); }, 100);
                  }
                  function mapBoxMsg(mapId, html){
                    var el = document.getElementById(mapId);
                    if(el && !el._leaflet_id) el.innerHTML = html;
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
                    var box0 = document.getElementById(mapId);
                    if(box0 && box0._leaflet_id) return; // sudah ada peta di elemen ini
                    waitFor(mapId, function(){
                      var box = document.getElementById(mapId);
                      if(box) box.innerHTML = ''; // hapus placeholder "Memuat peta…"
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
                    }, 80, function(){
                      mapBoxMsg(mapId, '<div style="display:flex;align-items:center;justify-content:center;height:100%;padding:14px;text-align:center;color:#c0392b;font-size:13px;line-height:1.5">Peta gagal dimuat.<br>Cek Console / Network: apakah <b>/vendor/leaflet.js</b> ter-load (200)?</div>');
                    });
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
                  // VIEWER (peta read-only di halaman publik) → init langsung.
                  function initViewers(){
                    var vs = document.querySelectorAll('[data-map-viewer]:not([data-map-init])');
                    for(var j=0;j<vs.length;j++){ (function(el){
                      el.setAttribute('data-map-init','1');
                      window.pulseMapViewer(el.id, parseFloat(el.getAttribute('data-lat')),
                        parseFloat(el.getAttribute('data-lng')), el.getAttribute('data-label')||'Lokasi');
                    })(vs[j]); }
                  }
                  // PICKER (create/update event) — JANGAN di-init saat DOMContentLoaded:
                  // kalau Leaflet mengisi div SEBELUM hydration WASM, Leptos menganggap
                  // div itu "harusnya kosong" → mismatch → peta terhapus / hydration gagal.
                  // Jadi picker di-init oleh Leptos Effect (post-hydration). Fungsi ini
                  // hanya FALLBACK bila Effect gagal (mis. WASM tak load): jalan setelah
                  // jeda, dan skip bila peta sudah dibuat Effect (_leaflet_id ada).
                  function initPickersFallback(){
                    var ps = document.querySelectorAll('[data-map-picker]:not([data-map-init])');
                    for(var i=0;i<ps.length;i++){ (function(el){
                      if(el._leaflet_id){ el.setAttribute('data-map-init','1'); return; }
                      el.setAttribute('data-map-init','1');
                      window.pulseMapPicker(el.id, el.getAttribute('data-lat-input'), el.getAttribute('data-lng-input'));
                    })(ps[i]); }
                  }
                  function autoInitMaps(){ initViewers(); }
                  // Fallback picker: jalan HANYA bila WASM gagal load (mismatch
                  // hydration terhindar). Bila `window.__pulseHydrated` ter-set
                  // (WASM jalan — lihat lib.rs hydrate()), Effect Leptos yang
                  // meng-init picker, jadi fallback tak perlu bertindak.
                  function schedulePickers(){
                    setTimeout(function(){
                      if(window.__pulseHydrated) return;   // WASM ada → biar Effect
                      initPickersFallback();
                    }, 1500);
                  }
                  function needsPicker(){ return !!document.querySelector('[data-map-picker]:not([data-map-init])'); }
                  if(document.readyState!=='loading'){ autoInitMaps(); if(needsPicker()) schedulePickers(); }
                  else { document.addEventListener('DOMContentLoaded', function(){ autoInitMaps(); if(needsPicker()) schedulePickers(); }); }
                  try{
                    var __mo=new MutationObserver(function(){
                      if(window.__mapT) return;
                      window.__mapT=setTimeout(function(){
                        window.__mapT=null;
                        autoInitMaps();
                        // Hanya jadwalkan fallback bila memang ada picker belum ter-init
                        // → hindari akumulasi setTimeout tiap mutasi DOM (SPA).
                        if(needsPicker()) schedulePickers();
                      }, 120);
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
                // NB: nama file hasil cargo-leptos adalah e-ticketing.wasm
                // (TANPA sufiks _bg — itu nama internal wasm-bindgen). Preload
                // ke _bg.wasm = 404 diam-diam, optimisasi tidak pernah jalan.
                <link
                    rel="preload"
                    href="/pkg/e-ticketing.wasm"
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
                <style nonce=use_nonce() inner_html=r#"
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
                <script nonce=use_nonce() inner_html=r#"
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
