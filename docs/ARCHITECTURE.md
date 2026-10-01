# CueLine architecture

CueLine is a Cargo workspace with two crates:

| Crate | Role |
| --- | --- |
| `cueline-core` | Pure, allocation-free timecode maths: frame rates, drop-frame labels, LTC encoder/decoder, MTC messages. No I/O, fully unit-tested. |
| `cueline` | The application: audio engine, MTC output, project files, export and the egui user interface. |

## Threads

```
 UI thread (egui, ~60 fps)          audio thread (cpal callback)        MTC thread (time-critical)
 ─────────────────────────          ────────────────────────────        ──────────────────────────
 edits project, sends Commands ──►  rtrb ring buffer ──► Mixer          reads SharedClock (seqlock)
 reads meters/clock (atomics)  ◄──  atomics, SharedClock ◄── publishes  sleeps / spins until each
 frees old track lists         ◄──  "garbage" ring buffer               quarter-frame is due, sends
```

* **Audio thread.** It runs the `Mixer` inside the cpal callback and never allocates, locks or
  blocks. Commands (`Play`, `Pause`, `Seek`, `SetTracks`, `SetTimecode`) come in through a
  wait-free `rtrb` queue. A replaced track list is sent back on a second queue, so the UI thread
  frees it rather than the real-time thread. Per-track parameters (gain, pan, mute, solo, offset)
  are atomics the UI writes directly. On Windows the thread registers with MMCSS as *Pro Audio*.
* **MTC thread.** Owns the MIDI connection and runs at `THREAD_PRIORITY_TIME_CRITICAL`. The
  process requests 1 ms timer resolution.
* **Workers.** File decoding, peak analysis, resampling and WAV export run on short-lived
  threads and report back through channels.

## LTC generation

LTC is biphase-mark encoded: the signal toggles at each bit boundary, and a `1` adds a toggle in
the middle of the bit. The polarity-correction bit keeps the number of `1`s per frame even, so
**every frame starts at the same level**. `LtcGenerator::render` uses that property to compute
each sample from its timeline position alone:

```
frame     = floor(sample × fps_num / (sample_rate × fps_den))   // exact integer maths
half_bit  = (remainder × 160) / (sample_rate × fps_den)
level     = table_for(frame)[half_bit]
```

No running state means no drift, at any rate, including 29.97 fps at 48 kHz where a frame is
1601.6 samples long. A seek, a loop or a buffer-size change cannot produce a malformed frame. A
one-pole filter shapes the edges to the ~25 µs rise time that SMPTE recommends. The tests encode
and then decode three seconds at every frame rate and at 44.1/48/96 kHz, and check that each
frame boundary falls within two samples of its theoretical position.

Because LTC is written into the same output buffer as the program audio, the two cannot drift
apart.

## The audio clock and MTC

MIDI is not part of the audio stream, so MTC needs to know *when* a given sample will be heard:

1. Each callback records `now` (QueryPerformanceCounter) and the output latency reported by the
   driver.
2. A second-order **delay-locked loop** (F. Adriaensen, *Using a DLL to filter time*) filters the
   callback times into a smooth estimate of each buffer's start time and of the real sample
   period. With ±1 ms of simulated callback jitter, the residual error stays under 150 µs.
3. The audio thread publishes `(timeline position, audible time, ns per sample, playing)` through
   a single-writer **seqlock** (`SharedClock`).
4. The MTC thread turns each quarter-frame index into a wall-clock deadline. It sleeps until
   about 1.5 ms before the deadline, re-reads the clock (so a seek or stop takes effect
   immediately), then spins until the deadline. A test checks that every message goes out within
   1 ms of its ideal time. Typical results are 10–250 µs.

Following the MIDI specification, piece 0 is always aligned on an even timecode frame, and every
locate sends a full-frame SysEx so that receivers jump straight to the new position.

## Project files

`.cueline` files are pretty-printed JSON (`project.rs`). Media paths are stored relative to the
project file whenever possible, every field has a default (old files keep loading), and saves go
through a temporary file and a rename, so a crash cannot corrupt a project.
