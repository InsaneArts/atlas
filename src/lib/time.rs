//! Times as Atlas records and sends them.
use std::time::Duration;

/// `duration` in whole milliseconds, or the most a `u64` holds for one
/// longer than that.
pub(crate) fn ms(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::ms;
    use std::time::Duration;

    #[test]
    fn milliseconds_are_whole_and_saturate() {
        assert_eq!(ms(Duration::from_micros(2_999)), 2);
        assert_eq!(ms(Duration::MAX), u64::MAX);
    }
}
