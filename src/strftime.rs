//! strftime-style time formatting for prompts (`\t`, `\D{format}`, ...) and
//! `flyline time --format`, matching what bash's C `strftime` prints.

/// Formats `now` with a strftime-style `format` (bash's `\D{format}`, `\t`, ...).
///
/// A specifier that jiff does not know is an error rather than a panic; the
/// format string is then shown as written, so a typo in PS1 is visible instead
/// of breaking the prompt (chrono panicked here, and the caught panic ended
/// the shell with EOF).
pub fn format(format: &str, now: &jiff::ZonedDateTime) -> String {
    jiff::fmt::strtime::format(c_locale_format(format).as_ref(), now).unwrap_or_else(|e| {
        log::debug!("Invalid time format {format:?}: {e}");
        format.to_string()
    })
}

/// Rewrites the locale-dependent specifiers to what C `strftime` prints in the
/// C/POSIX locale, which is what bash shows: jiff renders `%c` and `%x` in
/// CLDR's root locale (`1970 M01 1, Thu 00:00:00`) and `%r` without the
/// leading zero. The `E` and `O` modifiers select alternative representations,
/// which the C locale does not have, so they are dropped.
pub fn c_locale_format(format: &str) -> std::borrow::Cow<'_, str> {
    if !format.contains('%') {
        return std::borrow::Cow::Borrowed(format);
    }
    let mut out = String::with_capacity(format.len() + 16);
    let mut chars = format.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        if matches!(chars.peek(), Some('E' | 'O')) {
            chars.next();
        }
        match chars.next() {
            Some('c') => out.push_str("%a %b %e %H:%M:%S %Y"),
            Some('x') => out.push_str("%m/%d/%y"),
            Some('X') => out.push_str("%H:%M:%S"),
            Some('r') => out.push_str("%I:%M:%S %p"),
            Some(other) => {
                out.push('%');
                out.push(other);
            }
            None => out.push('%'),
        }
    }
    std::borrow::Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bash expands `\D{format}` with C `strftime`; flyline formats it with jiff.
    /// Compare the two in UTC and the C locale for every specifier a prompt is
    /// likely to use, at times that exercise padding, 12-hour clocks, day of year
    /// and week numbers.
    #[test]
    fn time_formats_match_c_strftime() {
        fn c_strftime(format: &str, secs: i64) -> String {
            let format = std::ffi::CString::new(format).unwrap();
            let mut tm: libc::tm = unsafe { std::mem::zeroed() };
            let t: libc::time_t = secs as libc::time_t;
            unsafe { libc::gmtime_r(&t, &mut tm) };
            let mut buf = [0u8; 256];
            let n =
                unsafe { libc::strftime(buf.as_mut_ptr().cast(), buf.len(), format.as_ptr(), &tm) };
            String::from_utf8_lossy(&buf[..n]).into_owned()
        }

        let formats = [
            "%H:%M:%S",
            "%I:%M:%S",
            "%I:%M %p",
            "%H:%M",
            "%T",
            "%R",
            "%r",
            "%D",
            "%F",
            "%Y-%m-%d",
            "%y",
            "%C",
            "%a %b %d",
            "%A, %B %e",
            "%h",
            "%j",
            "%u",
            "%w",
            "%U",
            "%W",
            "%V",
            "%G",
            "%g",
            "%k",
            "%l",
            "%n",
            "%t",
            "%%",
            "%-d",
            "%-m",
            "%-H",
            "%-I",
            "%_H",
            "%_d",
            "%0e",
            "%c",
            "%x",
            "%X",
            "%e",
            "%p",
            "%Ec",
            "%Ox",
            "%OH",
            "%%c",
            "at %r!",
        ];
        let times = [
            0_i64,
            951_782_400,               // 2000-02-29 00:00:00, leap day, midnight
            1_009_843_199,             // 2001-12-31 23:59:59, ISO week 1 of 2002
            1_795_000_000,             // 2026-11-18 ...
            1_704_067_200,             // 2024-01-01, a Monday
            1_735_617_600,             // 2024-12-31 04:00, ISO week 1 of 2025
            1_609_459_199,             // 2020-12-31 23:59:59, ISO week 53
            1_700_000_000 + 12 * 3600, // a PM hour
        ];
        let mut mismatches = Vec::new();
        for secs in times {
            let now = jiff::Timestamp::from_second(secs)
                .unwrap()
                .to_zoned_datetime(jiff::tz::TimeZone::UTC);
            for fmt in formats {
                let ours = format(fmt, &now);
                let theirs = c_strftime(fmt, secs);
                if ours != theirs {
                    mismatches.push(std::format!(
                        "{fmt:?} at {secs}: jiff {ours:?}, C {theirs:?}"
                    ));
                }
            }
        }
        assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
    }

    /// `%s`, `%z` and `%Z` are left out of the C comparison: macOS libc applies the
    /// local zone to them even for a UTC `struct tm`.
    #[test]
    fn epoch_and_zone_specifiers_follow_the_timestamp() {
        let now = jiff::Timestamp::from_second(1_795_000_000)
            .unwrap()
            .to_zoned_datetime(jiff::tz::TimeZone::UTC);
        assert_eq!(format("%s", &now), "1795000000");
        assert_eq!(format("%z", &now), "+0000");
        assert_eq!(format("%Z", &now), "UTC");
    }

    #[test]
    fn invalid_time_format_is_shown_as_written() {
        let now = jiff::Timestamp::UNIX_EPOCH.to_zoned_datetime(jiff::tz::TimeZone::UTC);
        // A lone trailing `%` is invalid; the rewrite of `%c` must not leak into the fallback.
        assert_eq!(format("%c %", &now), "%c %");
    }
}
