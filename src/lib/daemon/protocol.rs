//! Protocol v1 requests the helper daemon accepts, validated before anything
//! runs. Refusals keep their exact wording: clients and the live checks
//! match on it.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;

/// Longest request line, in bytes.
pub const MAX_REQUEST: usize = 1024 * 1024;
const MAX_ARGUMENTS: usize = 4096;
const MAX_ARGUMENT_BYTES: usize = 256 * 1024;
const MAX_TIMEOUT_MS: u64 = 10 * 60 * 1000;
const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const MAX_DIMENSION: u64 = 8192;

/// Farthest a region's top-left corner may lie from the display's, and its
/// longest side, in display points.
const MAX_POINTS: i32 = 16384;

/// A rectangle of the display in whole points: its top-left corner and size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Region {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Region {
    /// The region, if it lies within the protocol's bounds.
    fn new(x: i64, y: i64, width: i64, height: i64) -> Option<Self> {
        let corner = -i64::from(MAX_POINTS)..=i64::from(MAX_POINTS);
        let side = 1..=i64::from(MAX_POINTS);
        (corner.contains(&x)
            && corner.contains(&y)
            && side.contains(&width)
            && side.contains(&height))
        .then_some(Self {
            x: x as i32,
            y: y as i32,
            width: width as i32,
            height: height as i32,
        })
    }

    /// The smallest whole-point region holding x, y, width, height (floor
    /// the corner, ceil the far edges); None outside the protocol's bounds.
    pub fn from_points(x: f64, y: f64, width: f64, height: f64) -> Option<Self> {
        if ![x, y, width, height].iter().all(|n| n.is_finite()) {
            return None;
        }

        let whole = |n: f64| (n.abs() <= f64::from(MAX_POINTS) * 4.0).then_some(n as i64);
        let (left, top) = (whole(x.floor())?, whole(y.floor())?);
        let (right, bottom) = (whole((x + width).ceil())?, whole((y + height).ceil())?);

        Self::new(left, top, right - left, bottom - top)
    }
}

/// What a screenshot shows instead of the whole display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Crop {
    /// Every on-screen window of this process.
    App {
        pid: i64,
    },
    Region(Region),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Hello,
    Screenshot {
        max_dimension: Option<u32>,
        crop: Option<Crop>,
    },
    Apps,
    Run {
        argv: Vec<String>,
        timeout: Duration,
    },
    Restart,
    /// Log this helper out; `force` ends its session without asking its apps.
    Logout {
        force: bool,
    },
}

/// A refusal in the shape every Atlas failure has, so a reader needs one
/// parser. Nothing here is of unknown outcome: the request never ran.
pub fn failure(code: &str, error: impl Into<String>) -> Value {
    json!({"ok": false, "code": code, "error": error.into(), "uncertain": false})
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
        "logout" => Ok(Request::Logout {
            force: match &request["force"] {
                Value::Null => false,
                value => value
                    .as_bool()
                    .ok_or_else(|| bad("\"force\" must be true or false"))?,
            },
        }),
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

            Ok(Request::Screenshot {
                max_dimension,
                crop: crop(&request)?,
            })
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

/// A screenshot request's `pid` or `region`, at most one of them.
fn crop(request: &Value) -> Result<Option<Crop>, Value> {
    match (&request["pid"], &request["region"]) {
        (Value::Null, Value::Null) => Ok(None),
        (pid, Value::Null) => pid
            .as_i64()
            .filter(|pid| (1..=i64::from(i32::MAX)).contains(pid))
            .map(|pid| Some(Crop::App { pid }))
            .ok_or_else(|| bad("\"pid\" must be a process ID")),
        (Value::Null, region) => {
            let side = |key: &str| region.get(key).and_then(Value::as_i64);
            side("x")
                .zip(side("y"))
                .zip(side("width").zip(side("height")))
                .and_then(|((x, y), (width, height))| Region::new(x, y, width, height))
                .map(|region| Some(Crop::Region(region)))
                .ok_or_else(|| {
                    bad("\"region\" must be whole points: x and y -16384–16384, width and height 1–16384")
                })
        }
        _ => Err(bad("give \"pid\" or \"region\", not both")),
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
            parse(br#"{"op":"logout"}"#),
            Ok(Request::Logout { force: false })
        );
        assert_eq!(
            parse(br#"{"op":"logout","force":true}"#),
            Ok(Request::Logout { force: true })
        );
        assert_eq!(
            refused(r#"{"op":"logout","force":"yes"}"#)["error"],
            "\"force\" must be true or false"
        );
        assert_eq!(
            parse(br#"{"op":"screenshot","maxDimension":320}"#),
            Ok(Request::Screenshot {
                max_dimension: Some(320),
                crop: None
            })
        );
        assert_eq!(
            parse(br#"{"op":"screenshot"}"#),
            Ok(Request::Screenshot {
                max_dimension: None,
                crop: None
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

    /// The wording clients match on.
    #[test]
    fn refusals_keep_their_wording() {
        assert_eq!(
            refused(r#"{"notAnOp":true}"#),
            json!({"ok": false, "code": "bad-request", "error": "request is missing an \"op\" string", "uncertain": false})
        );
        assert_eq!(
            refused(r#"{"op":"nope"}"#),
            json!({"ok": false, "code": "bad-request", "error": "unknown op 'nope'", "uncertain": false})
        );
        assert_eq!(
            refused(r#"{"op":"run","argv":[],"timeoutMs":3000}"#),
            json!({"ok": false, "code": "bad-request", "error": "run requires a non-empty \"argv\" array", "uncertain": false})
        );
    }

    #[test]
    fn screenshots_parse_an_app_or_a_region() {
        let shot = |crop| {
            Ok(Request::Screenshot {
                max_dimension: None,
                crop,
            })
        };

        assert_eq!(
            parse(br#"{"op":"screenshot","pid":4242}"#),
            shot(Some(Crop::App { pid: 4242 }))
        );
        assert_eq!(
            parse(br#"{"op":"screenshot","region":{"x":-100,"y":80,"width":800,"height":600}}"#),
            shot(Some(Crop::Region(Region {
                x: -100,
                y: 80,
                width: 800,
                height: 600
            })))
        );
        assert_eq!(
            parse(br#"{"op":"screenshot","pid":null,"region":null}"#),
            shot(None)
        );
        assert_eq!(
            parse(br#"{"op":"screenshot","pid":7,"maxDimension":400}"#),
            Ok(Request::Screenshot {
                max_dimension: Some(400),
                crop: Some(Crop::App { pid: 7 })
            })
        );
        assert_eq!(
            refused(r#"{"op":"screenshot","pid":1,"region":{"x":0,"y":0,"width":1,"height":1}}"#)["error"],
            "give \"pid\" or \"region\", not both"
        );
        assert_eq!(
            refused(r#"{"op":"screenshot","pid":0}"#)["error"],
            "\"pid\" must be a process ID"
        );
        assert_eq!(
            refused(r#"{"op":"screenshot","region":{"x":0,"y":0,"width":0,"height":1}}"#)["error"],
            "\"region\" must be whole points: x and y -16384–16384, width and height 1–16384"
        );
    }

    #[test]
    fn regions_widen_to_whole_points() {
        assert_eq!(
            Region::from_points(1.5, 2.25, 3.0, 4.0),
            Some(Region {
                x: 1,
                y: 2,
                width: 4,
                height: 5
            })
        );
        assert_eq!(
            Region::from_points(-50.0, -50.0, 200.0, 200.0),
            Some(Region {
                x: -50,
                y: -50,
                width: 200,
                height: 200
            })
        );
        assert_eq!(Region::from_points(0.0, 0.0, 16385.0, 10.0), None);
        assert_eq!(Region::from_points(16385.0, 0.0, 10.0, 10.0), None);
        assert_eq!(Region::from_points(-16384.5, 0.0, 10.0, 10.0), None);
        assert_eq!(Region::from_points(0.0, 0.0, f64::NAN, 10.0), None);
        assert_eq!(Region::from_points(0.0, 0.0, 1e300, 10.0), None);
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
            r#"{"op":"screenshot","pid":0}"#,
            r#"{"op":"screenshot","pid":-1}"#,
            r#"{"op":"screenshot","pid":"5"}"#,
            r#"{"op":"screenshot","pid":2147483648}"#,
            r#"{"op":"screenshot","pid":5.5}"#,
            r#"{"op":"screenshot","region":{"x":0,"y":0,"width":10}}"#,
            r#"{"op":"screenshot","region":{"x":0,"y":0,"width":0,"height":10}}"#,
            r#"{"op":"screenshot","region":{"x":0,"y":0,"width":16385,"height":10}}"#,
            r#"{"op":"screenshot","region":{"x":-16385,"y":0,"width":10,"height":10}}"#,
            r#"{"op":"screenshot","region":{"x":0.5,"y":0,"width":10,"height":10}}"#,
            r#"{"op":"screenshot","region":"0,0,10,10"}"#,
            r#"{"op":"screenshot","region":[0,0,10,10]}"#,
            r#"{"op":"screenshot","pid":5,"region":{"x":0,"y":0,"width":10,"height":10}}"#,
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
