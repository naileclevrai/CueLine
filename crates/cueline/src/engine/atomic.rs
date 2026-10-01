//! Lock-free scalar cells shared between the UI and real-time threads.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct AtomicF32(AtomicU32);

impl AtomicF32 {
    pub fn new(v: f32) -> Self {
        Self(AtomicU32::new(v.to_bits()))
    }
    pub fn load(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }
    pub fn store(&self, v: f32) {
        self.0.store(v.to_bits(), Ordering::Relaxed)
    }
    /// Raises the stored value to `v` if larger (peak metering).
    pub fn fetch_max(&self, v: f32) {
        let mut cur = self.0.load(Ordering::Relaxed);
        while v > f32::from_bits(cur) {
            match self.0.compare_exchange_weak(cur, v.to_bits(), Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(c) => cur = c,
            }
        }
    }
    /// Reads and resets to zero (peak-hold consumption by the UI).
    pub fn take(&self) -> f32 {
        f32::from_bits(self.0.swap(0, Ordering::Relaxed))
    }
}

#[derive(Debug, Default)]
pub struct AtomicF64(AtomicU64);

impl AtomicF64 {
    pub fn new(v: f64) -> Self {
        Self(AtomicU64::new(v.to_bits()))
    }
    pub fn load(&self) -> f64 {
        f64::from_bits(self.0.load(Ordering::Relaxed))
    }
    pub fn store(&self, v: f64) {
        self.0.store(v.to_bits(), Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peak_hold() {
        let a = AtomicF32::new(0.0);
        a.fetch_max(0.5);
        a.fetch_max(0.2);
        assert_eq!(a.take(), 0.5);
        assert_eq!(a.load(), 0.0);
    }
}
