use crate::error::Error;
use std::path::Path;

/// Add the failed step without changing its code, certainty or recovery details.
pub(crate) fn context<'a, E: Into<Error>>(
    step: &'static str,
    path: &'a Path,
) -> impl FnOnce(E) -> Error + 'a {
    move |error| {
        let mut error = error.into();
        error.error = format!("{step} {}: {}", path.display(), error.error);
        error
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{Code, Holder};
    use std::error::Error as _;

    #[test]
    fn writing_and_context_keep_the_original_cause() {
        let path = Path::new("/records/pending.json");
        for errno in [libc::EPERM, libc::EACCES] {
            let original = std::io::Error::from_raw_os_error(errno);
            let kind = original.kind();
            let message = if errno == libc::EPERM {
                "This shell may not write /records/pending.json (operation not permitted), usually because \
                 an agent's sandbox blocks it: run atlas outside the sandbox. Outside one, \
                 check the file's permissions and flags."
                    .to_owned()
            } else {
                format!("{}: {original}", path.display())
            };
            let code = if errno == libc::EPERM {
                Code::Blocked
            } else {
                Code::Io
            };
            let error = context("Save record", path)(
                Error::writing(path, original)
                    .uncertain()
                    .held_by(Holder::Agent)
                    .with("pending", serde_json::json!(true)),
            );
            assert_eq!(
                serde_json::to_value(&error).unwrap(),
                serde_json::json!({"ok": false, "code": code,
                    "error": format!("Save record {}: {message}", path.display()),
                    "uncertain": true, "holder": "agent", "pending": true})
            );
            let source = error
                .source()
                .unwrap()
                .downcast_ref::<std::io::Error>()
                .unwrap();
            assert_eq!(source.kind(), kind);
            assert_eq!(source.raw_os_error(), Some(errno));
        }

        let error = context("Read record", path)(std::io::Error::from_raw_os_error(libc::ENOENT));
        assert_eq!(error.code, Code::Io);
        assert_eq!(
            error
                .source()
                .unwrap()
                .downcast_ref::<std::io::Error>()
                .unwrap()
                .raw_os_error(),
            Some(libc::ENOENT)
        );
    }

    #[test]
    fn context_preserves_the_failure_and_its_recovery_details() {
        let error = context("Read record", Path::new("/records/pending.json"))(
            Error::new(Code::Cleanup, "release was not acknowledged")
                .with_source(std::io::Error::other("private-context-source-canary-29b76"))
                .uncertain()
                .held_by(Holder::Agent)
                .with("pending", serde_json::json!(true)),
        );

        assert_eq!(error.code, Code::Cleanup);
        assert!(error.uncertain);
        assert_eq!(error.holder, Some(Holder::Agent));
        assert_eq!(error.details["pending"], true);
        assert_eq!(
            error.error,
            "Read record /records/pending.json: release was not acknowledged"
        );
        assert_eq!(serde_json::to_value(&error).unwrap()["ok"], false);
        assert_eq!(
            error.source().unwrap().to_string(),
            "private-context-source-canary-29b76"
        );
        for diagnostic in [
            error.to_string(),
            format!("{error:?}"),
            serde_json::to_string(&error).unwrap(),
        ] {
            assert!(!diagnostic.contains("private-context-source-canary-29b76"));
        }
    }
}
