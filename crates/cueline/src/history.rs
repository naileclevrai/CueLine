//! Undo/redo by snapshots. Audio buffers are shared through `Arc`, so a
//! snapshot only copies track definitions and markers.

use crate::app::Track;
use crate::project::Marker;

const LIMIT: usize = 200;

#[derive(Clone)]
pub struct Snapshot {
    pub tracks: Vec<Track>,
    pub markers: Vec<Marker>,
}

#[derive(Default)]
pub struct History {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl History {
    pub fn record(&mut self, s: Snapshot) {
        self.undo.push(s);
        if self.undo.len() > LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn undo(&mut self, current: Snapshot) -> Option<Snapshot> {
        let prev = self.undo.pop()?;
        self.redo.push(current);
        Some(prev)
    }

    pub fn redo(&mut self, current: Snapshot) -> Option<Snapshot> {
        let next = self.redo.pop()?;
        self.undo.push(current);
        Some(next)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(n: usize) -> Snapshot {
        Snapshot { tracks: Vec::new(), markers: vec![Marker::default(); n] }
    }

    #[test]
    fn undo_redo_cycle() {
        let mut h = History::default();
        h.record(snap(0));
        h.record(snap(1));
        let s = h.undo(snap(2)).unwrap();
        assert_eq!(s.markers.len(), 1);
        let s = h.undo(s).unwrap();
        assert_eq!(s.markers.len(), 0);
        assert!(h.undo(s.clone()).is_none());
        let s = h.redo(s).unwrap();
        assert_eq!(s.markers.len(), 1);
        h.record(snap(5));
        assert!(!h.can_redo());
    }
}
