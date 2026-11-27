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

    // Capture rustc version
    let rustc_version = Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=RUSTC_VERSION={rustc_version}");

    // Capture build target triple
    let build_target = std::env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=BUILD_TARGET={build_target}");

    if build_target.contains("android") {
        println!("cargo:rustc-link-arg=-Wl,-rpath=/data/data/com.termux/files/usr/lib");
        println!("cargo:rustc-link-arg=-Wl,--no-as-needed");
        println!("cargo:rustc-link-arg=-lreadline");
        println!("cargo:rustc-link-arg=-Wl,--as-needed");
    }

    // Never unmap the library on `enable -d flyline`: termina's SIGWINCH handler
    // and Rust's thread-local destructors stay registered after bash dlcloses
    // it, and Bionic (Termux) really unmaps it, so the next call into it
    // crashed bash (HalFrgrd/flyline#1040). glibc and Bionic both honour
    // DF_1_NODELETE; macOS dyld does not unload a dylib with thread-locals.
    if build_target.contains("linux") || build_target.contains("android") {
        println!("cargo:rustc-cdylib-link-arg=-Wl,-z,nodelete");
    }

    // Re-run when the reproducible-build timestamp changes.
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
    // Re-run when HEAD changes (branch switch or detached-HEAD commit)
    println!("cargo:rerun-if-changed=.git/HEAD");
    // Re-run when the example agent mode file changes (embedded via include_str! in agent_mode.rs)
    println!("cargo:rerun-if-changed=examples/agent_mode.sh");
    // Re-run when the current branch ref changes (new commit on a branch)
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
