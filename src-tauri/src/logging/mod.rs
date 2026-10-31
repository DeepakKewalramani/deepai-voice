//! Redacted logging (slice 0.5).
//!
//! Logs carry durations, counts, lengths and status only. They never carry API
//! keys, audio samples, transcripts or cleaned text. Every formatted line also
//! passes through [`redact_secrets`] before it is written, as a second guard.

pub mod events;
mod redact;

pub use redact::redact_secrets;

use std::io::{self, Write};
use std::sync::OnceLock;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::MakeWriter;

/// Default filter when `RUST_LOG` is not set.
pub const DEFAULT_FILTER: &str = "info";

static INIT: OnceLock<Result<(), String>> = OnceLock::new();

/// Installs the global tracing subscriber. Uses `RUST_LOG`, falling back to
/// [`DEFAULT_FILTER`]. Safe to call more than once; later calls return the
/// first result.
pub fn init_logging() -> Result<(), String> {
    INIT.get_or_init(|| {
        let filter =
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));
        let installed = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(RedactingStderr)
            .try_init();
        // Another subscriber may already be set (e.g. by a host or test harness).
        // That is not an error for us: logging still works through it.
        let _ = installed;
        Ok(())
    })
    .clone()
}

/// Writer that redacts secrets from each formatted log line before stderr.
#[derive(Clone, Copy)]
struct RedactingStderr;

impl<'a> MakeWriter<'a> for RedactingStderr {
    type Writer = RedactingWriter;
    fn make_writer(&'a self) -> Self::Writer {
        RedactingWriter
    }
}

struct RedactingWriter;

impl Write for RedactingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let line = String::from_utf8_lossy(buf);
        io::stderr().write_all(redact_secrets(&line).as_bytes())?;
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        io::stderr().flush()
    }
}
