//! Real "Test connection" client: one blocking `GET {base_url}/models` with ureq.
//! Runs only when the user presses the button. Connect timeout 5 s, total 10 s,
//! no redirects, no proxy, https only except this Mac. The key goes only in the
//! single auth header of the provider's auth style. Nothing from the request or the
//! response (URL, headers, body, error text) is logged or returned.

use super::AuthStyle;
use super::custom::is_local_url;
use super::probe::{HttpProbe, ProbeTarget, REASON_NO_FREE_CHECK, REASON_NOT_HTTPS, TestResult};
use std::time::{Duration, Instant};

pub struct UreqProbe {
    connect: Duration,
    total: Duration,
}

impl Default for UreqProbe {
    fn default() -> Self {
        Self::with_timeouts(Duration::from_secs(5), Duration::from_secs(10))
    }
}

impl UreqProbe {
    pub fn with_timeouts(connect: Duration, total: Duration) -> Self {
        Self { connect, total }
    }

    fn agent(&self) -> ureq::Agent {
        ureq::Agent::config_builder()
            .timeout_connect(Some(self.connect))
            .timeout_global(Some(self.total))
            .max_redirects(0)
            .http_status_as_error(false)
            .proxy(None)
            .build()
            .new_agent()
    }
}

/// The one auth header for a style, or None when no key is sent.
/// Returns Err when the style cannot be tested.
fn auth_header(auth: &AuthStyle, key: Option<&str>) -> Result<Option<(String, String)>, ()> {
    let need = |k: Option<&str>| k.map(str::to_owned).ok_or(());
    Ok(match auth {
        AuthStyle::None => None,
        AuthStyle::Unknown => return Err(()),
        AuthStyle::Bearer => Some(("Authorization".into(), format!("Bearer {}", need(key)?))),
        AuthStyle::Token => Some(("Authorization".into(), format!("Token {}", need(key)?))),
        AuthStyle::XApiKey => Some(("x-api-key".into(), need(key)?)),
        AuthStyle::Header(name) => Some((name.to_string(), need(key)?)),
    })
}

/// Maps an HTTP status to a result. Redirects are reported, never followed.
pub fn classify_status(status: u16, latency_ms: u64) -> TestResult {
    match status {
        200..=299 => TestResult::Ok { latency_ms },
        401 | 403 => TestResult::KeyRejected,
        404 => TestResult::NotFound,
        s => TestResult::Other { status: s },
    }
}

fn classify_error(e: &ureq::Error) -> TestResult {
    match e {
        ureq::Error::Timeout(_) => TestResult::Timeout,
        ureq::Error::Io(io) if io.kind() == std::io::ErrorKind::TimedOut => TestResult::Timeout,
        _ => TestResult::Unreachable,
    }
}

impl HttpProbe for UreqProbe {
    fn available(&self) -> bool {
        true
    }

    fn probe(&self, target: &ProbeTarget<'_>, key: Option<&str>) -> TestResult {
        // Re-check the address with the same rules as the form, so a hand-edited
        // settings file cannot point the probe somewhere the form would refuse.
        let Ok(validated) = crate::providers::custom::validate_base_url(target.base_url) else {
            return TestResult::NotAvailable {
                reason: REASON_NOT_HTTPS,
            };
        };
        let base = validated.trim_end_matches('/');
        let https = base.starts_with("https://");
        if !https && !(base.starts_with("http://") && is_local_url(base)) {
            return TestResult::NotAvailable {
                reason: REASON_NOT_HTTPS,
            };
        }
        let Ok(header) = auth_header(target.auth, key) else {
            return TestResult::NotAvailable {
                reason: REASON_NO_FREE_CHECK,
            };
        };
        let url = format!("{base}/models");
        let mut req = self.agent().get(&url).header("Accept", "application/json");
        if let Some((name, value)) = &header {
            req = req.header(name.as_str(), value.as_str());
        }
        let start = Instant::now();
        match req.call() {
            // The body is dropped unread: only the status matters.
            Ok(resp) => classify_status(
                resp.status().as_u16(),
                start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
            ),
            Err(e) => classify_error(&e),
        }
    }
}
