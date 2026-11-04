//! The real HTTP transport: blocking `ureq` with rustls.

use std::io::Read;
use std::time::Duration;

use serde_json::Value;

use crate::{HttpError, Transport};

/// The largest response body read (a long streamed reply is a few MB).
const MAX_STREAM_BYTES: u64 = 256 * 1024 * 1024;
/// The most read of an error response.
const MAX_ERROR_BYTES: u64 = 64 * 1024;

/// [`Transport`] over `ureq` 3 (rustls, no async runtime).
///
/// `timeout` bounds the wait for the response headers and each read of the stream after them,
/// not the whole reply: a long turn streams for minutes, with `ping` events in between.
#[derive(Debug, Clone, Copy, Default)]
pub struct UreqTransport;

impl Transport for UreqTransport {
    fn post(
        &self,
        url: &str,
        headers: &[(String, String)],
        body: &[u8],
        timeout: Duration,
    ) -> Result<Box<dyn Read + Send>, HttpError> {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .timeout_connect(Some(Duration::from_secs(30)))
            .timeout_recv_response(Some(timeout))
            .timeout_per_read(Some(timeout))
            .user_agent(format!("OpenStitch/{}", env!("CARGO_PKG_VERSION")))
            .build();
        let agent = ureq::Agent::new_with_config(config);
        let mut req = agent.post(url);
        for (k, v) in headers {
            req = req.header(k.as_str(), v.as_str());
        }
        let res = req.send(body).map_err(|e| HttpError {
            status: None,
            message: match e {
                ureq::Error::Timeout(_) => {
                    "The Claude API did not answer in time (timed out).".to_string()
                }
                e => format!("Could not reach the Claude API: {e}"),
            },
            retry_after: None,
        })?;
        let status = res.status().as_u16();
        if (200..300).contains(&status) {
            let reader = res
                .into_body()
                .into_with_config()
                .limit(MAX_STREAM_BYTES)
                .reader();
            return Ok(Box::new(reader));
        }
        let retry_after = res
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|s| s.is_finite() && *s >= 0.0)
            .map(Duration::from_secs_f64);
        let text = res
            .into_body()
            .with_config()
            .limit(MAX_ERROR_BYTES)
            .read_to_string()
            .unwrap_or_default();
        Err(HttpError {
            status: Some(status),
            message: error_message(&text),
            retry_after,
        })
    }
}

/// The `error.message` of an API error body (`{"type":"error","error":{"type","message"}}`),
/// else the body's first line.
pub(crate) fn error_message(body: &str) -> String {
    if let Ok(v) = serde_json::from_str::<Value>(body) {
        if let Some(m) = v.pointer("/error/message").and_then(Value::as_str) {
            return m.to_string();
        }
        if let Some(m) = v.get("message").and_then(Value::as_str) {
            return m.to_string();
        }
    }
    body.lines()
        .next()
        .unwrap_or("")
        .chars()
        .take(300)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_bodies() {
        assert_eq!(
            error_message(
                r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#
            ),
            "invalid x-api-key"
        );
        assert_eq!(error_message("Bad gateway\n<html>"), "Bad gateway");
        assert_eq!(error_message(""), "");
    }
}
