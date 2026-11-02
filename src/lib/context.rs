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
mod tests;
