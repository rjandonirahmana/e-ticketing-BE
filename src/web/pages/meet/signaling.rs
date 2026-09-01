//! meet/signaling.rs — Loop pesan WebSocket: tangani admit/roster/signal/state,
//! plus lifecycle koneksi (preview, connect, teardown).

use leptos::prelude::*;
use serde_json::json;
use wasm_bindgen::prelude::*;

use super::tiles::{remove_tile, set_tile_state};
use super::webrtc::{handle_answer, handle_candidate, handle_offer, make_offer};
use super::{build_ws_url, Ctx, Phase};

/// Tangani satu pesan JSON dari server.
pub(super) async fn handle_msg(ctx: &Ctx, raw: &str) {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) else {
        return;
    };
    let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
    match ty {
        "joined" | "admitted" => {
            if let Some(id) = v.get("self_id").and_then(|s| s.as_str()) {
                ctx.self_id.set(id.to_string());
            }
            if let Some(peers) = v.get("peers").and_then(|p| p.as_array()) {
                for p in peers {
                    if let (Some(id), Some(name)) = (
                        p.get("id").and_then(|x| x.as_str()),
                        p.get("name").and_then(|x| x.as_str()),
                    ) {
                        ctx.names.borrow_mut().insert(id.to_string(), name.to_string());
                    }
                }
            }
            if let Some(pending) = v.get("pending").and_then(|p| p.as_array()) {
                let list: Vec<(String, String)> = pending
                    .iter()
                    .filter_map(|p| {
                        Some((
                            p.get("id")?.as_str()?.to_string(),
                            p.get("name")?.as_str()?.to_string(),
                        ))
                    })
                    .collect();
                ctx.pending.set(list);
            }
            ctx.phase.set(Phase::InMeet);
            // Peer yang baru bergabung initiate offer ke tiap peer yang sudah ada.
            if let Some(peers) = v.get("peers").and_then(|p| p.as_array()) {
                for p in peers {
                    if let Some(id) = p.get("id").and_then(|x| x.as_str()) {
                        make_offer(ctx, id).await;
                    }
                }
            }
            ctx.sync_count();
            ctx.send_state();
        }
        "waiting" => {
            if let Some(id) = v.get("self_id").and_then(|s| s.as_str()) {
                ctx.self_id.set(id.to_string());
            }
            ctx.phase.set(Phase::Waiting);
        }
        "denied" => {
            ctx.phase.set(Phase::Denied);
            if let Some(ws) = ctx.ws.borrow().as_ref() {
                let _ = ws.close();
            }
        }
        "join_request" => {
            if let (Some(id), Some(name)) = (
                v.get("peer_id").and_then(|x| x.as_str()),
                v.get("name").and_then(|x| x.as_str()),
            ) {
                ctx.names.borrow_mut().insert(id.to_string(), name.to_string());
                ctx.pending.update(|list| {
                    if !list.iter().any(|(pid, _)| pid == id) {
                        list.push((id.to_string(), name.to_string()));
                    }
                });
            }
        }
        "pending_left" => {
            if let Some(id) = v.get("peer_id").and_then(|x| x.as_str()) {
                ctx.pending.update(|list| list.retain(|(pid, _)| pid != id));
            }
        }
        "peer_joined" => {
            if let (Some(id), Some(name)) = (
                v.get("peer_id").and_then(|x| x.as_str()),
                v.get("name").and_then(|x| x.as_str()),
            ) {
                ctx.names.borrow_mut().insert(id.to_string(), name.to_string());
                // Tamu baru yang akan initiate offer; kita cukup catat nama lalu
                // kirim ulang status media kita agar avatar/mic di sisi dia benar.
                ctx.send_state();
            }
        }
        "peer_left" => {
            if let Some(id) = v.get("peer_id").and_then(|x| x.as_str()) {
                remove_tile(id);
                if let Some(pc) = ctx.pcs.borrow_mut().remove(id) {
                    // Lepas handler sebelum drop closure (di baris berikut);
                    // close() menghentikan product lanjutan.
                    pc.set_onicecandidate(None);
                    pc.set_ontrack(None);
                    pc.close();
                }
                ctx.pc_closures.borrow_mut().remove(id); // drop closure peer ini
                ctx.names.borrow_mut().remove(id);
                ctx.states.borrow_mut().remove(id);
                ctx.remote_ready.borrow_mut().remove(id);
                ctx.pending_ice.borrow_mut().remove(id);
                ctx.sync_count();
            }
        }
        "chat" => {
            if let (Some(name), Some(text)) = (
                v.get("name").and_then(|x| x.as_str()),
                v.get("text").and_then(|x| x.as_str()),
            ) {
                ctx.chat
                    .update(|list| list.push((name.to_string(), text.to_string())));
            }
        }
        "peer_state" => {
            if let Some(id) = v.get("peer_id").and_then(|x| x.as_str()) {
                let mic = v.get("mic").and_then(|b| b.as_bool()).unwrap_or(true);
                let cam = v.get("cam").and_then(|b| b.as_bool()).unwrap_or(true);
                ctx.states.borrow_mut().insert(id.to_string(), (mic, cam));
                set_tile_state(id, mic, cam);
            }
        }
        "roster" => {
            if let Some(peers) = v.get("peers").and_then(|p| p.as_array()) {
                for p in peers {
                    if let (Some(id), Some(name)) = (
                        p.get("id").and_then(|x| x.as_str()),
                        p.get("name").and_then(|x| x.as_str()),
                    ) {
                        ctx.names.borrow_mut().insert(id.to_string(), name.to_string());
                    }
                }
            }
        }
        "signal" => {
            let from = v.get("from").and_then(|x| x.as_str()).unwrap_or("");
            let data = v.get("data").cloned().unwrap_or(serde_json::Value::Null);
            if from.is_empty() {
                return;
            }
            match data.get("sdp_type").and_then(|t| t.as_str()) {
                Some("offer") => {
                    let sdp = data.get("sdp").and_then(|s| s.as_str()).unwrap_or("");
                    handle_offer(ctx, from, sdp).await;
                    ctx.sync_count();
                }
                Some("answer") => {
                    let sdp = data.get("sdp").and_then(|s| s.as_str()).unwrap_or("");
                    handle_answer(ctx, from, sdp).await;
                }
                _ => {
                    if data.get("candidate").is_some() {
                        handle_candidate(ctx, from, &data).await;
                    }
                }
            }
        }
        "meeting_ended" => {
            // Dulu di sini ada pembersihan SEPARUH — peer connection dan
            // WebSocket ditutup, kamera tidak. Saat host membubarkan meet,
            // peserta lain ditinggalkan dengan kamera yang masih menyala.
            // Tak ada alasan jalur ini berbeda dari menekan tombol keluar.
            ctx.phase.set(Phase::Ended);
            teardown(ctx);
        }
        "error" => {
            ctx.error_msg.set(Some(
                v.get("message").and_then(|m| m.as_str()).unwrap_or("Error").to_string(),
            ));
            ctx.phase.set(Phase::Error);
        }
        _ => {}
    }
}

/// Siapkan preview kamera (green room). Dipanggil saat mount untuk host & tamu.
pub(super) async fn setup_preview(
    ctx: Ctx,
    local_sig: RwSignal<Option<send_wrapper::SendWrapper<web_sys::MediaStream>>>,
) {
    let stream = match crate::web::rtc::request_camera_mic()
        .await
        .map_err(|e| e.user_message())
    {
        Ok(s) => s,
        Err(e) => {
            ctx.error_msg.set(Some(e));
            ctx.phase.set(Phase::Error);
            return;
        }
    };
    // Izin kamera bisa memakan waktu lama — orangnya sempat menekan tombol
    // kembali sebelum dialognya dijawab. Bila itu terjadi, `teardown` sudah
    // lewat, dan menyimpan stream ini berarti menyimpannya di tempat yang tak
    // akan pernah dibersihkan lagi: kamera menyala di halaman yang bahkan sudah
    // ditinggalkan.
    if ctx.bubar.get() {
        let tracks = stream.get_tracks();
        for i in 0..tracks.length() {
            let t: web_sys::MediaStreamTrack = tracks.get(i).unchecked_into();
            t.stop();
        }
        return;
    }
    *ctx.local.borrow_mut() = Some(stream.clone());
    local_sig.set(Some(send_wrapper::SendWrapper::new(stream)));
    ctx.phase.set(Phase::Prejoin);
}

/// Buka koneksi WS lalu kirim `join`. Media sudah disiapkan di green room.
pub(super) async fn connect(ctx: Ctx, room_id: String, as_host: bool, name: String) {
    // Ambil ICE server (STUN + TURN) sekali untuk semua peer connection.
    *ctx.ice.borrow_mut() = Some(crate::web::rtc::fetch_ice_servers().await);

    let url = build_ws_url(&format!("/ws/meet/{room_id}"));
    let ws = match web_sys::WebSocket::new(&url) {
        Ok(ws) => ws,
        Err(e) => {
            ctx.error_msg.set(Some(format!("WS gagal: {e:?}")));
            ctx.phase.set(Phase::Error);
            return;
        }
    };

    // onmessage → antri ke channel, diproses SATU consumer secara BERURUTAN.
    // KRUSIAL: kalau tiap pesan di-spawn paralel, ICE candidate bisa diproses
    // sebelum offer/answer selesai setRemoteDescription → addIceCandidate gagal
    // → peer tak pernah terhubung. Pemrosesan berurutan menjamin offer→answer→
    // candidate sesuai urutan tiba.
    let on_msg = {
        let (tx, mut rx) = futures::channel::mpsc::unbounded::<String>();
        let cb = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |e: web_sys::MessageEvent| {
            if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
                let _ = tx.unbounded_send(txt.into());
            }
        });
        ws.set_onmessage(Some(cb.as_ref().unchecked_ref()));

        let ctx2 = ctx.clone();
        wasm_bindgen_futures::spawn_local(async move {
            use futures::StreamExt;
            while let Some(s) = rx.next().await {
                handle_msg(&ctx2, &s).await;
                // Hentikan consumer saat meet usai agar tidak menggantung.
                if matches!(
                    ctx2.phase.get_untracked(),
                    Phase::Ended | Phase::Denied | Phase::Error
                ) {
                    break;
                }
            }
        });
        cb
    };
    // onopen → kirim join.
    let on_open = {
        let ws_ref = ws.clone();
        let join = json!({ "type": "join", "as_host": as_host, "name": name });
        let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            let _ = ws_ref.send_with_str(&join.to_string());
        });
        ws.set_onopen(Some(cb.as_ref().unchecked_ref()));
        cb
    };
    // onerror.
    let on_err = {
        let ctx2 = ctx.clone();
        let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            ctx2.error_msg.set(Some("Koneksi signaling terputus".to_string()));
        });
        ws.set_onerror(Some(cb.as_ref().unchecked_ref()));
        cb
    };

    // Pegang ketiga closure WS untuk sesi ini → di-drop saat teardown.
    *ctx.ws_closures.borrow_mut() = Some((on_msg, on_open, on_err));
    *ctx.ws.borrow_mut() = Some(ws);
}

/// Hentikan semua: tutup PC, stop track kamera/mic, tutup WS.
pub(super) fn teardown(ctx: &Ctx) {
    // ── MEDIA DIMATIKAN PALING DULU ───────────────────────────────────────
    // Bukan urutan yang sembarang. Segala yang di bawah ini — meminjam
    // `RefCell`, menutup peer connection yang bisa memicu handler, menutup
    // WebSocket — punya cara masing-masing untuk gagal atau memanikkan, dan
    // apa pun yang gagal di sana akan melewati baris yang mematikan kamera.
    //
    // Dari sudut pandang orangnya, itu kegagalan yang paling tidak bisa
    // dimaafkan: lampu kamera tetap menyala sesudah ia merasa sudah keluar.
    // Jadi kamera mati lebih dulu, selalu, apa pun yang terjadi sesudahnya.
    ctx.bubar.set(true);
    for slot in [&ctx.local, &ctx.screen] {
        if let Some(stream) = slot.borrow().clone() {
            let tracks = stream.get_tracks();
            for i in 0..tracks.length() {
                let t: web_sys::MediaStreamTrack = tracks.get(i).unchecked_into();
                // Lepas `onended` dulu: menghentikan track berbagi layar akan
                // memicunya, dan penanganannya mencoba memulihkan kamera —
                // tepat pada saat kita sedang membubarkan semuanya.
                t.set_onended(None);
                t.stop();
            }
        }
    }
    *ctx.local.borrow_mut() = None;
    *ctx.screen.borrow_mut() = None;

    for (_, pc) in ctx.pcs.borrow_mut().drain() {
        pc.set_onicecandidate(None);
        pc.set_ontrack(None);
        pc.close();
    }
    // Drop semua closure per-peer (onicecandidate/ontrack) → tak bocor.
    ctx.pc_closures.borrow_mut().clear();
    let ws = ctx.ws.borrow().clone();
    if let Some(ws) = ws {
        // Lepas handler sebelum drop closure di bawah; close() menghentikan product.
        ws.set_onmessage(None);
        ws.set_onopen(None);
        ws.set_onerror(None);
        let _ = ws.close();
    }
    *ctx.ws.borrow_mut() = None;
    // Drop closure WS signaling (onmessage/onopen/onerror) → tak bocor.
    *ctx.ws_closures.borrow_mut() = None;
}
