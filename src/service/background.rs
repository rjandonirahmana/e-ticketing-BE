//! background.rs — Eksekutor tugas latar bounded (fire-and-forget yang aman).
//!
//! Masalah pola `tokio::spawn` telanjang di jalur order: saat flash-sale ribuan
//! order/detik menelurkan ribuan task notifikasi sekaligus, semuanya berebut
//! koneksi pool DB yang sama dengan jalur checkout kritis → checkout melambat.
//!
//! Eksekutor ini membatasi DUA hal:
//!   1. Jumlah job antre (channel bounded) — plafon memori.
//!   2. Job berjalan serentak (semaphore) — sisakan koneksi pool untuk checkout.
//!
//! Job bersifat best-effort: bila antrean penuh, job dibuang (state order tetap
//! konsisten karena sudah di-commit ke DB; yang hilang hanya notifikasi live).

use std::future::Future;
use std::sync::Arc;

use futures::future::BoxFuture;
use tokio::sync::{mpsc, Semaphore};

pub struct BackgroundJobs {
    tx: mpsc::Sender<BoxFuture<'static, ()>>,
}

impl BackgroundJobs {
    /// `concurrency`: maksimum job berjalan serentak (batasi tekanan ke pool DB).
    /// `queue_cap`: maksimum job menunggu di antrean sebelum di-drop.
    pub fn new(concurrency: usize, queue_cap: usize) -> Arc<Self> {
        let (tx, mut rx) = mpsc::channel::<BoxFuture<'static, ()>>(queue_cap.max(1));
        let sem = Arc::new(Semaphore::new(concurrency.max(1)));

        // Satu dispatcher menarik dari antrean, menahan permit (backpressure),
        // lalu menjalankan job. Jumlah task hidup ≤ concurrency + antrean.
        tokio::spawn(async move {
            while let Some(job) = rx.recv().await {
                let Ok(permit) = sem.clone().acquire_owned().await else {
                    break; // semaphore ditutup — mustahil di sini, tapi aman
                };
                tokio::spawn(async move {
                    job.await;
                    drop(permit);
                });
            }
        });

        Arc::new(Self { tx })
    }

    /// Jadwalkan job latar. Best-effort: `false` bila antrean penuh (job dibuang)
    /// agar jalur request TIDAK PERNAH memblok menunggu slot notifikasi.
    pub fn spawn(&self, fut: impl Future<Output = ()> + Send + 'static) -> bool {
        match self.tx.try_send(Box::pin(fut)) {
            Ok(_) => true,
            Err(_) => {
                tracing::warn!("background jobs queue penuh — job dilewati (best-effort)");
                false
            }
        }
    }
}
