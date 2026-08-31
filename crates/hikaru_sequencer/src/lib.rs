// crates/hikaru_sequencer/src/lib.rs

pub mod clip;
pub mod quantizer;
pub mod matrix;

// Re-exportamos para que sea fácil de usar
pub use clip::{Clip, ClipState, TriggerMode};
pub use quantizer::{QuantizationEngine, QuantizationGrid};
pub use matrix::TrackMatrix;