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

    #[test]
    fn context_preserves_the_failure_and_its_recovery_details() {
        let error = context("Read record", Path::new("/records/pending.json"))(
            Error::new(Code::Cleanup, "release was not acknowledged")
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
        assert!(!error.ok);
    }
}
