use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

const SUPERVISOR_TEST_LOCK_FILE: &str = "Cadence-runtime-supervisor-test.lock";
const SUPERVISOR_TEST_LOCK_TIMEOUT: Duration = Duration::from_secs(120);
const SUPERVISOR_TEST_LOCK_POLL: Duration = Duration::from_millis(50);

pub(crate) struct SupervisorProcessLock {
    path: PathBuf,
    _file: File,
}

pub(crate) fn lock_supervisor_test_process() -> SupervisorProcessLock {
    let path = std::env::temp_dir().join(SUPERVISOR_TEST_LOCK_FILE);
    let deadline = Instant::now() + SUPERVISOR_TEST_LOCK_TIMEOUT;

    loop {
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                writeln!(file, "{}", std::process::id())
                    .expect("write supervisor test lock pid");
                return SupervisorProcessLock { path, _file: file };
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                assert!(
                    Instant::now() < deadline,
                    "timed out waiting for supervisor test process lock at {}",
                    path.display()
                );
                thread::sleep(SUPERVISOR_TEST_LOCK_POLL);
            }
            Err(error) => panic!(
                "failed to acquire supervisor test process lock at {}: {error}",
                path.display()
            ),
        }
    }
}

impl Drop for SupervisorProcessLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
