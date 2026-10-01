//! Audio device enumeration and the output stream that drives the mixer.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, FromSample, SampleFormat, SizedSample, StreamConfig};
use rtrb::{Consumer, Producer, RingBuffer};
use serde::{Deserialize, Serialize};

use super::clock::{now_ns, ClockSnapshot, Dll};
use super::mixer::Mixer;
use super::shared::{Command, EngineShared, RtTrack};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AudioConfig {
    /// Host API name (e.g. "WASAPI", "ASIO"); `None` = system default.
    pub host: Option<String>,
    /// Device name; `None` = default output device.
    pub device: Option<String>,
    pub sample_rate: Option<u32>,
    pub buffer_frames: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub name: String,
    pub channels: u16,
    pub sample_rates: Vec<u32>,
    pub buffer_range: Option<(u32, u32)>,
}

pub fn host_names() -> Vec<String> {
    cpal::available_hosts().iter().map(|h| h.name().to_string()).collect()
}

fn host_by_name(name: Option<&str>) -> cpal::Host {
    name.and_then(|n| cpal::available_hosts().into_iter().find(|h| h.name() == n))
        .and_then(|id| cpal::host_from_id(id).ok())
        .unwrap_or_else(cpal::default_host)
}

fn device_name(d: &cpal::Device) -> String {
    d.description().map(|d| d.name().to_string()).unwrap_or_else(|_| d.to_string())
}

const COMMON_RATES: [u32; 6] = [44_100, 48_000, 88_200, 96_000, 176_400, 192_000];

pub fn list_output_devices(host: Option<&str>) -> Vec<DeviceInfo> {
    let host = host_by_name(host);
    let Ok(devices) = host.output_devices() else { return Vec::new() };
    devices
        .filter_map(|d| {
            let configs: Vec<_> = d.supported_output_configs().ok()?.collect();
            let channels = configs.iter().map(|c| c.channels()).max()?;
            let rates: Vec<u32> = COMMON_RATES
                .into_iter()
                .filter(|r| configs.iter().any(|c| (c.min_sample_rate()..=c.max_sample_rate()).contains(r)))
                .collect();
            let buffer_range = configs.iter().find_map(|c| match c.buffer_size() {
                cpal::SupportedBufferSize::Range { min, max } => Some((*min, *max)),
                cpal::SupportedBufferSize::Unknown => None,
            });
            Some(DeviceInfo { name: device_name(&d), channels, sample_rates: rates, buffer_range })
        })
        .collect()
}

/// A running output stream plus the UI-side ends of its queues.
pub struct AudioEngine {
    _stream: cpal::Stream,
    commands: Producer<Command>,
    garbage: Consumer<Vec<RtTrack>>,
    pub host_name: String,
    pub device_name: String,
    pub sample_rate: u32,
    pub channels: u16,
}

impl AudioEngine {
    pub fn start(cfg: &AudioConfig, shared: Arc<EngineShared>, position: i64) -> Result<Self, String> {
        let host = host_by_name(cfg.host.as_deref());
        let device = match &cfg.device {
            Some(name) => host
                .output_devices()
                .map_err(|e| e.to_string())?
                .find(|d| &device_name(d) == name)
                .or_else(|| host.default_output_device()),
            None => host.default_output_device(),
        }
        .ok_or("no audio output device available")?;

        let default = device.default_output_config().map_err(|e| e.to_string())?;
        let mut supported = default;
        if let Some(rate) = cfg.sample_rate.filter(|r| *r != default.sample_rate()) {
            let found = device
                .supported_output_configs()
                .map_err(|e| e.to_string())?
                .filter(|c| c.channels() == default.channels())
                .find_map(|c| c.try_with_sample_rate(rate));
            match found {
                Some(c) => supported = c,
                None => log::warn!("{rate} Hz not supported, using {} Hz", default.sample_rate()),
            }
        }
        let sample_rate = supported.sample_rate();
        let channels = supported.channels();
        let mut config: StreamConfig = supported.config();
        if let Some(frames) = cfg.buffer_frames {
            config.buffer_size = BufferSize::Fixed(frames);
        }

        shared.sample_rate.store(sample_rate, Ordering::Relaxed);
        shared.channels.store(channels as u32, Ordering::Relaxed);
        shared.stream_error.store(false, Ordering::Relaxed);

        let (cmd_tx, cmd_rx) = RingBuffer::new(256);
        let (gb_tx, gb_rx) = RingBuffer::new(64);
        let mixer = Mixer::new(shared.clone(), cmd_rx, gb_tx, sample_rate, position);

        let stream = match supported.sample_format() {
            SampleFormat::F32 => build::<f32>(&device, &config, mixer, shared.clone()),
            SampleFormat::I16 => build::<i16>(&device, &config, mixer, shared.clone()),
            SampleFormat::I32 => build::<i32>(&device, &config, mixer, shared.clone()),
            SampleFormat::U16 => build::<u16>(&device, &config, mixer, shared.clone()),
            other => return Err(format!("unsupported sample format {other}")),
        };
        let stream = match stream {
            Ok(s) => s,
            // Some drivers refuse a fixed buffer size: fall back to default.
            Err(e) if cfg.buffer_frames.is_some() => {
                log::warn!("fixed buffer size rejected ({e}), using driver default");
                let retry = AudioConfig { buffer_frames: None, ..cfg.clone() };
                return Self::start(&retry, shared, position);
            }
            Err(e) => return Err(e),
        };
        stream.play().map_err(|e| e.to_string())?;

        Ok(Self {
            _stream: stream,
            commands: cmd_tx,
            garbage: gb_rx,
            host_name: host.id().name().to_string(),
            device_name: device_name(&device),
            sample_rate,
            channels,
        })
    }

    pub fn send(&mut self, cmd: Command) {
        if self.commands.push(cmd).is_err() {
            log::error!("audio command queue full");
        }
    }

    /// Frees track lists the audio thread has released. Call regularly.
    pub fn collect_garbage(&mut self) {
        while self.garbage.pop().is_ok() {}
    }
}

fn build<T>(
    device: &cpal::Device,
    config: &StreamConfig,
    mut mixer: Mixer,
    shared: Arc<EngineShared>,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32> + Send + 'static,
{
    let channels = config.channels as usize;
    let sample_rate = config.sample_rate;
    let mut dll = Dll::new(sample_rate);
    let mut prev_frames = 0u32;
    let mut scratch: Vec<f32> = vec![0.0; 8192 * channels];
    let mut promoted = false;
    let mut latency_avg = 0.0f64;
    let err_shared = shared.clone();
    let is_f32 = std::any::TypeId::of::<T>() == std::any::TypeId::of::<f32>();

    let data_cb = move |data: &mut [T], info: &cpal::OutputCallbackInfo| {
        let started = Instant::now();
        let now = now_ns();
        if !promoted {
            crate::platform::promote_audio_thread();
            promoted = true;
        }
        let ts = info.timestamp();
        let latency = ts.playback.duration_since(ts.callback).as_secs_f64();
        latency_avg = if latency_avg == 0.0 { latency } else { latency_avg + (latency - latency_avg) * 0.05 };

        let frames = (data.len() / channels) as u32;
        let t0 = dll.update(now as f64 * 1e-9, prev_frames);
        prev_frames = frames;

        if is_f32 {
            // SAFETY: T is f32 (checked through TypeId), same layout.
            let out = unsafe { std::slice::from_raw_parts_mut(data.as_mut_ptr() as *mut f32, data.len()) };
            mixer.process(out, channels);
        } else {
            for chunk in data.chunks_mut(scratch.len()) {
                let s = &mut scratch[..chunk.len()];
                mixer.process(s, channels);
                for (d, &v) in chunk.iter_mut().zip(s.iter()) {
                    *d = T::from_sample(v);
                }
            }
        }

        // `process` applies queued commands first, so the buffer just rendered
        // started at `position - frames` when playing.
        if shared.silent_output.load(Ordering::Relaxed) {
            for d in data.iter_mut() {
                *d = T::EQUILIBRIUM;
            }
        }
        let playing = mixer.is_playing();
        let start = mixer.position() - if playing { frames as i64 } else { 0 };
        shared.clock.publish(ClockSnapshot {
            position: start,
            audible_ns: ((t0 + latency_avg) * 1e9) as u64,
            ns_per_sample: dll.seconds_per_sample() * 1e9,
            playing,
        });

        let period = frames as f64 / sample_rate as f64;
        shared.dsp_load.store((started.elapsed().as_secs_f64() / period) as f32);
        shared.buffer_frames.store(frames, Ordering::Relaxed);
        shared.latency_ns.store((latency_avg * 1e9) as u32, Ordering::Relaxed);
    };
    let err_cb = move |e: cpal::Error| {
        log::error!("audio stream error: {e}");
        err_shared.stream_error.store(true, Ordering::Relaxed);
    };
    device.build_output_stream::<T, _, _>(*config, data_cb, err_cb, None).map_err(|e| e.to_string())
}
