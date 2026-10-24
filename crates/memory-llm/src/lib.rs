//! Model providers for extraction (B1-02) and the prompt and validation they share (B1-03).

mod ollama;
pub mod prompt;
pub mod validate;

pub use ollama::{Ollama, OllamaConfig};
