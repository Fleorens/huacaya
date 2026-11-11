//! Blocking HTTP GET against the llama.cpp server. The only place that talks to `ureq`.

use std::time::Duration;

use anyhow::{Context, Result, bail};

/// A reusable client: base URL, optional bearer key, short timeout.
#[derive(Clone)]
pub struct Client {
    agent: ureq::Agent,
    base: String,
    api_key: Option<String>,
}

impl Client {
    pub fn new(base_url: &str, api_key: Option<String>, timeout: Duration) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .http_status_as_error(false)
            .build()
            .into();
        Self { agent, base: base_url.trim_end_matches('/').to_string(), api_key }
    }

    /// GET `path` (starting with `/`), with `query` pairs percent-encoded by `ureq`.
    /// Returns the body of a 2xx answer. Any other status is an error that carries the status
    /// code and the path, never the key.
    pub fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<String> {
        let mut request = self.agent.get(format!("{}{}", self.base, path));
        for (name, value) in query {
            request = request.query(name, value);
        }
        if let Some(key) = &self.api_key {
            request = request.header("Authorization", format!("Bearer {key}"));
        }
        let mut response = request.call().with_context(|| format!("GET {path}"))?;
        let status = response.status();
        if !status.is_success() {
            bail!("GET {path}: HTTP {}", status.as_u16());
        }
        response.body_mut().read_to_string().with_context(|| format!("GET {path}: body"))
    }
}
