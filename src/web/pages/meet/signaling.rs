//! meet/signaling.rs — Loop pesan WebSocket: tangani admit/roster/signal/state,
//! plus lifecycle koneksi (preview, connect, teardown).

use leptos::prelude::*;
use serde_json::json;
use wasm_bindgen::prelude::*;

use super::tiles::{remove_tile, set_tile_state};
use super::webrtc::{handle_answer, handle_candidate, handle_offer, make_offer};
use super::{build_ws_url, get_user_media, Ctx, Phase};

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
                    pc.close();
                }
                ctx.names.borrow_mut().remove(id);
                ctx.states.borrow_mut().remove(id);
                ctx.sync_count();
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
            ctx.phase.set(Phase::Ended);
            for (_, pc) in ctx.pcs.borrow_mut().drain() {
                pc.close();
            }
            if let Some(ws) = ctx.ws.borrow().as_ref() {
                let _ = ws.close();
            }
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
    let stream = match get_user_media().await {
        Ok(s) => s,
        Err(e) => {
            ctx.error_msg.set(Some(e));
            ctx.phase.set(Phase::Error);
            return;
        }
    };
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

    // onmessage → dispatch async.
    {
        let ctx2 = ctx.clone();
        let cb = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |e: web_sys::MessageEvent| {
            if let Ok(txt) = e.data().dyn_into::<js_sys::JsString>() {
                let s: String = txt.into();
                let ctx3 = ctx2.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    handle_msg(&ctx3, &s).await;
                });
            }
        });
        ws.set_onmessage(Some(cb.as_ref().unchecked_ref()));
        cb.forget();
    }
    // onopen → kirim join.
    {
        let ws_ref = ws.clone();
        let join = json!({ "type": "join", "as_host": as_host, "name": name });
        let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            let _ = ws_ref.send_with_str(&join.to_string());
        });
        ws.set_onopen(Some(cb.as_ref().unchecked_ref()));
        cb.forget();
    }
    // onerror.
    {
        let ctx2 = ctx.clone();
        let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |_| {
            ctx2.error_msg.set(Some("Koneksi signaling terputus".to_string()));
        });
        ws.set_onerror(Some(cb.as_ref().unchecked_ref()));
        cb.forget();
    }

    *ctx.ws.borrow_mut() = Some(ws);
}

/// Hentikan semua: tutup PC, stop track kamera/mic, tutup WS.
pub(super) fn teardown(ctx: &Ctx) {
    for (_, pc) in ctx.pcs.borrow_mut().drain() {
        pc.close();
    }
    let stream = ctx.local.borrow().clone();
    if let Some(stream) = stream {
        let tracks = stream.get_tracks();
        for i in 0..tracks.length() {
            let t: web_sys::MediaStreamTrack = tracks.get(i).unchecked_into();
            t.stop();
        }
    }
    let ws = ctx.ws.borrow().clone();
    if let Some(ws) = ws {
        let _ = ws.close();
    }
}
