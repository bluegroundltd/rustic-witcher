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
