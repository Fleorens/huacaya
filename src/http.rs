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
        self.send(request, path)
    }

    /// GET `http://127.0.0.1:<port><path>` on the loaded instance, with the same key and timeout as `get`.
    /// Same error rules: the error carries the port and path only, never the key.
    pub fn get_local(&self, port: u16, path: &str) -> Result<String> {
        self.send(self.agent.get(local_url(port, path)), &format!(":{port}{path}"))
    }

    /// Adds the bearer key, sends, and returns the body of a 2xx answer. `label` names the request in
    /// errors; it never contains the key.
    fn send(
        &self,
        mut request: ureq::RequestBuilder<ureq::typestate::WithoutBody>,
        label: &str,
    ) -> Result<String> {
        if let Some(key) = &self.api_key {
            request = request.header("Authorization", format!("Bearer {key}"));
        }
        let mut response = request.call().with_context(|| format!("GET {label}"))?;
        let status = response.status();
        if !status.is_success() {
            bail!("GET {label}: HTTP {}", status.as_u16());
        }
        response.body_mut().read_to_string().with_context(|| format!("GET {label}: body"))
    }
}

/// The URL of `path` on the local instance at `port`; exactly one `/` between port and path.
fn local_url(port: u16, path: &str) -> String {
    format!("http://127.0.0.1:{port}/{}", path.trim_start_matches('/'))
}

/// `true` when the URL's host is `127.0.0.1`, `localhost` or `[::1]` (scheme and port ignored, ASCII
/// case-insensitive): the instances' local ports are then reachable directly.
pub fn is_loopback_url(url: &str) -> bool {
    let Some(host) = host_of(url) else {
        return false;
    };
    host.eq_ignore_ascii_case("127.0.0.1")
        || host.eq_ignore_ascii_case("localhost")
        || host.eq_ignore_ascii_case("[::1]")
}

/// The host of a URL: the text between `://` and the next `:` or `/`, brackets kept for an IPv6 literal.
fn host_of(url: &str) -> Option<String> {
    let rest = url.split("://").nth(1)?;
    if let Some(after_open) = rest.strip_prefix('[') {
        // IPv6 literal: `[...]` up to the closing `]`, brackets kept.
        return Some(format!("[{}]", after_open.split(']').next()?));
    }
    rest.split([':', '/']).next().map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::local_url;

    #[test]
    fn local_url_keeps_the_slash() {
        assert_eq!(local_url(35335, "/metrics"), "http://127.0.0.1:35335/metrics");
        assert_eq!(local_url(35335, "slots"), "http://127.0.0.1:35335/slots");
    }
}
