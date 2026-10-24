//! Ollama `/api/generate` with a JSON-schema `format`. The server is local by default; a
//! non-loopback URL is refused unless `allow_remote` is set, so nothing leaves the machine by
//! accident.

use std::time::Duration;

use memory_core::extract::{Candidate, ExtractionInput, MemoryExtractor};
use memory_core::{Error, Result};
use serde::Deserialize;
use serde_json::{Value, json};

pub const DEFAULT_URL: &str = "http://127.0.0.1:11434";
pub const DEFAULT_MODEL: &str = "qwen3:8b";
/// A local 8B model on a long session is slow; extraction runs in the background.
const DEFAULT_TIMEOUT_SECS: u64 = 120;
/// Per event, so one huge tool summary cannot crowd out the rest of the session.
const MAX_EVENT_CHARS: usize = 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OllamaConfig {
    pub base_url: String,
    pub model: String,
    pub timeout: Duration,
    /// Explicit opt-in to a server that is not on this machine.
    pub allow_remote: bool,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_URL.into(),
            model: DEFAULT_MODEL.into(),
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            allow_remote: false,
        }
    }
}

pub struct Ollama {
    config: OllamaConfig,
    agent: ureq::Agent,
}

fn is_loopback(url: &str) -> bool {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split('/').next().unwrap_or("");
    let host = match authority.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or(""),
        None => authority.split(':').next().unwrap_or(""),
    };
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

impl Ollama {
    pub fn new(config: OllamaConfig) -> Result<Self> {
        // shortcut: http only (no TLS feature built in), upgrade when a remote https provider is needed.
        if !config.base_url.starts_with("http://") {
            return Err(Error::InvalidArgument(
                "the Ollama URL must start with http://".into(),
            ));
        }
        if !config.allow_remote && !is_loopback(&config.base_url) {
            return Err(Error::InvalidArgument(format!(
                "{} is not a local address; remote model servers need allow_remote",
                config.base_url
            )));
        }
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(config.timeout))
            .http_status_as_error(false)
            .build()
            .into();
        Ok(Self { config, agent })
    }

    pub fn config(&self) -> &OllamaConfig {
        &self.config
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.config.base_url.trim_end_matches('/'))
    }

    /// Names of the models the server has pulled; also the reachability probe for `doctor`.
    pub fn models(&self) -> Result<Vec<String>> {
        let mut resp = self
            .agent
            .get(&self.url("/api/tags"))
            .call()
            .map_err(unavailable)?;
        if !resp.status().is_success() {
            return Err(Error::Unavailable(format!("HTTP {}", resp.status())));
        }
        let v: Value = resp.body_mut().read_json().map_err(unavailable)?;
        Ok(v["models"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| m["name"].as_str().map(String::from))
            .collect())
    }

    /// One non-streaming generation constrained to `schema`; returns the parsed JSON.
    pub fn generate_json(&self, prompt: &str, schema: &Value) -> Result<Value> {
        let body = json!({
            "model": self.config.model,
            "prompt": prompt,
            "stream": false,
            "format": schema,
            "think": false,
            "options": {"temperature": 0},
        });
        let mut resp = self
            .agent
            .post(&self.url("/api/generate"))
            .send_json(&body)
            .map_err(unavailable)?;
        if !resp.status().is_success() {
            // Never echo the body: the server may repeat the prompt.
            return Err(Error::Unavailable(format!(
                "HTTP {} (is model {} pulled?)",
                resp.status(),
                self.config.model
            )));
        }
        let v: Value = resp.body_mut().read_json().map_err(unavailable)?;
        let text = v["response"]
            .as_str()
            .ok_or_else(|| Error::InvalidArgument("model reply has no response field".into()))?;
        serde_json::from_str(text)
            .map_err(|_| Error::InvalidArgument("model output is not valid JSON".into()))
    }
}

fn unavailable(e: ureq::Error) -> Error {
    Error::Unavailable(e.to_string())
}

// shortcut: minimal prompt and schema so the provider is demonstrable; B1-03 replaces them
// with the restricted, evidence-bearing versions.
fn prompt(input: &ExtractionInput) -> String {
    let mut p = String::from(
        "You extract durable engineering knowledge from a coding-agent session. \
Return only lasting decisions, constraints, procedures, gotchas or facts worth remembering \
in future sessions; ignore routine activity. Return an empty list if there is none. \
Never include secrets.\n\nSession events:\n",
    );
    for e in &input.events {
        let text: String = e
            .text
            .as_deref()
            .unwrap_or("")
            .chars()
            .take(MAX_EVENT_CHARS)
            .collect();
        p.push_str(&format!("- [{}] {text}\n", e.kind.as_str()));
    }
    p
}

fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {"candidates": {"type": "array", "items": {
            "type": "object",
            "properties": {
                "kind": {"enum": ["fact", "decision", "procedure", "constraint", "gotcha"]},
                "title": {"type": "string"},
                "content": {"type": "string"},
            },
            "required": ["kind", "title", "content"],
        }}},
        "required": ["candidates"],
    })
}

#[derive(Deserialize)]
struct Output {
    candidates: Vec<Candidate>,
}

impl MemoryExtractor for Ollama {
    fn name(&self) -> String {
        format!("ollama:{}", self.config.model)
    }

    fn extract(&self, input: &ExtractionInput) -> Result<Vec<Candidate>> {
        let out = self.generate_json(&prompt(input), &schema())?;
        let out: Output = serde_json::from_value(out)
            .map_err(|_| Error::InvalidArgument("model output does not match the schema".into()))?;
        Ok(out.candidates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_detection() {
        for u in [
            "http://127.0.0.1:11434",
            "http://localhost",
            "http://[::1]:1/x",
        ] {
            assert!(is_loopback(u), "{u}");
        }
        for u in [
            "http://10.0.0.5:11434",
            "http://localhost.evil.com",
            "http://127.0.0.1.evil.com",
        ] {
            assert!(!is_loopback(u), "{u}");
        }
    }
}
