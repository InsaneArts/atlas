//! Comparing the names people and apps give things.

/// Whether two app names name the same app: compared case-insensitively,
/// so `textedit` and `TextEdit` (or `ÉDITEUR` and `Éditeur`) agree.
pub(crate) fn same_text(a: &str, b: &str) -> bool {
    a == b
        || a.chars()
            .flat_map(char::to_lowercase)
            .eq(b.chars().flat_map(char::to_lowercase))
}
