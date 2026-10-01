//! Timecode primitives shared by the CueLine application.
//!
//! Everything in this crate is allocation-free and safe to call from a
//! real-time audio thread.

pub mod rate;

pub use rate::FrameRate;
