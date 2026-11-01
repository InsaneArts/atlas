use crate::error::Error;
use std::fmt::Display;

/// Add the failed step without changing its code, certainty or recovery details.
pub(crate) fn context<E: Into<Error>>(step: impl Display) -> impl FnOnce(E) -> Error {
    move |error| {
        let mut error = error.into();
        error.error = format!("{step}: {}", error.error);
        error
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{Code, Holder};

    #[test]
    fn context_preserves_the_failure_and_its_recovery_details() {
        let error = context("End input control")(
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
            "End input control: release was not acknowledged"
        );
        assert!(!error.ok);
    }
}
