//! Timecode primitives shared by the CueLine application.
//!
//! Everything in this crate is allocation-free and safe to call from a
//! real-time audio thread.

pub mod ltc;
pub mod rate;
pub mod timecode;

pub use rate::FrameRate;
pub use timecode::Timecode;
