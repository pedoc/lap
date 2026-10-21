//! Preserve useful ffprobe diagnostics in debug and release builds.
const DISPLAY_LIMIT: usize = 8192;

pub(crate) fn failure_message(code: Option<i32>, stderr: &[u8]) -> String {
    let raw = String::from_utf8_lossy(stderr);
    let mut chars = raw.trim().chars();
    let mut detail: String = chars.by_ref().take(DISPLAY_LIMIT).collect();
    if detail.is_empty() {
        detail.push_str("No diagnostic output was returned. Check that the file is readable and FFprobe is available");
    } else if chars.next().is_some() {
        detail.push_str(" … (diagnostic truncated)");
    }
    format!(
        "ffprobe exited with code {}: {}",
        code.map(|v| v.to_string())
            .unwrap_or_else(|| "unknown".into()),
        detail
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_missing_movie_index_is_preserved_in_the_error() {
        let text = failure_message(
            Some(1),
            b"[mov] moov atom not found\r\nclip.mov: Invalid data found when processing input\r\n",
        );
        assert!(text.starts_with("ffprobe exited with code 1:"));
        assert!(text.contains("moov atom not found"));
        assert!(text.contains("Invalid data found"));
        assert!(!text.ends_with('\n'));
    }
    #[test]
    fn missing_stderr_or_exit_code_still_produces_an_actionable_message() {
        let text = failure_message(None, b" \r\n");
        assert!(text.contains("code unknown"));
        assert!(text.contains("No diagnostic output"));
    }
    #[test]
    fn invalid_utf8_and_long_unicode_output_do_not_panic_or_flood_the_ui() {
        assert!(failure_message(Some(1), &[0xff, 0xfe]).contains('\u{fffd}'));
        let text = failure_message(Some(1), "诊断".repeat(DISPLAY_LIMIT).as_bytes());
        assert!(text.contains("diagnostic truncated"));
        assert!(text.chars().count() < DISPLAY_LIMIT + 100);
    }
}
