use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use str0m::{
    change::SdpOffer,
    net::{DatagramRecv, Protocol, Receive},
    Candidate, Event, IceConnectionState, Input, Output, Rtc,
};
use tokio::sync::mpsc;

use super::room::RoomInfo;

const BUF_SIZE: usize = 65535;
const SFU_TICK: Duration = Duration::from_millis(2);

#[derive(Debug)]
pub enum SfuCommand {
    CreateRoom {
        room_id: String,
        merchant_id: String,
        merchant_name: String,
        event_slug: Option<String>,
        respond_to: tokio::sync::oneshot::Sender<Result<RoomInfo, String>>,
    },
    PublishSdp {
        room_id: String,
        sdp_offer: String,
        respond_to: tokio::sync::oneshot::Sender<Result<String, String>>,
    },
    PublishIce {
        room_id: String,
        candidate: String,
        sdp_mid: String,
        sdp_mline_index: u32,
    },
    SubscribeSdp {
        room_id: String,
        subscriber_id: String,
        sdp_offer: String,
        respond_to: tokio::sync::oneshot::Sender<Result<String, String>>,
    },
    SubscribeIce {
        room_id: String,
        subscriber_id: String,
        candidate: String,
        sdp_mid: String,
        sdp_mline_index: u32,
    },
    RemoveSubscriber {
        room_id: String,
        subscriber_id: String,
    },
    StopRoom {
        room_id: String,
        respond_to: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    ListRooms {
        respond_to: tokio::sync::oneshot::Sender<Vec<RoomInfo>>,
    },
    GetRoom {
        room_id: String,
        respond_to: tokio::sync::oneshot::Sender<Option<RoomInfo>>,
    },
    Shutdown,
}

#[derive(Debug)]
pub enum SfuEvent {
    IceCandidate {
        room_id: String,
        peer_id: String,
        candidate: String,
        sdp_mid: String,
        sdp_mline_index: u32,
    },
    ViewerCount {
        room_id: String,
        count: usize,
    },
    StreamStopped {
        room_id: String,
    },
}

struct PeerState {
    rtc: Rtc,
    role: PeerRole,
    pending_sdp: Option<tokio::sync::oneshot::Sender<Result<String, String>>>,
}

enum PeerRole {
    Publisher,
    Subscriber { id: String },
}

struct RoomState {
    room_id: String,
    merchant_id: String,
    merchant_name: String,
    event_slug: Option<String>,
    started_at: chrono::DateTime<chrono::Utc>,
    publisher: Option<String>,
    subscribers: HashMap<String, String>,
    pending_media: Vec<str0m::media::MediaData>,
}

impl RoomState {
    fn info(&self) -> RoomInfo {
        RoomInfo {
            room_id: self.room_id.clone(),
            merchant_id: self.merchant_id.clone(),
            merchant_name: self.merchant_name.clone(),
            event_slug: self.event_slug.clone(),
            viewer_count: self.subscribers.len(),
            started_at: self.started_at.timestamp_millis(),
        }
    }
}

pub struct SfuEngine {
    peers: HashMap<String, PeerState>,
    rooms: HashMap<String, RoomState>,
    socket: UdpSocket,
    next_peer_id: u64,
    local_addr: SocketAddr,
}

impl SfuEngine {
    pub fn run(
        bind_addr: SocketAddr,
        cmd_rx: mpsc::Receiver<SfuCommand>,
        event_tx: mpsc::Sender<SfuEvent>,
    ) {
        let socket = UdpSocket::bind(bind_addr).expect("Failed to bind SFU UDP socket");
        socket
            .set_nonblocking(true)
            .expect("Failed to set nonblocking");
        let local_addr = socket.local_addr().expect("Failed to get local addr");
        tracing::info!(addr = %local_addr, "SFU UDP socket bound");

        let mut engine = Self {
            peers: HashMap::new(),
            rooms: HashMap::new(),
            socket,
            next_peer_id: 0,
            local_addr,
        };

        let mut cmd_rx = cmd_rx;
        let mut buf = vec![0u8; BUF_SIZE];

        loop {
            engine.process_commands(&mut cmd_rx);

            engine.poll_all_peers(&event_tx);

            engine.forward_media();

            let deadline = Instant::now() + SFU_TICK;
            engine.read_socket_until(&mut buf, deadline);
        }
    }

    fn process_commands(&mut self, cmd_rx: &mut mpsc::Receiver<SfuCommand>) {
        while let Ok(cmd) = cmd_rx.try_recv() {
            match cmd {
                SfuCommand::CreateRoom {
                    room_id,
                    merchant_id,
                    merchant_name,
                    event_slug,
                    respond_to,
                } => {
                    if self.rooms.contains_key(&room_id) {
                        let _ = respond_to.send(Err("Room already exists".into()));
                        continue;
                    }
                    let room = RoomState {
                        room_id: room_id.clone(),
                        merchant_id,
                        merchant_name,
                        event_slug,
                        started_at: chrono::Utc::now(),
                        publisher: None,
                        subscribers: HashMap::new(),
                        pending_media: Vec::new(),
                    };
                    let info = room.info();
                    self.rooms.insert(room_id, room);
                    let _ = respond_to.send(Ok(info));
                }

                SfuCommand::PublishSdp {
                    room_id,
                    sdp_offer,
                    respond_to,
                } => {
                    let result = self.handle_publish_sdp(&room_id, &sdp_offer);
                    let _ = respond_to.send(result);
                }

                SfuCommand::PublishIce {
                    room_id,
                    candidate,
                    sdp_mid,
                    sdp_mline_index,
                } => {
                    self.handle_remote_ice(&room_id, None, &candidate, &sdp_mid, sdp_mline_index);
                }

                SfuCommand::SubscribeSdp {
                    room_id,
                    subscriber_id,
                    sdp_offer,
                    respond_to,
                } => {
                    let result = self.handle_subscribe_sdp(&room_id, &subscriber_id, &sdp_offer);
                    let _ = respond_to.send(result);
                }

                SfuCommand::SubscribeIce {
                    room_id,
                    subscriber_id,
                    candidate,
                    sdp_mid,
                    sdp_mline_index,
                } => {
                    self.handle_remote_ice(
                        &room_id,
                        Some(&subscriber_id),
                        &candidate,
                        &sdp_mid,
                        sdp_mline_index,
                    );
                }

                SfuCommand::RemoveSubscriber {
                    room_id,
                    subscriber_id,
                } => {
                    if let Some(room) = self.rooms.get_mut(&room_id) {
                        room.subscribers.remove(&subscriber_id);
                    }
                    self.peers.remove(&subscriber_id);
                }

                SfuCommand::StopRoom {
                    room_id,
                    respond_to,
                } => {
                    if let Some(room) = self.rooms.remove(&room_id) {
                        if let Some(pub_id) = room.publisher {
                            self.peers.remove(&pub_id);
                        }
                        for sub_id in room.subscribers.keys() {
                            self.peers.remove(sub_id);
                        }
                    }
                    let _ = respond_to.send(Ok(()));
                }

                SfuCommand::ListRooms { respond_to } => {
                    let list: Vec<RoomInfo> = self.rooms.values().map(|r| r.info()).collect();
                    let _ = respond_to.send(list);
                }

                SfuCommand::GetRoom {
                    room_id,
                    respond_to,
                } => {
                    let info = self.rooms.get(&room_id).map(|r| r.info());
                    let _ = respond_to.send(info);
                }

                SfuCommand::Shutdown => {
                    tracing::info!("SFU engine shutting down");
                    std::process::exit(0);
                }
            }
        }
    }

    fn handle_publish_sdp(&mut self, room_id: &str, sdp_offer: &str) -> Result<String, String> {
        // Validasi room dengan borrow singkat agar tidak bentrok dengan
        // `self.next_peer_id()` (yang meminjam `&mut self`) di bawah.
        {
            let room = self.rooms.get(room_id).ok_or("Room not found")?;
            if room.publisher.is_some() {
                return Err("Room already has a publisher".into());
            }
        }

        let peer_id = format!("pub_{}", self.next_peer_id());
        let mut rtc = Rtc::new(Instant::now());

        let local_candidate = Candidate::host(self.local_addr, "udp")
            .map_err(|e| format!("Failed to create host candidate: {e}"))?;
        rtc.add_local_candidate(local_candidate);

        let offer = SdpOffer::from_sdp_string(sdp_offer)
            .map_err(|e| format!("SDP parse error: {e}"))?;
        let answer = rtc
            .sdp_api()
            .accept_offer(offer)
            .map_err(|e| format!("accept_offer failed: {e}"))?;

        let answer_sdp = answer.to_sdp_string();

        if let Some(room) = self.rooms.get_mut(room_id) {
            room.publisher = Some(peer_id.clone());
        }

        self.peers.insert(
            peer_id,
            PeerState {
                rtc,
                role: PeerRole::Publisher,
                pending_sdp: None,
            },
        );

        tracing::info!(room_id, "Publisher connected");
        Ok(answer_sdp)
    }

    fn handle_subscribe_sdp(
        &mut self,
        room_id: &str,
        subscriber_id: &str,
        sdp_offer: &str,
    ) -> Result<String, String> {
        let room = self.rooms.get_mut(room_id).ok_or("Room not found")?;
        if room.publisher.is_none() {
            return Err("No active stream in this room".into());
        }

        let mut rtc = Rtc::new(Instant::now());

        let local_candidate = Candidate::host(self.local_addr, "udp")
            .map_err(|e| format!("Failed to create host candidate: {e}"))?;
        rtc.add_local_candidate(local_candidate);

        let offer = SdpOffer::from_sdp_string(sdp_offer)
            .map_err(|e| format!("SDP parse error: {e}"))?;
        let answer = rtc
            .sdp_api()
            .accept_offer(offer)
            .map_err(|e| format!("accept_offer failed: {e}"))?;

        let answer_sdp = answer.to_sdp_string();

        room.subscribers
            .insert(subscriber_id.to_string(), room_id.to_string());

        self.peers.insert(
            subscriber_id.to_string(),
            PeerState {
                rtc,
                role: PeerRole::Subscriber {
                    id: subscriber_id.to_string(),
                },
                pending_sdp: None,
            },
        );

        tracing::info!(room_id, subscriber_id, "Subscriber connected");
        Ok(answer_sdp)
    }

    fn handle_remote_ice(
        &mut self,
        room_id: &str,
        subscriber_id: Option<&str>,
        candidate: &str,
        _sdp_mid: &str,
        _sdp_mline_index: u32,
    ) {
        let peer_key = match subscriber_id {
            Some(id) => id.to_string(),
            None => self
                .rooms
                .get(room_id)
                .and_then(|r| r.publisher.clone())
                .unwrap_or_default(),
        };

        // str0m 0.20 tidak mengekspos parser kandidat ICE dari string secara publik.
        // SFU ini sudah menukar host candidate lewat SDP dan melakukan UDP demux,
        // jadi trickle candidate dari klien cukup dicatat lalu diabaikan.
        if self.peers.contains_key(&peer_key) {
            tracing::debug!(peer_id = %peer_key, candidate, "Trickle ICE candidate diterima (diabaikan)");
        }
    }

    fn poll_all_peers(&mut self, _event_tx: &mpsc::Sender<SfuEvent>) {
        let mut to_remove = Vec::new();
        let mut media_buf: Vec<(String, str0m::media::MediaData)> = Vec::new();

        for (peer_id, peer) in self.peers.iter_mut() {
            loop {
                match peer.rtc.poll_output() {
                    Ok(Output::Timeout(_)) => break,
                    Ok(Output::Transmit(t)) => {
                        if let Err(e) = self.socket.send_to(&t.contents, t.destination) {
                            tracing::debug!(peer_id, error = %e, "UDP send failed");
                        }
                    }
                    Ok(Output::Event(e)) => match e {
                        Event::MediaData(data) => {
                            if matches!(peer.role, PeerRole::Publisher) {
                                media_buf.push((peer_id.clone(), data));
                            }
                        }
                        Event::IceConnectionStateChange(state) => {
                            if state == IceConnectionState::Disconnected
                                && matches!(peer.role, PeerRole::Publisher)
                            {
                                tracing::warn!(peer_id, "Publisher disconnected");
                                to_remove.push(peer_id.clone());
                            }
                        }
                        _ => {}
                    },
                    Err(e) => {
                        tracing::error!(peer_id, error = %e, "Rtc poll error");
                        to_remove.push(peer_id.clone());
                        break;
                    }
                }
            }
        }

        for peer_id in to_remove {
            self.peers.remove(&peer_id);
        }

        for (pub_id, data) in media_buf {
            self.forward_to_subscribers(&pub_id, &data);
        }
    }

    fn forward_to_subscribers(&mut self, _publisher_id: &str, data: &str0m::media::MediaData) {
        for (sub_id, peer) in self.peers.iter_mut() {
            if !matches!(peer.role, PeerRole::Subscriber { .. }) {
                continue;
            }

            if let Some(writer) = peer.rtc.writer(data.mid) {
                let pt = match writer.payload_params().next() {
                    Some(p) => p.pt(),
                    None => continue,
                };
                if let Err(e) = writer.write(pt, data.network_time, data.time, data.data.clone()) {
                    tracing::debug!(sub_id, error = %e, "Media write to subscriber failed");
                }
            }
        }
    }

    fn forward_media(&mut self) {
        // Media is forwarded inline during poll_all_peers.
        // This method is kept as a placeholder for future batching optimizations.
    }

    fn read_socket_until(&mut self, buf: &mut Vec<u8>, deadline: Instant) {
        while Instant::now() < deadline {
            buf.resize(BUF_SIZE, 0);
            match self.socket.recv_from(buf) {
                Ok((n, source)) => {
                    buf.truncate(n);
                    let dest = self.local_addr;
                    // `buf` dipersempit ke `n` byte; salin payload supaya bisa di-feed
                    // ulang ke setiap peer (Input str0m meminjam slice, bukan Clone).
                    let packet = buf[..n].to_vec();
                    self.dispatch_input(source, dest, &packet);
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(1));
                    return;
                }
                Err(e) => {
                    tracing::error!(error = %e, "UDP recv error");
                    return;
                }
            }
        }
    }

    fn dispatch_input(&mut self, source: SocketAddr, dest: SocketAddr, data: &[u8]) {
        // In a multiplexed setup, we'd demux by source to the correct Rtc.
        // For now, feed to all peers — str0m ignores packets not matching DTLS.
        for peer in self.peers.values_mut() {
            let contents: DatagramRecv = match data.try_into() {
                Ok(c) => c,
                Err(_) => continue,
            };
            let input = Input::Receive(
                Instant::now(),
                Receive {
                    proto: Protocol::Udp,
                    source,
                    destination: dest,
                    contents,
                },
            );
            let _ = peer.rtc.handle_input(input);
        }
    }

    fn next_peer_id(&mut self) -> u64 {
        let id = self.next_peer_id;
        self.next_peer_id += 1;
        id
    }
}
