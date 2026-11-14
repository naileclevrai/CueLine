//! OSC over UDP. Only immediate bundles are executed; scheduled bundles are rejected.
use serde::{Deserialize, Serialize};
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OscConfig {
    pub enabled: bool,
    pub listen: String,
    pub feedback: bool,
    pub destination: String,
}
impl Default for OscConfig {
    fn default() -> Self {
        Self { enabled: false, listen: "127.0.0.1:9000".into(), feedback: false, destination: "127.0.0.1:9001".into() }
    }
}
#[derive(Debug, PartialEq)]
pub enum Action {
    Play,
    Pause,
    Stop,
    Toggle,
    Seek(f64),
    Cue(usize),
    Next,
    Previous,
    AddCue,
}

fn string(data: &[u8], offset: &mut usize) -> Option<String> {
    let tail = data.get(*offset..)?;
    let end = tail.iter().position(|b| *b == 0)?;
    let value = std::str::from_utf8(&tail[..end]).ok()?.to_owned();
    let len = (end + 4) & !3;
    if tail.get(end..len)?.iter().any(|b| *b != 0) {
        return None;
    }
    *offset += len;
    Some(value)
}
fn word(data: &[u8], offset: &mut usize) -> Option<[u8; 4]> {
    let bytes = data.get(*offset..*offset + 4)?.try_into().ok()?;
    *offset += 4;
    Some(bytes)
}
fn message(data: &[u8]) -> Option<Action> {
    let mut p = 0;
    let address = string(data, &mut p)?;
    let tags = string(data, &mut p)?;
    // Touch controllers commonly send 1 on press and 0 on release. Only a
    // positive press invokes a button; zero never triggers a second action.
    let button = match tags.as_str() {
        "," | ",T" => true,
        ",i" if address != "/cueline/cue" && address != "/cueline/seek" => i32::from_be_bytes(word(data, &mut p)?) > 0,
        ",f" if address != "/cueline/seek" => {
            let value = f32::from_bits(u32::from_be_bytes(word(data, &mut p)?));
            value.is_finite() && value > 0.0
        }
        _ => false,
    };
    if button && p == data.len() {
        return match address.as_str() {
            "/cueline/play" => Some(Action::Play),
            "/cueline/pause" => Some(Action::Pause),
            "/cueline/stop" => Some(Action::Stop),
            "/cueline/toggle" => Some(Action::Toggle),
            "/cueline/cue/next" => Some(Action::Next),
            "/cueline/cue/previous" => Some(Action::Previous),
            "/cueline/cue/add" => Some(Action::AddCue),
            _ => None,
        };
    }
    let action = match (address.as_str(), tags.as_str()) {
        ("/cueline/seek", ",f") => Action::Seek(f32::from_bits(u32::from_be_bytes(word(data, &mut p)?)) as f64),
        ("/cueline/seek", ",i") => Action::Seek(i32::from_be_bytes(word(data, &mut p)?) as f64),
        ("/cueline/cue", ",i") => {
            let n = i32::from_be_bytes(word(data, &mut p)?);
            Action::Cue(usize::try_from(n.checked_sub(1)?).ok()?)
        }
        _ => return None,
    };
    if p != data.len() {
        return None;
    }
    if let Action::Seek(v) = action {
        if !v.is_finite() || !(0.0..=86_400.0).contains(&v) {
            return None;
        }
    }
    Some(action)
}
fn packet(data: &[u8], depth: usize, actions: &mut Vec<Action>) {
    if depth > 4 || actions.len() >= 64 {
        return;
    }
    if data.starts_with(b"#bundle\0") {
        // OSC immediate timetag is 1. Never execute a future bundle early.
        if data.get(8..16) != Some(&[0, 0, 0, 0, 0, 0, 0, 1]) {
            return;
        }
        let mut p = 16;
        while p < data.len() && actions.len() < 64 {
            let Some(n) = word(data, &mut p).map(u32::from_be_bytes) else { break };
            let Some(end) = p.checked_add(n as usize).filter(|end| *end <= data.len()) else { break };
            packet(&data[p..end], depth + 1, actions);
            p = end;
        }
    } else if let Some(action) = message(data) {
        actions.push(action);
    }
}
fn osc_string(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(s.as_bytes());
    out.push(0);
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}
fn encode(address: &str, tag: &str, bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    osc_string(&mut out, address);
    osc_string(&mut out, tag);
    out.extend_from_slice(bytes);
    out
}
pub struct Osc {
    socket: Option<UdpSocket>,
    destination: Option<SocketAddr>,
    pub error: Option<String>,
    last_feedback: Instant,
}
impl Osc {
    pub fn new(config: &OscConfig) -> Self {
        let mut osc = Self { socket: None, destination: None, error: None, last_feedback: Instant::now() };
        if let Err(e) = osc.configure(config) {
            osc.error = Some(e);
        }
        osc
    }
    pub fn configure(&mut self, config: &OscConfig) -> Result<(), String> {
        if !config.enabled {
            self.socket = None;
            self.destination = None;
            self.error = None;
            return Ok(());
        }
        let listen: SocketAddr =
            config.listen.parse().map_err(|_| "Invalid listen address (use IP:port)".to_string())?;
        let destination: Option<SocketAddr> = if config.feedback {
            Some(config.destination.parse().map_err(|_| "Invalid feedback address (use IP:port)".to_string())?)
        } else {
            None
        };
        if listen.port() == 0 || destination.is_some_and(|d| d.port() == 0 || d.is_ipv4() != listen.is_ipv4()) {
            return Err("Use non-zero ports and matching IP versions".into());
        }
        // Preserve the old listener if a new address cannot be bound.
        let same = self.socket.as_ref().and_then(|s| s.local_addr().ok()) == Some(listen);
        if !same {
            let socket = UdpSocket::bind(listen).map_err(|e| format!("Cannot listen on {listen}: {e}"))?;
            socket.set_nonblocking(true).map_err(|e| e.to_string())?;
            self.socket = Some(socket);
        }
        self.destination = destination;
        self.error = None;
        Ok(())
    }
    pub fn poll(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        let Some(socket) = &self.socket else { return actions };
        let mut buf = [0u8; 65535];
        for _ in 0..32 {
            match socket.recv_from(&mut buf) {
                Ok((n, _)) => packet(&buf[..n], 0, &mut actions),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => {
                    self.error = Some(format!("OSC receive: {e}"));
                    break;
                }
            }
            if actions.len() >= 64 {
                break;
            }
        }
        actions
    }
    pub fn feedback(&mut self, playing: bool, seconds: f64, timecode: &str, cue: Option<usize>) {
        let (Some(socket), Some(destination)) = (&self.socket, self.destination) else { return };
        if self.last_feedback.elapsed() < Duration::from_millis(100) {
            return;
        }
        self.last_feedback = Instant::now();
        let mut tc = Vec::new();
        osc_string(&mut tc, timecode);
        for msg in [
            encode("/cueline/state/playing", ",i", &(playing as i32).to_be_bytes()),
            encode("/cueline/state/position", ",f", &(seconds as f32).to_bits().to_be_bytes()),
            encode("/cueline/state/timecode", ",s", &tc),
            encode("/cueline/state/cue", ",i", &(cue.map_or(0, |n| n as i32 + 1)).to_be_bytes()),
        ] {
            if let Err(e) = socket.send_to(&msg, destination) {
                self.error = Some(format!("OSC send: {e}"));
                break;
            }
        }
    }
    pub fn active(&self) -> bool {
        self.socket.is_some()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decode_commands_and_reject_invalid() {
        assert_eq!(message(&encode("/cueline/play", ",", &[])), Some(Action::Play));
        assert_eq!(message(&encode("/cueline/seek", ",f", &12.5f32.to_bits().to_be_bytes())), Some(Action::Seek(12.5)));
        assert_eq!(message(&encode("/cueline/cue", ",i", &2i32.to_be_bytes())), Some(Action::Cue(1)));
        assert_eq!(message(&encode("/cueline/cue", ",i", &0i32.to_be_bytes())), None);
        assert_eq!(message(&encode("/cueline/seek", ",f", &f32::NAN.to_bits().to_be_bytes())), None);
        assert_eq!(message(&encode("/cueline/play", ",i", &1i32.to_be_bytes())), Some(Action::Play));
        assert_eq!(message(&encode("/cueline/play", ",f", &0f32.to_bits().to_be_bytes())), None);
    }
    #[test]
    fn bundle_immediate_only() {
        let msg = encode("/cueline/play", ",", &[]);
        let mut b = b"#bundle\0\0\0\0\0\0\0\0\x01".to_vec();
        b.extend_from_slice(&(msg.len() as u32).to_be_bytes());
        b.extend_from_slice(&msg);
        let mut a = vec![];
        packet(&b, 0, &mut a);
        assert_eq!(a, [Action::Play]);
        b[15] = 2;
        a.clear();
        packet(&b, 0, &mut a);
        assert!(a.is_empty());
    }
    #[test]
    fn udp_receive_and_feedback() {
        let listener = UdpSocket::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let remote = UdpSocket::bind("127.0.0.1:0").unwrap();
        remote.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
        let mut osc = Osc {
            socket: Some(listener),
            destination: Some(remote.local_addr().unwrap()),
            error: None,
            last_feedback: Instant::now() - Duration::from_secs(1),
        };
        remote.send_to(&encode("/cueline/play", ",", &[]), address).unwrap();
        let mut actions = Vec::new();
        for _ in 0..100 {
            actions.extend(osc.poll());
            if !actions.is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(actions, [Action::Play]);
        osc.feedback(true, 12.0, "01:00:12:00", Some(1));
        let mut data = [0; 512];
        let (n, _) = remote.recv_from(&mut data).unwrap();
        assert_eq!(&data[..n], &encode("/cueline/state/playing", ",i", &1i32.to_be_bytes()));
    }
}
