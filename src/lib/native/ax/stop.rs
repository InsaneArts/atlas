//! Why a scan is incomplete, the words a reply names each reason by, and
//! which recorded AX errors say something went wrong.
use super::ABSENT;

/// Whether an `"attribute:code"` error key says something went wrong,
/// rather than that the element simply lacks the attribute.
pub(crate) fn telling(key: &str) -> bool {
    let code = key
        .rsplit_once(':')
        .and_then(|(_, code)| code.parse::<i32>().ok());

    code.is_none_or(|code| !ABSENT.iter().any(|absent| absent.0 == code))
}
