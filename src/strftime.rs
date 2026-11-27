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

/// The current time in the zone bash would use: its `TZ` variable when set (an
/// IANA name such as `Europe/Paris`, or a POSIX rule such as `EST5EDT`), else the
/// system zone.
///
/// Bash re-reads `TZ` as soon as it changes (`export TZ=UTC` then `\t`), while
/// jiff's system zone is cached for minutes and ignores a `TZ` set in the shell.
pub fn local_now() -> jiff::ZonedDateTime {
    jiff::Timestamp::now().to_zoned_datetime(local_time_zone())
}

fn local_time_zone() -> jiff::tz::TimeZone {
    static CACHE: std::sync::Mutex<Option<(String, jiff::tz::TimeZone)>> =
        std::sync::Mutex::new(None);

    let Some(tz) = crate::shell::backend()
        .env_var("TZ")
        .filter(|tz| !tz.is_empty())
    else {
        return jiff::tz::TimeZone::system();
    };
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((cached, zone)) = cache.as_ref()
        && *cached == tz
    {
        return zone.clone();
    }
    let name = tz.strip_prefix(':').unwrap_or(&tz);
    let zone = jiff::tz::TimeZone::get(name)
        .or_else(|_| jiff::tz::TimeZone::posix(name))
        .unwrap_or_else(|e| {
            log::debug!("Unknown TZ {tz:?}: {e}; using UTC, as the C library does");
            jiff::tz::TimeZone::UTC
        });
    *cache = Some((tz, zone.clone()));
    zone
}

/// Rewrites the locale-dependent specifiers to what C `strftime` prints in the
/// C/POSIX locale, which is what bash shows: jiff renders `%c` and `%x` in
/// CLDR's root locale (`1970 M01 1, Thu 00:00:00`) and `%r` without the
/// leading zero.
///
/// A specifier may carry flags and a width (`%^c`, `%-d`, `%10Y`). For the
/// composite ones the case flags (`^`, `#`) are passed on to their parts, as
/// glibc does. The `E` and `O` modifiers select alternative representations,
/// which the C locale does not have; they are dropped where POSIX allows them
/// and left in place elsewhere, so jiff rejects them (`%E%`, `%Ea`).
pub fn c_locale_format(format: &str) -> std::borrow::Cow<'_, str> {
    if !format.contains('%') {
        return std::borrow::Cow::Borrowed(format);
    }
    let mut out = String::with_capacity(format.len() + 16);
    let mut rest = format;
    while let Some(pos) = rest.find('%') {
        out.push_str(&rest[..pos]);
        rest = &rest[pos + 1..];

        let flags_len = rest
            .find(|c: char| !matches!(c, '-' | '_' | '0' | '^' | '#'))
            .unwrap_or(rest.len());
        let (flags, after_flags) = rest.split_at(flags_len);
        let width_len = after_flags
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(after_flags.len());
        let (width, after_width) = after_flags.split_at(width_len);

        let mut spec_chars = after_width.chars();
        let (modifier, conversion) = match (spec_chars.next(), spec_chars.next()) {
            (Some(m @ ('E' | 'O')), Some(c)) => (Some(m), Some(c)),
            (Some(c), _) => (None, Some(c)),
            (None, _) => (None, None),
        };
        let Some(conversion) = conversion else {
            // A lone `%` (or `%-` ...) at the end: let jiff report it.
            out.push('%');
            out.push_str(rest);
            rest = "";
            break;
        };
        let modifier_allowed = match modifier {
            Some('E') => "cCxXyY".contains(conversion),
            Some('O') => "deHImMSuUVwWy".contains(conversion),
            _ => true,
        };
        let consumed = flags.len() + width.len() + modifier.map_or(0, char::len_utf8);
        rest = &rest[consumed + conversion.len_utf8()..];

        if !modifier_allowed {
            out.push('%');
            out.push_str(flags);
            out.push_str(width);
            out.extend(modifier);
            out.push(conversion);
            continue;
        }
        let case: String = flags.chars().filter(|c| matches!(c, '^' | '#')).collect();
        let composite = match conversion {
            'c' => Some(format!("%{case}a %{case}b %e %H:%M:%S %Y")),
            'x' => Some("%m/%d/%y".to_string()),
            'X' => Some("%H:%M:%S".to_string()),
            'r' => Some(format!("%I:%M:%S %{case}p")),
            _ => None,
        };
        match composite {
            Some(expansion) => out.push_str(&expansion),
            None => {
                out.push('%');
                out.push_str(flags);
                out.push_str(width);
                out.push(conversion);
            }
        }
    }
    out.push_str(rest);
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
        // E and O only modify the specifiers POSIX allows them on.
        assert_eq!(format("%E%", &now), "%E%");
        assert_eq!(format("%Ea", &now), "%Ea");
        assert_eq!(format("%OH:%EY", &now), "00:1970");
    }

    #[test]
    fn flags_reach_the_parts_of_composite_specifiers() {
        let now = jiff::Timestamp::UNIX_EPOCH.to_zoned_datetime(jiff::tz::TimeZone::UTC);
        // glibc: `%^c` upper-cases the day and month names of the C-locale `%c`.
        assert_eq!(format("%^c", &now), "THU JAN  1 00:00:00 1970");
        assert_eq!(format("%-d/%_m %10Y", &now), "1/ 1 0000001970");
        assert_eq!(format("100%%", &now), "100%");
    }

    #[test]
    fn local_time_follows_the_shells_tz() {
        let backend = crate::shell::backend();
        let _ = backend.export_env_var("TZ", "UTC");
        assert_eq!(local_now().offset().seconds(), 0);
        let _ = backend.export_env_var("TZ", "EST5");
        assert_eq!(local_now().offset().seconds(), -5 * 3600);
        let _ = backend.export_env_var("TZ", ":Asia/Tokyo");
        assert_eq!(local_now().offset().seconds(), 9 * 3600);
        let _ = backend.unset_env_var("TZ");
    }
}
