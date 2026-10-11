//! Protocol v1 requests the helper daemon accepts, validated before anything
//! runs. Replies and error wording match the capture daemon Atlas used
//! before, so existing clients are unaffected.
use serde_json::{Value, json};
use std::time::Duration;

/// Longest request line, in bytes.
pub const MAX_REQUEST: usize = 1024 * 1024;
const MAX_ARGUMENTS: usize = 4096;
const MAX_ARGUMENT_BYTES: usize = 256 * 1024;
const MAX_TIMEOUT_MS: u64 = 10 * 60 * 1000;
const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const MAX_DIMENSION: u64 = 8192;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Hello,
    Screenshot {
        max_dimension: Option<u32>,
    },
    Apps,
    Run {
        argv: Vec<String>,
        timeout: Duration,
    },
    Restart,
}

/// An error reply.
pub fn failure(code: &str, error: impl Into<String>) -> Value {
    json!({"ok": false, "code": code, "error": error.into()})
}

fn bad(error: impl Into<String>) -> Value {
    failure("bad-request", error)
}

/// Parse one request line, or the reply that refuses it.
pub fn parse(line: &[u8]) -> Result<Request, Value> {
    if line.len() > MAX_REQUEST {
        return Err(bad("request exceeds 1 MiB"));
    }
    let request: Value =
        serde_json::from_slice(line).map_err(|_| bad("request is not a JSON object"))?;
    if !request.is_object() {
        return Err(bad("request is not a JSON object"));
    }
    let op = request["op"]
        .as_str()
        .ok_or_else(|| bad("request is missing an \"op\" string"))?;
    match op {
        "hello" => Ok(Request::Hello),
        "apps" => Ok(Request::Apps),
        "restart" => Ok(Request::Restart),
        "screenshot" => {
            let max_dimension = match &request["maxDimension"] {
                Value::Null => None,
                value => Some(
                    value
                        .as_u64()
                        .filter(|n| (1..=MAX_DIMENSION).contains(n))
                        .and_then(|n| u32::try_from(n).ok())
                        .ok_or_else(|| bad("\"maxDimension\" must be 1–8192"))?,
                ),
            };
            Ok(Request::Screenshot { max_dimension })
        }
        "run" => {
            let argv = request["argv"]
                .as_array()
                .filter(|argv| !argv.is_empty())
                .ok_or_else(|| bad("run requires a non-empty \"argv\" array"))?;
            if argv.len() > MAX_ARGUMENTS {
                return Err(bad("run accepts at most 4096 arguments"));
            }
            let argv = argv
                .iter()
                .map(|argument| {
                    argument
                        .as_str()
                        .filter(|text| text.len() <= MAX_ARGUMENT_BYTES && !text.contains('\0'))
                        .map(str::to_owned)
                        .ok_or_else(|| bad("run arguments must be strings without NUL"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            if argv[0].is_empty() {
                return Err(bad("run requires a non-empty \"argv\" array"));
            }
            let timeout = match &request["timeoutMs"] {
                Value::Null => DEFAULT_TIMEOUT_MS,
                value => value
                    .as_u64()
                    .filter(|n| (1..=MAX_TIMEOUT_MS).contains(n))
                    .ok_or_else(|| bad("\"timeoutMs\" must be 1–600000"))?,
            };
            Ok(Request::Run {
                argv,
                timeout: Duration::from_millis(timeout),
            })
        }
        other => Err(bad(format!("unknown op '{other}'"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused(line: &str) -> Value {
        parse(line.as_bytes()).unwrap_err()
    }

    #[test]
    fn requests_parse_into_operations() {
        assert_eq!(parse(br#"{"op":"hello"}"#), Ok(Request::Hello));
        assert_eq!(parse(br#"{"op":"apps"}"#), Ok(Request::Apps));
        assert_eq!(parse(br#"{"op":"restart"}"#), Ok(Request::Restart));
        assert_eq!(
            parse(br#"{"op":"screenshot","maxDimension":320}"#),
            Ok(Request::Screenshot {
                max_dimension: Some(320)
            })
        );
        assert_eq!(
            parse(br#"{"op":"screenshot"}"#),
            Ok(Request::Screenshot {
                max_dimension: None
            })
        );
        assert_eq!(
            parse(br#"{"op":"run","argv":["/bin/echo","hi"],"timeoutMs":3000}"#),
            Ok(Request::Run {
                argv: vec!["/bin/echo".into(), "hi".into()],
                timeout: Duration::from_millis(3000)
            })
        );
        let Ok(Request::Run { timeout, .. }) = parse(br#"{"op":"run","argv":["true"]}"#) else {
            panic!("run without a timeout");
        };
        assert_eq!(timeout, Duration::from_secs(30));
    }

    /// The wording the previous capture daemon used, recorded from it.
    #[test]
    fn refusals_match_the_recorded_daemon() {
        assert_eq!(
            refused(r#"{"notAnOp":true}"#),
            json!({"ok": false, "code": "bad-request", "error": "request is missing an \"op\" string"})
        );
        assert_eq!(
            refused(r#"{"op":"nope"}"#),
            json!({"ok": false, "code": "bad-request", "error": "unknown op 'nope'"})
        );
        assert_eq!(
            refused(r#"{"op":"run","argv":[],"timeoutMs":3000}"#),
            json!({"ok": false, "code": "bad-request", "error": "run requires a non-empty \"argv\" array"})
        );
    }

    #[test]
    fn every_bound_is_enforced() {
        for line in [
            "not json",
            "[1]",
            r#"{"op":7}"#,
            r#"{"op":"run","argv":"/bin/echo"}"#,
            r#"{"op":"run","argv":[""]}"#,
            r#"{"op":"run","argv":[7]}"#,
            "{\"op\":\"run\",\"argv\":[\"a\\u0000b\"]}",
            r#"{"op":"run","argv":["true"],"timeoutMs":0}"#,
            r#"{"op":"run","argv":["true"],"timeoutMs":600001}"#,
            r#"{"op":"run","argv":["true"],"timeoutMs":"5"}"#,
            r#"{"op":"screenshot","maxDimension":0}"#,
            r#"{"op":"screenshot","maxDimension":8193}"#,
            r#"{"op":"screenshot","maxDimension":1.5}"#,
        ] {
            assert_eq!(refused(line)["code"], "bad-request", "{line}");
        }
        let many = format!(
            r#"{{"op":"run","argv":[{}]}}"#,
            vec!["\"x\""; MAX_ARGUMENTS + 1].join(",")
        );
        assert_eq!(refused(&many)["code"], "bad-request");
        let huge = vec![b' '; MAX_REQUEST + 1];
        assert_eq!(parse(&huge).unwrap_err()["error"], "request exceeds 1 MiB");
    }
}
