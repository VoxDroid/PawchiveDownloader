//! Talking to the operating system about power and disks: keeping the
//! computer awake while downloads run, free space on the download drive, and
//! sleeping or shutting down when the queue is done.

use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

/// What to do once every download has finished.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WhenDone {
    #[default]
    Nothing,
    Quit,
    Sleep,
    Shutdown,
}

/// Free bytes for the user on the drive holding `path` (or its nearest
/// existing parent). None when it cannot be told.
pub fn free_space(path: &Path) -> Option<u64> {
    let mut dir = path;
    while !dir.exists() {
        dir = dir.parent()?;
    }
    imp::free_space(dir)
}

/// Keeps the computer from sleeping while held. Dropping or `set(false)`
/// lets it sleep again. The display may still turn off.
pub struct KeepAwake {
    inner: parking_lot::Mutex<imp::Awake>,
}

impl Default for KeepAwake {
    fn default() -> Self {
        Self::new()
    }
}

impl KeepAwake {
    pub fn new() -> Self {
        Self {
            inner: parking_lot::Mutex::new(imp::Awake::default()),
        }
    }

    pub fn set(&self, on: bool) {
        let mut inner = self.inner.lock();
        if on {
            inner.hold();
        } else {
            inner.release();
        }
    }

    pub fn is_held(&self) -> bool {
        self.inner.lock().held()
    }
}

/// Starts sleep or shutdown. Quit is handled by the app itself.
pub fn perform(action: WhenDone) -> Result<(), String> {
    let (program, args) = match action {
        WhenDone::Nothing | WhenDone::Quit => return Ok(()),
        WhenDone::Sleep => imp::sleep_command(),
        WhenDone::Shutdown => imp::shutdown_command(),
    };
    Command::new(program)
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("{program}: {e}"))
}

#[cfg(windows)]
mod imp {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::sync::mpsc;

    const ES_CONTINUOUS: u32 = 0x8000_0000;
    const ES_SYSTEM_REQUIRED: u32 = 0x0000_0001;

    #[link(name = "kernel32")]
    extern "system" {
        fn SetThreadExecutionState(flags: u32) -> u32;
        fn GetDiskFreeSpaceExW(
            directory: *const u16,
            free_to_caller: *mut u64,
            total: *mut u64,
            total_free: *mut u64,
        ) -> i32;
    }

    pub fn sleep_command() -> (&'static str, Vec<&'static str>) {
        (
            "rundll32.exe",
            vec!["powrprof.dll,SetSuspendState", "0,1,0"],
        )
    }
    pub fn shutdown_command() -> (&'static str, Vec<&'static str>) {
        (
            "shutdown.exe",
            vec![
                "/s",
                "/t",
                "30",
                "/c",
                "Pawchive Downloader finished its downloads.",
            ],
        )
    }

    pub fn free_space(dir: &Path) -> Option<u64> {
        let wide: Vec<u16> = dir.as_os_str().encode_wide().chain([0]).collect();
        let mut free = 0u64;
        // SAFETY: a valid, NUL-terminated wide string and out pointers to locals.
        let ok = unsafe {
            GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &mut free,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        (ok != 0).then_some(free)
    }

    /// The execution state belongs to a thread, so one thread holds it.
    #[derive(Default)]
    pub struct Awake {
        stop: Option<mpsc::Sender<()>>,
    }

    impl Awake {
        pub fn hold(&mut self) {
            if self.stop.is_some() {
                return;
            }
            let (tx, rx) = mpsc::channel::<()>();
            let spawned = std::thread::Builder::new()
                .name("keep-awake".into())
                .spawn(move || {
                    // SAFETY: plain flag values; no pointers involved.
                    unsafe { SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED) };
                    let _ = rx.recv();
                    unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
                });
            if spawned.is_ok() {
                self.stop = Some(tx);
            }
        }
        pub fn release(&mut self) {
            if let Some(tx) = self.stop.take() {
                let _ = tx.send(());
            }
        }
        pub fn held(&self) -> bool {
            self.stop.is_some()
        }
    }

    impl Drop for Awake {
        fn drop(&mut self) {
            self.release();
        }
    }
}

#[cfg(unix)]
mod imp {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;
    use std::process::{Child, Command, Stdio};

    #[cfg(target_os = "macos")]
    pub fn sleep_command() -> (&'static str, Vec<&'static str>) {
        ("pmset", vec!["sleepnow"])
    }
    #[cfg(target_os = "macos")]
    pub fn shutdown_command() -> (&'static str, Vec<&'static str>) {
        (
            "osascript",
            vec!["-e", "tell application \"System Events\" to shut down"],
        )
    }
    #[cfg(not(target_os = "macos"))]
    pub fn sleep_command() -> (&'static str, Vec<&'static str>) {
        ("systemctl", vec!["suspend"])
    }
    #[cfg(not(target_os = "macos"))]
    pub fn shutdown_command() -> (&'static str, Vec<&'static str>) {
        ("systemctl", vec!["poweroff"])
    }

    pub fn free_space(dir: &Path) -> Option<u64> {
        let path = CString::new(dir.as_os_str().as_bytes()).ok()?;
        let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
        // SAFETY: a valid C string and a pointer to a zeroed local struct.
        let rc = unsafe { libc::statvfs(path.as_ptr(), &mut stat) };
        (rc == 0).then(|| (stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64))
    }

    /// A helper process that blocks sleep while it runs.
    #[derive(Default)]
    pub struct Awake {
        child: Option<Child>,
    }

    impl Awake {
        pub fn hold(&mut self) {
            if self.child.is_some() {
                return;
            }
            let pid = std::process::id().to_string();
            #[cfg(target_os = "macos")]
            let command = Command::new("caffeinate").args(["-i", "-w", &pid]).spawn();
            #[cfg(not(target_os = "macos"))]
            let command = {
                let _ = &pid;
                Command::new("systemd-inhibit")
                    .args([
                        "--what=sleep:idle",
                        "--who=Pawchive Downloader",
                        "--why=Downloading",
                        "--mode=block",
                        "sleep",
                        "infinity",
                    ])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
            };
            self.child = command.ok();
        }
        pub fn release(&mut self) {
            if let Some(mut child) = self.child.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        pub fn held(&self) -> bool {
            self.child.is_some()
        }
    }

    impl Drop for Awake {
        fn drop(&mut self) {
            self.release();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_free_space_of_existing_and_missing_folders() {
        let dir = std::env::temp_dir();
        let free = free_space(&dir).expect("free space of the temp folder");
        assert!(free > 0);
        let missing = dir.join("pawchive-no-such-folder").join("deeper");
        assert_eq!(free_space(&missing).map(|f| f > 0), Some(true));
    }

    #[test]
    fn holds_and_releases_the_awake_state() {
        let awake = KeepAwake::new();
        awake.set(true);
        awake.set(true);
        awake.set(false);
        assert!(!awake.is_held());
    }
}
