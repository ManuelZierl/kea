//! Opt-in structured completion from a cooperating receiver. Endpoint addresses
//! come ONLY from the user's local configuration, never from terminal output.
use super::completion::Candidate;
use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    time::{Duration, Instant},
};

const MAX_CONFIG: usize = 64 * 1024;
const MAX_REPLY: usize = 1024 * 1024;
const MAX_CANDIDATES: usize = 200;
const TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    address: SocketAddr,
    #[serde(default)]
    token: Option<String>,
}

#[derive(Default)]
pub struct Providers(BTreeMap<String, Endpoint>);

impl Providers {
    pub fn load() -> (Self, Option<String>) {
        let Some(path) = std::env::var_os("KEA_COMPLETION_PROVIDERS") else {
            return (Self::default(), None);
        };
        match Self::load_file(&std::path::PathBuf::from(path)) {
            Ok(providers) => (providers, None),
            Err(error) => (
                Self::default(),
                Some(format!("Completion providers unavailable: {error}")),
            ),
        }
    }
    fn load_file(path: &std::path::Path) -> Result<Self> {
        let mut bytes = Vec::new();
        File::open(path)?
            .take((MAX_CONFIG + 1) as u64)
            .read_to_end(&mut bytes)?;
        anyhow::ensure!(
            bytes.len() <= MAX_CONFIG,
            "provider configuration exceeds 64 KiB"
        );
        let endpoints: BTreeMap<String, Endpoint> =
            serde_json::from_slice(&bytes).context("invalid provider configuration")?;
        for endpoint in endpoints.values() {
            anyhow::ensure!(
                endpoint.address.ip().is_loopback() && endpoint.address.port() != 0,
                "completion endpoints must use numeric loopback addresses and nonzero ports"
            );
        }
        Ok(Self(endpoints))
    }
    pub fn for_context(&self, context: &str) -> Option<Endpoint> {
        self.0.get(context).cloned()
    }
}

#[derive(Serialize)]
struct Request<'a> {
    protocol: u8,
    request_id: u64,
    context: &'a str,
    text: &'a str,
    /// UTF-8 byte offset, not a character count or PowerShell's UTF-16 offset.
    cursor: usize,
    token: Option<&'a str>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Response {
    protocol: u8,
    request_id: u64,
    context: String,
    candidates: Vec<Replacement>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Replacement {
    label: String,
    replacement: String,
    start: usize,
    end: usize,
}

impl Endpoint {
    /// No PTY input, shell evaluation, process launch, or automatic retry.
    /// The cooperating provider is responsible for its own native completion
    /// semantics. A fresh subprocess is not claimed to equal a live shell.
    pub fn complete(
        &self,
        request_id: u64,
        context: &str,
        text: &str,
        cursor: usize,
    ) -> Result<Vec<Candidate>> {
        anyhow::ensure!(
            text.len() <= 64 * 1024 && cursor <= text.len() && text.is_char_boundary(cursor),
            "invalid completion draft"
        );
        let deadline = Instant::now() + TIMEOUT;
        let mut stream = TcpStream::connect_timeout(&self.address, TIMEOUT)
            .context("cannot connect to completion provider")?;
        stream.set_write_timeout(Some(remaining(deadline)?))?;
        let mut request = serde_json::to_vec(&Request {
            protocol: 1,
            request_id,
            context,
            text,
            cursor,
            token: self.token.as_deref(),
        })?;
        request.push(b'\n');
        stream
            .write_all(&request)
            .context("cannot send completion request")?;
        let mut response = Vec::new();
        let mut buffer = [0; 4096];
        loop {
            stream.set_read_timeout(Some(remaining(deadline)?))?;
            let count = stream
                .read(&mut buffer)
                .context("completion provider did not respond in time")?;
            anyhow::ensure!(count != 0, "incomplete completion response");
            if let Some(end) = buffer[..count].iter().position(|b| *b == b'\n') {
                anyhow::ensure!(
                    response.len() + end <= MAX_REPLY,
                    "completion response exceeds limit"
                );
                response.extend_from_slice(&buffer[..end]);
                break;
            }
            anyhow::ensure!(
                response.len() + count <= MAX_REPLY,
                "completion response exceeds limit"
            );
            response.extend_from_slice(&buffer[..count]);
        }
        validate_response(&response, request_id, context, text)
    }
}

fn remaining(deadline: Instant) -> Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|d| !d.is_zero())
        .ok_or_else(|| anyhow::anyhow!("completion provider deadline exceeded"))
}

fn validate_response(
    bytes: &[u8],
    request_id: u64,
    context: &str,
    text: &str,
) -> Result<Vec<Candidate>> {
    let response: Response =
        serde_json::from_slice(bytes).context("invalid completion response")?;
    anyhow::ensure!(
        response.protocol == 1 && response.request_id == request_id && response.context == context,
        "stale or mismatched completion response"
    );
    anyhow::ensure!(
        response.candidates.len() <= MAX_CANDIDATES,
        "too many completion candidates"
    );
    response
        .candidates
        .into_iter()
        .map(|item| {
            anyhow::ensure!(
                item.start <= item.end
                    && item.end <= text.len()
                    && text.is_char_boundary(item.start)
                    && text.is_char_boundary(item.end),
                "invalid completion replacement range"
            );
            anyhow::ensure!(
                item.label.len() <= 1024
                    && item.replacement.len() <= 64 * 1024
                    && !item.label.chars().any(char::is_control)
                    && !item.replacement.chars().any(char::is_control),
                "invalid completion text"
            );
            Ok(Candidate {
                label: item.label,
                replacement: item.replacement,
                range: item.start..item.end,
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "../../tests/unit/editor/provider.rs"]
mod tests;
