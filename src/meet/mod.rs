//! meet — Konferensi video "zoom meet" P2P mesh dengan waiting room.
//!
//! Berbeda dari `live` (SFU 1-ke-banyak), `meet` adalah mesh dua arah antara
//! host (merchant) dan tamu undangan. Server HANYA relay signaling + kontrol
//! admit (hanya host yang boleh mengizinkan tamu masuk). Tidak ada media yang
//! melewati server — browser saling terhubung langsung.

pub mod api;
pub mod room;
pub mod service;

pub use service::MeetService;
