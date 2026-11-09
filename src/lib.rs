//! Red Light: a synthesized drum machine inspired by classic early-80s analog
//! drum machines.
//!
//! The library is the whole engine and is UI-free, so it can be tested and
//! rendered offline. `main.rs` adds the window and the audio device.

pub mod clock;
pub mod demos;
pub mod dsp;
pub mod engine;
pub mod fx;
pub mod pattern;
pub mod storage;
pub mod voices;
pub mod wav;
