//! A small wrapper around [`std::process::Child`] that kills and reaps the
//! child when it is dropped.
//!
//! Used both for the AI agent process (see [`crate::app::ContentMode::AgentModeWaiting`])
//! and for custom prompt widget child processes (see
//! [`crate::prompt_manager`]) so that no orphaned processes are left behind
//! when their owning state transitions away.

/// A [`std::process::Child`] that is killed and waited on when dropped, so it
/// does not outlive its owner.
///
/// When spawned via [`KillOnDropChild::spawn`] (or a [`std::process::Command`]
/// configured with `process_group(0)`), dropping kills the entire process group
/// so descendant processes are not orphaned.
pub struct KillOnDropChild(pub std::process::Child);

impl KillOnDropChild {
    pub fn new(child: std::process::Child) -> Self {
        KillOnDropChild(child)
    }

    /// Place `cmd` in its own process group (`pgid == pid`) and spawn it wrapped
    /// in a [`KillOnDropChild`] so dropping terminates its entire process tree.
    #[allow(dead_code)]
    pub fn spawn(cmd: &mut std::process::Command) -> std::io::Result<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        cmd.spawn().map(Self)
    }
}

impl std::ops::Deref for KillOnDropChild {
    type Target = std::process::Child;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for KillOnDropChild {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl std::fmt::Debug for KillOnDropChild {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KillOnDropChild")
            .field("pid", &self.0.id())
            .finish()
    }
}

impl Drop for KillOnDropChild {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            let pid = self.0.id();
            if pid > 0
                && let Ok(raw_pid) = libc::pid_t::try_from(pid)
            {
                let _ = nix::sys::signal::killpg(
                    nix::unistd::Pid::from_raw(raw_pid),
                    nix::sys::signal::Signal::SIGKILL,
                );
            }
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Whether `pid` is a live process. A killed child that nobody has reaped yet
/// is a zombie, which `kill(pid, 0)` still finds; that happens when the
/// test runs as PID 1's descendant in a container, where orphans are not
/// reaped until the shell exits. Count a zombie as dead.
#[cfg(test)]
pub(crate) fn test_process_is_running(pid: nix::unistd::Pid) -> bool {
    let exists = nix::sys::signal::kill(pid, None).is_ok();
    #[cfg(target_os = "linux")]
    if exists
        && let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat"))
        && let Some(state) = stat
            .rsplit_once(')')
            .and_then(|(_, rest)| rest.trim_start().chars().next())
    {
        return state != 'Z';
    }
    exists
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    #[test]
    fn dropping_kill_on_drop_child_kills_process_group_descendants() {
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", "sleep 60 & echo $!; wait"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        let mut child = KillOnDropChild::spawn(&mut cmd).expect("spawn sh");
        let stdout = child.stdout.take().expect("piped stdout");
        let mut line = String::new();
        BufReader::new(stdout)
            .read_line(&mut line)
            .expect("read grandchild pid");
        let grandchild_pid = nix::unistd::Pid::from_raw(
            line.trim()
                .parse::<libc::pid_t>()
                .expect("parse grandchild pid"),
        );

        assert!(
            nix::sys::signal::kill(grandchild_pid, None).is_ok(),
            "grandchild {grandchild_pid} should be alive before drop"
        );

        drop(child);

        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if !test_process_is_running(grandchild_pid) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }

        let _ = nix::sys::signal::kill(grandchild_pid, nix::sys::signal::Signal::SIGKILL);
        panic!("grandchild {grandchild_pid} survived KillOnDropChild::drop");
    }
}
