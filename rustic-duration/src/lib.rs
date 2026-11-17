use std::time::Duration;

/// If we are above 1000ms we want to print the duration as seconds
/// instead of ms, to avoid cognitive overhead.
pub fn beautify_duration(elapsed_duration: Duration) -> String {
    if elapsed_duration.as_millis() < 1000 {
        format!("{}ms", elapsed_duration.as_millis())
    } else {
        format!("{}s", elapsed_duration.as_secs())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_duration_is_printed_in_ms() {
        assert_eq!(beautify_duration(Duration::ZERO), "0ms");
    }

    #[test]
    fn below_one_second_is_printed_in_ms() {
        assert_eq!(beautify_duration(Duration::from_millis(999)), "999ms");
    }

    #[test]
    fn exactly_one_second_is_printed_in_seconds() {
        assert_eq!(beautify_duration(Duration::from_millis(1000)), "1s");
    }

    #[test]
    fn above_one_second_is_truncated_to_whole_seconds() {
        assert_eq!(beautify_duration(Duration::from_millis(1999)), "1s");
        assert_eq!(beautify_duration(Duration::from_secs(125)), "125s");
    }
}
