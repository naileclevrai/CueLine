//! Reading cue lists from other tools: CSV / text, Standard MIDI Files
//! (marker and cue-point meta events) and WAV/BWF cue chunks (as written by
//! Reaper, Pro Tools, Wavelab…).

use std::path::Path;

use cueline_core::{FrameRate, Timecode};

/// Where an imported marker sits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MarkerTime {
    /// Seconds from the start of the file (MIDI, WAV) or project (CSV).
    Seconds(f64),
    /// An absolute timecode label (CSV).
    Timecode(Timecode),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImportedMarker {
    pub name: String,
    pub time: MarkerTime,
}

pub const EXTENSIONS: [&str; 6] = ["csv", "txt", "tsv", "mid", "midi", "wav"];

pub fn import(path: &Path) -> Result<Vec<ImportedMarker>, String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let markers = match ext.as_str() {
        "mid" | "midi" => parse_midi(&bytes)?,
        "wav" | "wave" | "bwf" => parse_wav_cues(&bytes)?,
        _ => parse_csv(&String::from_utf8_lossy(&bytes)),
    };
    if markers.is_empty() {
        return Err(format!("no markers found in {}", path.display()));
    }
    Ok(markers)
}

// ----- CSV / text --------------------------------------------------------------

/// `HH:MM:SS:FF` / `HH:MM:SS;FF` (exactly four fields), any frame rate.
fn parse_timecode(s: &str) -> Option<Timecode> {
    if s.split([':', ';']).count() != 4 {
        return None;
    }
    Timecode::parse(s, FrameRate::Fps30)
}

/// `12.5`, `1:02.500` or `01:02:03.25` as seconds.
fn parse_seconds(s: &str) -> Option<f64> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() > 3 || parts.iter().any(|p| p.is_empty()) {
        return None;
    }
    let mut total = 0.0;
    for p in &parts {
        let v: f64 = p.parse().ok()?;
        if v < 0.0 {
            return None;
        }
        total = total * 60.0 + v;
    }
    // A bare integer is ambiguous (often a cue number), unless it is the only value.
    (parts.len() > 1 || s.contains('.')).then_some(total)
}

pub fn parse_csv(text: &str) -> Vec<ImportedMarker> {
    let first = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let delim =
        [('\t', first.matches('\t').count()), (';', first.matches(';').count()), (',', first.matches(',').count())]
            .into_iter()
            .max_by_key(|(_, n)| *n)
            .filter(|(_, n)| *n > 0)
            .map(|(c, _)| c);
    let mut out = Vec::new();
    for line in text.lines() {
        let fields: Vec<String> = match delim {
            Some(d) => line.split(d).map(|f| f.trim().trim_matches('"').trim().to_string()).collect(),
            None => line.split_whitespace().map(str::to_string).collect(),
        };
        let mut time = None;
        let mut time_idx = None;
        for (i, f) in fields.iter().enumerate() {
            if let Some(tc) = parse_timecode(f) {
                time = Some(MarkerTime::Timecode(tc));
            } else if let Some(s) = parse_seconds(f) {
                time = Some(MarkerTime::Seconds(s));
            } else {
                continue;
            }
            time_idx = Some(i);
            break;
        }
        let Some(time) = time else { continue }; // header or blank line
                                                 // Name: the first other field that is not a plain number (cue index).
        let name = fields
            .iter()
            .enumerate()
            .filter(|(i, f)| Some(*i) != time_idx && !f.is_empty())
            .map(|(_, f)| f)
            .find(|f| f.parse::<f64>().is_err() && parse_timecode(f).is_none() && parse_seconds(f).is_none())
            .cloned()
            .unwrap_or_default();
        out.push(ImportedMarker { name, time });
    }
    out
}

// ----- Standard MIDI File ------------------------------------------------------------

pub fn parse_midi(bytes: &[u8]) -> Result<Vec<ImportedMarker>, String> {
    use midly::{MetaMessage, Smf, Timing, TrackEventKind};
    let smf = Smf::parse(bytes).map_err(|e| format!("invalid MIDI file: {e}"))?;

    // Absolute ticks of every tempo change and marker, over all tracks.
    let mut tempos: Vec<(u64, u32)> = Vec::new();
    let mut marks: Vec<(u64, String)> = Vec::new();
    for track in &smf.tracks {
        let mut tick = 0u64;
        for ev in track {
            tick += ev.delta.as_int() as u64;
            if let TrackEventKind::Meta(meta) = ev.kind {
                match meta {
                    MetaMessage::Tempo(t) => tempos.push((tick, t.as_int())),
                    MetaMessage::Marker(text) | MetaMessage::CuePoint(text) => {
                        marks.push((tick, String::from_utf8_lossy(text).trim().to_string()))
                    }
                    _ => {}
                }
            }
        }
    }
    tempos.sort_by_key(|t| t.0);
    marks.sort_by_key(|m| m.0);

    let seconds_at = |tick: u64| -> f64 {
        match smf.header.timing {
            Timing::Timecode(fps, sub) => tick as f64 / (fps.as_f32() as f64 * sub as f64),
            Timing::Metrical(tpb) => {
                let tpb = tpb.as_int() as f64;
                let (mut secs, mut last_tick, mut us_per_beat) = (0.0, 0u64, 500_000.0);
                for &(t, tempo) in tempos.iter().take_while(|(t, _)| *t <= tick) {
                    secs += (t - last_tick) as f64 / tpb * us_per_beat / 1e6;
                    last_tick = t;
                    us_per_beat = tempo as f64;
                }
                secs + (tick - last_tick) as f64 / tpb * us_per_beat / 1e6
            }
        }
    };
    Ok(marks
        .into_iter()
        .map(|(tick, name)| ImportedMarker { name, time: MarkerTime::Seconds(seconds_at(tick)) })
        .collect())
}

// ----- WAV / BWF cue chunks ---------------------------------------------------------

fn u32_le(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

pub fn parse_wav_cues(bytes: &[u8]) -> Result<Vec<ImportedMarker>, String> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a WAV file".into());
    }
    let mut sample_rate = 0u32;
    let mut cues: Vec<(u32, u32)> = Vec::new(); // (id, sample offset)
    let mut labels: Vec<(u32, String)> = Vec::new();
    let mut pos = 12;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32_le(bytes, pos + 4).unwrap_or(0) as usize;
        let body = &bytes[pos + 8..(pos + 8 + size).min(bytes.len())];
        match id {
            b"fmt " => sample_rate = u32_le(body, 4).unwrap_or(0),
            b"cue " => {
                let n = u32_le(body, 0).unwrap_or(0) as usize;
                for i in 0..n {
                    let e = 4 + i * 24;
                    if let (Some(cid), Some(offset)) = (u32_le(body, e), u32_le(body, e + 20)) {
                        cues.push((cid, offset));
                    }
                }
            }
            b"LIST" if body.get(0..4) == Some(b"adtl") => {
                let mut p = 4;
                while p + 8 <= body.len() {
                    let sid = &body[p..p + 4];
                    let ssize = u32_le(body, p + 4).unwrap_or(0) as usize;
                    let sbody = &body[p + 8..(p + 8 + ssize).min(body.len())];
                    if sid == b"labl" && sbody.len() >= 4 {
                        let text = &sbody[4..];
                        let end = text.iter().position(|&c| c == 0).unwrap_or(text.len());
                        labels
                            .push((u32_le(sbody, 0).unwrap_or(0), String::from_utf8_lossy(&text[..end]).into_owned()));
                    }
                    p += 8 + ssize + (ssize & 1);
                }
            }
            _ => {}
        }
        pos += 8 + size + (size & 1);
    }
    if sample_rate == 0 {
        return Err("WAV file has no format chunk".into());
    }
    cues.sort_by_key(|c| c.1);
    Ok(cues
        .into_iter()
        .map(|(cid, offset)| ImportedMarker {
            name: labels.iter().find(|(l, _)| *l == cid).map(|(_, n)| n.clone()).unwrap_or_default(),
            time: MarkerTime::Seconds(offset as f64 / sample_rate as f64),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_with_timecodes_and_header() {
        let text = "#,Name,Start\n1,House lights,10:00:00:00\n2,\"Pyro, big\",10:00:24:12\n";
        let m = parse_csv(text);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].name, "House lights");
        assert_eq!(m[0].time, MarkerTime::Timecode(Timecode::new(10, 0, 0, 0)));
        assert_eq!(m[1].time, MarkerTime::Timecode(Timecode::new(10, 0, 24, 12)));
    }

    #[test]
    fn csv_with_seconds_semicolons_and_tabs() {
        let m = parse_csv("Intro;0.0\nDrop;1:02.500\n");
        assert_eq!(m[1], ImportedMarker { name: "Drop".into(), time: MarkerTime::Seconds(62.5) });
        let m = parse_csv("12.25\tVerse\n");
        assert_eq!(m[0], ImportedMarker { name: "Verse".into(), time: MarkerTime::Seconds(12.25) });
    }

    #[test]
    fn midi_markers_follow_the_tempo_map() {
        use midly::num::{u15, u24, u28};
        use midly::{Format, Header, MetaMessage, Smf, Timing, TrackEvent, TrackEventKind};
        let meta = |delta: u32, m| TrackEvent { delta: u28::new(delta), kind: TrackEventKind::Meta(m) };
        let mut smf = Smf::new(Header::new(Format::SingleTrack, Timing::Metrical(u15::new(480))));
        smf.tracks.push(vec![
            meta(0, MetaMessage::Tempo(u24::new(500_000))),   // 120 bpm
            meta(960, MetaMessage::Marker(b"Verse")),         // 2 beats = 1 s
            meta(0, MetaMessage::Tempo(u24::new(1_000_000))), // 60 bpm
            meta(480, MetaMessage::CuePoint(b"Pyro")),        // + 1 beat = 1 s
            meta(0, MetaMessage::EndOfTrack),
        ]);
        let mut bytes = Vec::new();
        smf.write_std(&mut bytes).unwrap();
        let m = parse_midi(&bytes).unwrap();
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].name, "Verse");
        assert_eq!(m[0].time, MarkerTime::Seconds(1.0));
        assert_eq!(m[1].time, MarkerTime::Seconds(2.0));
    }

    #[test]
    fn wav_cue_and_label_chunks() {
        let mut fmt = Vec::new();
        fmt.extend_from_slice(&1u16.to_le_bytes());
        fmt.extend_from_slice(&1u16.to_le_bytes());
        fmt.extend_from_slice(&48_000u32.to_le_bytes());
        fmt.extend_from_slice(&96_000u32.to_le_bytes());
        fmt.extend_from_slice(&2u16.to_le_bytes());
        fmt.extend_from_slice(&16u16.to_le_bytes());
        let mut cue = 2u32.to_le_bytes().to_vec();
        for (id, offset) in [(1u32, 96_000u32), (2, 24_000)] {
            cue.extend_from_slice(&id.to_le_bytes());
            cue.extend_from_slice(&0u32.to_le_bytes());
            cue.extend_from_slice(b"data");
            cue.extend_from_slice(&0u32.to_le_bytes());
            cue.extend_from_slice(&0u32.to_le_bytes());
            cue.extend_from_slice(&offset.to_le_bytes());
        }
        let mut adtl = b"adtl".to_vec();
        for (id, name) in [(1u32, "Chorus\0"), (2, "Intro\0")] {
            adtl.extend_from_slice(b"labl");
            let len = 4 + name.len() as u32;
            adtl.extend_from_slice(&len.to_le_bytes());
            adtl.extend_from_slice(&id.to_le_bytes());
            adtl.extend_from_slice(name.as_bytes());
            if len % 2 == 1 {
                adtl.push(0);
            }
        }
        let mut body = b"WAVE".to_vec();
        for (id, data) in [(b"fmt ", fmt), (b"cue ", cue), (b"LIST", adtl), (b"data", vec![0u8; 4])] {
            body.extend_from_slice(id);
            body.extend_from_slice(&(data.len() as u32).to_le_bytes());
            body.extend_from_slice(&data);
        }
        let mut wav = b"RIFF".to_vec();
        wav.extend_from_slice(&(body.len() as u32).to_le_bytes());
        wav.extend_from_slice(&body);

        let m = parse_wav_cues(&wav).unwrap();
        assert_eq!(
            m,
            vec![
                ImportedMarker { name: "Intro".into(), time: MarkerTime::Seconds(0.5) },
                ImportedMarker { name: "Chorus".into(), time: MarkerTime::Seconds(2.0) },
            ]
        );
    }
}
