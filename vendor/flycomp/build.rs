use std::process::Command;

fn main() {
    // Capture git commit hash
    let git_hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());

    // Build datetime (UTC, ISO 8601), computed without a date crate.
    // Honor SOURCE_DATE_EPOCH (https://reproducible-builds.org/specs/source-date-epoch/)
    // when set so builds are reproducible; fall back to the wall clock otherwise.
    let build_secs = match std::env::var("SOURCE_DATE_EPOCH") {
        Ok(epoch) => epoch.parse::<i64>().ok(),
        Err(_) => std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .and_then(|d| i64::try_from(d.as_secs()).ok()),
    };
    let build_time = build_secs.map_or_else(|| "unknown".to_string(), utc_iso8601);

    println!("cargo:rustc-env=GIT_HASH={git_hash}");
    println!("cargo:rustc-env=BUILD_TIME={build_time}");

    // Re-run when the reproducible-build timestamp changes.
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
    // Re-run when HEAD changes
    println!("cargo:rerun-if-changed=.git/HEAD");
    if let Ok(head) = std::fs::read_to_string(".git/HEAD")
        && let Some(refpath) = head.strip_prefix("ref: ")
    {
        println!("cargo:rerun-if-changed=.git/{}", refpath.trim());
    }
}

/// Formats Unix seconds as `YYYY-MM-DDTHH:MM:SSZ` (proleptic Gregorian, UTC),
/// using Howard Hinnant's days-to-civil algorithm.
fn utc_iso8601(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        rem % 3_600 / 60,
        rem % 60
    )
}
