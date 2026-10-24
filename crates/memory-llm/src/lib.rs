//! Model providers for extraction (B1-02). Only Ollama so far.

mod ollama;

pub use ollama::{Ollama, OllamaConfig};
