pub mod firered;
pub mod vad;

pub use vad::{
    detect_phrases_from_samples, detect_phrases_from_wav, detect_phrases_from_wav_verbose,
    SliceReport, VadEngine,
};
