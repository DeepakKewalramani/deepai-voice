//! The "Test connection" seam. `HttpProbe` hides the HTTP client so the hub logic
//! is tested with fakes; the real client is `http::UreqProbe`. A result carries no
//! key, URL, header or response body.

use super::{AuthStyle, Capability};
use serde::Serialize;

/// What to check. The key is passed separately and only ever goes in the auth header.
#[derive(Debug, Clone)]
pub struct ProbeTarget<'a> {
    pub capability: Capability,
    pub base_url: &'a str,
    pub auth: &'a AuthStyle,
    pub model: &'a str,
}

/// Outcome of a connection test, as the UI sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum TestResult {
    /// The test cannot run for this provider; `reason` is a fixed sentence.
    NotAvailable {
        reason: &'static str,
    },
    Ok {
        #[serde(rename = "latencyMs")]
        latency_ms: u64,
    },
    /// HTTP 401 or 403.
    KeyRejected,
    /// HTTP 404: usually a wrong base URL.
    NotFound,
    /// DNS, connection or TLS failure.
    Unreachable,
    Timeout,
    /// Any other HTTP status, including redirects (never followed).
    Other {
        status: u16,
    },
}

pub const REASON_NO_CLIENT: &str = "No HTTP client is built in.";
pub const REASON_NO_FREE_CHECK: &str =
    "No free check is documented for this provider yet, so it is not tested.";
pub const REASON_NO_KEY: &str = "No key is saved for this provider.";
pub const REASON_NOT_HTTPS: &str = "Only https addresses, or http on this Mac, can be tested.";

pub trait HttpProbe {
    /// False when no HTTP client is built in.
    fn available(&self) -> bool;
    /// Runs one free metadata request (`GET {base_url}/models`).
    fn probe(&self, target: &ProbeTarget<'_>, key: Option<&str>) -> TestResult;
}

/// Stand-in used when no HTTP client is available.
pub struct NoHttpClient;

impl HttpProbe for NoHttpClient {
    fn available(&self) -> bool {
        false
    }
    fn probe(&self, _target: &ProbeTarget<'_>, _key: Option<&str>) -> TestResult {
        TestResult::NotAvailable {
            reason: REASON_NO_CLIENT,
        }
    }
}
