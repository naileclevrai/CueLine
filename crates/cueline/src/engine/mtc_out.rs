//! MIDI Timecode output.
//!
//! A dedicated time-critical thread schedules each quarter-frame against
//! the DLL-filtered audio clock, so MTC follows the samples actually leaving
//! the audio interface rather than the jittery callback timing. It sleeps
//! until ~1 ms before each message and then spins for sub-millisecond
//! accuracy.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use cueline_core::mtc::{full_frame, MtcSequencer};
use cueline_core::FrameRate;
use midir::{MidiOutput, MidiOutputConnection};

use super::clock::now_ns;
use super::shared::EngineShared;

const SPIN_NS: u64 = 1_200_000;

/// Destination for MIDI bytes; abstracted so scheduling can be tested.
pub trait MidiSink: Send {
    fn send(&mut self, bytes: &[u8]) -> Result<(), String>;
}

impl MidiSink for MidiOutputConnection {
    fn send(&mut self, bytes: &[u8]) -> Result<(), String> {
        MidiOutputConnection::send(self, bytes).map_err(|e| e.to_string())
    }
}

pub fn list_ports() -> Vec<String> {
    let Ok(out) = MidiOutput::new("CueLine") else { return Vec::new() };
    out.ports().iter().filter_map(|p| out.port_name(p).ok()).collect()
}

enum Msg {
    SetPort(Option<String>),
    SetEnabled(bool),
    SetOffsetMs(f32),
    SetTimecode { rate: FrameRate, start_frames: i64 },
    Quit,
}

#[derive(Default)]
pub struct MtcStatus {
    pub connected: AtomicBool,
    pub sending: AtomicBool,
    pub error: Mutex<Option<String>>,
}

pub struct MtcOutput {
    tx: Sender<Msg>,
    pub status: Arc<MtcStatus>,
    thread: Option<JoinHandle<()>>,
}

impl MtcOutput {
    pub fn spawn(shared: Arc<EngineShared>) -> Self {
        let (tx, rx) = mpsc::channel();
        let status = Arc::new(MtcStatus::default());
        let st = status.clone();
        let thread = std::thread::Builder::new()
            .name("cueline-mtc".into())
            .spawn(move || Worker::new(shared, st, rx).run())
            .expect("spawn MTC thread");
        Self { tx, status, thread: Some(thread) }
    }

    pub fn set_port(&self, port: Option<String>) {
        let _ = self.tx.send(Msg::SetPort(port));
    }
    pub fn set_enabled(&self, on: bool) {
        let _ = self.tx.send(Msg::SetEnabled(on));
    }
    /// Positive values send MTC earlier, compensating receiver latency.
    pub fn set_offset_ms(&self, ms: f32) {
        let _ = self.tx.send(Msg::SetOffsetMs(ms));
    }
    pub fn set_timecode(&self, rate: FrameRate, start_frames: i64) {
        let _ = self.tx.send(Msg::SetTimecode { rate, start_frames });
    }
}

impl Drop for MtcOutput {
    fn drop(&mut self) {
        let _ = self.tx.send(Msg::Quit);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

struct Worker {
    shared: Arc<EngineShared>,
    status: Arc<MtcStatus>,
    rx: Receiver<Msg>,
    conn: Option<Box<dyn MidiSink>>,
    enabled: bool,
    offset_ms: f32,
    seq: MtcSequencer,
    next_q: Option<i64>,
    /// Position for which a full-frame was last sent while stopped.
    parked_at: Option<i64>,
}

impl Worker {
    fn new(shared: Arc<EngineShared>, status: Arc<MtcStatus>, rx: Receiver<Msg>) -> Self {
        Self {
            shared,
            status,
            rx,
            conn: None,
            enabled: false,
            offset_ms: 0.0,
            seq: MtcSequencer { rate: FrameRate::default(), start_frames: 0 },
            next_q: None,
            parked_at: None,
        }
    }

    fn run(mut self) {
        crate::platform::promote_timing_thread();
        loop {
            if !self.drain_messages() {
                return;
            }
            if !self.enabled || self.conn.is_none() {
                self.status.sending.store(false, Ordering::Relaxed);
                self.next_q = None;
                self.parked_at = None;
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
            self.tick();
        }
    }

    /// Returns false when the thread must exit.
    fn drain_messages(&mut self) -> bool {
        loop {
            match self.rx.try_recv() {
                Ok(Msg::Quit) | Err(TryRecvError::Disconnected) => return false,
                Ok(Msg::SetPort(p)) => self.connect(p),
                Ok(Msg::SetEnabled(on)) => self.enabled = on,
                Ok(Msg::SetOffsetMs(ms)) => self.offset_ms = ms,
                Ok(Msg::SetTimecode { rate, start_frames }) => {
                    self.seq = MtcSequencer { rate, start_frames };
                    self.next_q = None;
                    self.parked_at = None;
                }
                Err(TryRecvError::Empty) => return true,
            }
        }
    }

    fn connect(&mut self, port: Option<String>) {
        self.conn = None;
        self.status.connected.store(false, Ordering::Relaxed);
        let Some(name) = port else {
            *self.status.error.lock().unwrap() = None;
            return;
        };
        let result = (|| {
            let out = MidiOutput::new("CueLine").map_err(|e| e.to_string())?;
            let port = out
                .ports()
                .into_iter()
                .find(|p| out.port_name(p).ok().as_deref() == Some(name.as_str()))
                .ok_or_else(|| format!("MIDI port \"{name}\" not found"))?;
            out.connect(&port, "CueLine MTC").map_err(|e| e.to_string())
        })();
        match result {
            Ok(c) => {
                self.conn = Some(Box::new(c));
                self.status.connected.store(true, Ordering::Relaxed);
                *self.status.error.lock().unwrap() = None;
                self.parked_at = None;
            }
            Err(e) => {
                log::warn!("MTC: {e}");
                *self.status.error.lock().unwrap() = Some(e);
            }
        }
    }

    fn send(&mut self, bytes: &[u8]) {
        if let Some(c) = &mut self.conn {
            if let Err(e) = c.send(bytes) {
                log::warn!("MTC send failed: {e}");
            }
        }
    }

    fn send_full_frame(&mut self, position: i64, sample_rate: u32) {
        let frame = self.seq.rate.frame_at_sample(position, sample_rate);
        let msg = full_frame(self.seq.label_at_frame(frame), self.seq.rate);
        self.send(&msg);
    }

    fn tick(&mut self) {
        let sr = self.shared.sample_rate.load(Ordering::Relaxed);
        let snap = self.shared.clock.read();

        if !snap.playing {
            self.status.sending.store(false, Ordering::Relaxed);
            self.next_q = None;
            if self.parked_at != Some(snap.position) {
                self.send_full_frame(snap.position, sr);
                self.parked_at = Some(snap.position);
            }
            std::thread::sleep(Duration::from_millis(5));
            return;
        }
        self.parked_at = None;
        self.status.sending.store(true, Ordering::Relaxed);

        let lead = self.offset_ms as f64 * sr as f64 / 1000.0;
        let pos_now = snap.position_at(now_ns()) + lead;
        let expected = self.seq.next_quarter_at(pos_now.ceil() as i64, sr);
        let q = match self.next_q {
            Some(q) if (q - expected).abs() <= 2 => q,
            _ => {
                // Start, seek or stall: re-anchor and tell receivers where we are.
                self.send_full_frame(pos_now as i64, sr);
                expected
            }
        };

        let target_ns = snap.ns_at(self.seq.quarter_sample(q, sr) - lead);
        let now = now_ns() as f64;
        if target_ns - now > SPIN_NS as f64 {
            // Sleep, then re-read the clock: it may have moved (seek/stop).
            let ms = ((target_ns - now - SPIN_NS as f64) / 1e6).clamp(0.0, 5.0);
            self.next_q = Some(q);
            std::thread::sleep(Duration::from_secs_f64(ms / 1000.0));
            return;
        }
        while (now_ns() as f64) < target_ns {
            std::hint::spin_loop();
        }
        let msg = self.seq.message(q);
        self.send(&msg);
        self.next_q = Some(q + 1);
    }
}
