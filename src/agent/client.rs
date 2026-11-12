//! Talks to one agent over HTTP, reusing `http::Client` (bearer token, short timeout, 2xx body). The
//! http layer already names the path and status in its error, so only a parse failure gets a context.

use std::time::Duration;

use anyhow::{Context, Result};

use crate::agent::proto::LogPage;
use crate::collect::sampler::SystemSnapshot;
use crate::http::Client;

/// Talks to one agent; the token goes in `Authorization: Bearer` (`http::Client` already does that).
pub struct AgentClient {
    http: Client,
}

impl AgentClient {
    /// `url` like `http://192.168.1.203:9099`; 3 s timeout.
    pub fn new(url: &str, token: Option<String>) -> Self {
        Self { http: Client::new(url, token, Duration::from_secs(3)) }
    }

    /// `GET /v1/system`, parsed.
    pub fn system(&self) -> Result<SystemSnapshot> {
        let body = self.http.get("/v1/system", &[])?;
        serde_json::from_str(&body).context("parse /v1/system")
    }

    /// `GET /v1/log?since=<since>`, parsed.
    pub fn log(&self, since: u64) -> Result<LogPage> {
        let since_str = since.to_string();
        let body = self.http.get("/v1/log", &[("since", since_str.as_str())])?;
        serde_json::from_str(&body).context("parse /v1/log")
    }

    /// `GET /v1/health`: the agent's version.
    pub fn version(&self) -> Result<String> {
        let body = self.http.get("/v1/health", &[])?;
        #[derive(serde::Deserialize)]
        struct Health {
            version: String,
        }
        serde_json::from_str(&body).context("parse /v1/health").map(|Health { version }| version)
    }
}
