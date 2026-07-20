use std::{
    io,
    process::{Child, Command},
    thread,
    time::{Duration, Instant},
};

const GRACEFUL_TERMINATION_TIMEOUT: Duration = Duration::from_millis(500);

pub(crate) fn configure_process_tree_root(command: &mut Command) {
    if xero_agent_core::mutation_boundary_child_active() {
        return;
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command.creation_flags(CREATE_NEW_PROCESS_GROUP);
    }
}

/// Registers platform ownership that cannot be expressed through
/// `std::process::Command` before spawn. Windows roots are placed in a Job
/// Object configured to kill every member when Xero releases the job.
pub(crate) fn register_process_tree_root(child: &Child) -> io::Result<()> {
    if xero_agent_core::mutation_boundary_child_active() {
        return Ok(());
    }

    #[cfg(windows)]
    {
        return windows_job::register(child.id());
    }

    #[cfg(not(windows))]
    {
        let _ = child;
        Ok(())
    }
}

pub(crate) fn terminate_process_tree(child: &mut Child) -> io::Result<std::process::ExitStatus> {
    #[cfg(unix)]
    {
        return terminate_unix_process_group(child);
    }

    #[cfg(windows)]
    {
        return terminate_windows_process_tree(child);
    }

    #[cfg(not(any(unix, windows)))]
    {
        child.kill()?;
        child.wait()
    }
}

/// Conservatively reports whether an operating-system process still exists.
/// Permission failures are treated as live so a lease is never stolen merely
/// because Xero cannot inspect its owner.
pub(crate) fn process_is_alive(process_id: u32) -> bool {
    #[cfg(unix)]
    {
        let Ok(process_id) = libc::pid_t::try_from(process_id) else {
            return false;
        };
        let result = unsafe { libc::kill(process_id, 0) };
        if result == 0 {
            return true;
        }
        return io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH);
    }

    #[cfg(windows)]
    {
        return windows_process::is_alive(process_id);
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = process_id;
        true
    }
}

/// Returns a stable identity for one operating-system process lifetime.
///
/// A PID alone is not an ownership fence because operating systems reuse PIDs. Durable leases
/// persist this value alongside the PID and only consider the owner live while both still match.
pub(crate) fn process_birth_identity(process_id: u32) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let stat = std::fs::read_to_string(format!("/proc/{process_id}/stat")).ok()?;
        let fields_after_name = stat.rsplit_once(") ")?.1;
        let start_ticks = fields_after_name.split_whitespace().nth(19)?;
        let boot_id = std::fs::read_to_string("/proc/sys/kernel/random/boot_id").ok()?;
        return Some(format!("linux:{}:{start_ticks}", boot_id.trim()));
    }

    #[cfg(target_os = "macos")]
    {
        let process_id = libc::pid_t::try_from(process_id).ok()?;
        let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::zeroed();
        let size = std::mem::size_of::<libc::proc_bsdinfo>();
        let read = unsafe {
            libc::proc_pidinfo(
                process_id,
                libc::PROC_PIDTBSDINFO,
                0,
                info.as_mut_ptr().cast(),
                i32::try_from(size).ok()?,
            )
        };
        if read != i32::try_from(size).ok()? {
            return None;
        }
        let info = unsafe { info.assume_init() };
        return Some(format!(
            "macos:{}:{}",
            info.pbi_start_tvsec, info.pbi_start_tvusec
        ));
    }

    #[cfg(windows)]
    {
        let script = format!(
            "(Get-Process -Id {process_id} -ErrorAction Stop).StartTime.ToUniversalTime().Ticks"
        );
        let output = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let birth = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        return (!birth.is_empty()).then(|| format!("windows:{birth}"));
    }

    #[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
    {
        let output = Command::new("ps")
            .args(["-o", "lstart=", "-p", &process_id.to_string()])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let birth = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        return (!birth.is_empty()).then(|| format!("unix:{birth}"));
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = process_id;
        None
    }
}

/// Whether the PID still names the exact process lifetime that acquired a durable lease.
pub(crate) fn process_identity_is_live(process_id: u32, expected_birth_identity: &str) -> bool {
    process_is_alive(process_id)
        && process_birth_identity(process_id).as_deref() == Some(expected_birth_identity)
}

#[cfg(unix)]
fn terminate_unix_process_group(child: &mut Child) -> io::Result<std::process::ExitStatus> {
    let child_id = child.id();
    let mut root_status = child.try_wait()?;
    let owned_process_group_exists = process_group_exists(child_id);
    if !owned_process_group_exists {
        if let Some(status) = root_status.or(child.try_wait()?) {
            return Ok(status);
        }
        child.kill()?;
        return child.wait();
    }
    if let Err(error) = signal_process_group_id(child_id, libc::SIGTERM) {
        return recover_group_signal_race_after_root_exit(child, &mut root_status, error);
    }
    let deadline = Instant::now() + GRACEFUL_TERMINATION_TIMEOUT;
    while process_group_exists(child_id) && Instant::now() < deadline {
        if root_status.is_none() {
            root_status = child.try_wait()?;
        }
        thread::sleep(Duration::from_millis(20));
    }

    if process_group_exists(child_id) {
        if let Err(error) = signal_process_group_id(child_id, libc::SIGKILL) {
            return recover_group_signal_race_after_root_exit(child, &mut root_status, error);
        }
    }
    root_status.map_or_else(|| child.wait(), Ok)
}

#[cfg(unix)]
fn recover_group_signal_race_after_root_exit(
    child: &mut Child,
    root_status: &mut Option<std::process::ExitStatus>,
    signal_error: io::Error,
) -> io::Result<std::process::ExitStatus> {
    // A short-lived root can exit after `try_wait`/group discovery but before
    // the signal. On macOS that race may report EPERM for the now-stale group
    // instead of ESRCH. Only suppress it when the owned root is provably
    // reaped; a live root with an actual permission failure must still fail.
    if signal_error.raw_os_error() != Some(libc::EPERM) {
        return Err(signal_error);
    }
    if root_status.is_none() {
        *root_status = child.try_wait()?;
    }
    root_status.take().ok_or(signal_error)
}

#[cfg(windows)]
fn terminate_windows_process_tree(child: &mut Child) -> io::Result<std::process::ExitStatus> {
    let child_id = child.id();
    if let Some(status) = child.try_wait()? {
        cleanup_process_group_after_root_exit(child_id);
        return Ok(status);
    }

    if let Err(graceful_error) = terminate_process_tree_gracefully(child) {
        terminate_windows_process_tree_forcefully(child).map_err(|force_error| {
            io::Error::new(
                force_error.kind(),
                format!(
                    "graceful process-tree termination failed ({graceful_error}); forceful termination also failed ({force_error})"
                ),
            )
        })?;
        let status = child.wait()?;
        cleanup_process_group_after_root_exit(child_id);
        return Ok(status);
    }
    if let Some(status) = wait_for_exit(child, GRACEFUL_TERMINATION_TIMEOUT)? {
        cleanup_process_group_after_root_exit(child_id);
        return Ok(status);
    }

    terminate_windows_process_tree_forcefully(child)?;
    let status = child.wait()?;
    cleanup_process_group_after_root_exit(child_id);
    Ok(status)
}

pub(crate) fn cleanup_process_group_after_root_exit(child_id: u32) {
    #[cfg(unix)]
    {
        let _ = signal_process_group_id(child_id, libc::SIGTERM);
        let deadline = Instant::now() + Duration::from_millis(100);
        while process_group_exists(child_id) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        if process_group_exists(child_id) {
            let _ = signal_process_group_id(child_id, libc::SIGKILL);
        }
    }

    #[cfg(windows)]
    {
        windows_job::cleanup(child_id);
    }
}

#[cfg(windows)]
fn wait_for_exit(
    child: &mut Child,
    timeout: Duration,
) -> io::Result<Option<std::process::ExitStatus>> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(unix)]
fn signal_process_group_id(child_id: u32, signal: libc::c_int) -> io::Result<()> {
    let process_group_id = -(child_id as libc::pid_t);
    let result = unsafe { libc::kill(process_group_id, signal) };
    if result == 0 {
        return Ok(());
    }

    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        Ok(())
    } else {
        Err(error)
    }
}

#[cfg(unix)]
fn process_group_exists(child_id: u32) -> bool {
    let process_group_id = -(child_id as libc::pid_t);
    let result = unsafe { libc::kill(process_group_id, 0) };
    if result == 0 {
        return true;
    }
    let error = io::Error::last_os_error();
    error.raw_os_error() != Some(libc::ESRCH)
}

#[cfg(windows)]
fn terminate_process_tree_gracefully(child: &mut Child) -> io::Result<()> {
    windows_process::signal_break(child.id())
}

#[cfg(windows)]
fn terminate_windows_process_tree_forcefully(child: &mut Child) -> io::Result<()> {
    if windows_job::terminate(child.id()).is_ok() {
        return Ok(());
    }
    child.kill()
}

#[cfg(windows)]
mod windows_process {
    use std::{ffi::c_void, io};

    const CTRL_BREAK_EVENT: u32 = 1;
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const STILL_ACTIVE: u32 = 259;
    const ERROR_INVALID_PARAMETER: i32 = 87;

    type RawHandle = *mut c_void;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "OpenProcess"]
        fn open_process(desired_access: u32, inherit_handle: i32, process_id: u32) -> RawHandle;
        #[link_name = "GetExitCodeProcess"]
        fn get_exit_code_process(process: RawHandle, exit_code: *mut u32) -> i32;
        #[link_name = "GenerateConsoleCtrlEvent"]
        fn generate_console_ctrl_event(control_event: u32, process_group_id: u32) -> i32;
        #[link_name = "CloseHandle"]
        fn close_handle(handle: RawHandle) -> i32;
    }

    pub(super) fn is_alive(process_id: u32) -> bool {
        let process = unsafe { open_process(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
        if process.is_null() {
            return io::Error::last_os_error().raw_os_error() != Some(ERROR_INVALID_PARAMETER);
        }

        let mut exit_code = 0;
        let queried = unsafe { get_exit_code_process(process, &mut exit_code) };
        unsafe {
            let _ = close_handle(process);
        }
        queried == 0 || exit_code == STILL_ACTIVE
    }

    pub(super) fn signal_break(process_group_id: u32) -> io::Result<()> {
        if unsafe { generate_console_ctrl_event(CTRL_BREAK_EVENT, process_group_id) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

#[cfg(windows)]
mod windows_job {
    use std::{
        collections::HashMap,
        ffi::c_void,
        io,
        mem::size_of,
        ptr,
        sync::{Mutex, OnceLock},
    };

    const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS: i32 = 9;
    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x0000_2000;
    const PROCESS_TERMINATE: u32 = 0x0001;
    const PROCESS_SET_QUOTA: u32 = 0x0100;

    type RawHandle = *mut c_void;

    #[repr(C)]
    #[derive(Default)]
    struct JobObjectBasicLimitInformation {
        per_process_user_time_limit: i64,
        per_job_user_time_limit: i64,
        limit_flags: u32,
        minimum_working_set_size: usize,
        maximum_working_set_size: usize,
        active_process_limit: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }

    #[repr(C)]
    #[derive(Default)]
    struct IoCounters {
        read_operation_count: u64,
        write_operation_count: u64,
        other_operation_count: u64,
        read_transfer_count: u64,
        write_transfer_count: u64,
        other_transfer_count: u64,
    }

    #[repr(C)]
    #[derive(Default)]
    struct JobObjectExtendedLimitInformation {
        basic_limit_information: JobObjectBasicLimitInformation,
        io_info: IoCounters,
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory_used: usize,
        peak_job_memory_used: usize,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "CreateJobObjectW"]
        fn create_job_object(job_attributes: *const c_void, name: *const u16) -> RawHandle;
        #[link_name = "SetInformationJobObject"]
        fn set_information_job_object(
            job: RawHandle,
            information_class: i32,
            information: *const c_void,
            information_length: u32,
        ) -> i32;
        #[link_name = "OpenProcess"]
        fn open_process(desired_access: u32, inherit_handle: i32, process_id: u32) -> RawHandle;
        #[link_name = "AssignProcessToJobObject"]
        fn assign_process_to_job_object(job: RawHandle, process: RawHandle) -> i32;
        #[link_name = "TerminateJobObject"]
        fn terminate_job_object(job: RawHandle, exit_code: u32) -> i32;
        #[link_name = "CloseHandle"]
        fn close_handle(handle: RawHandle) -> i32;
    }

    struct JobHandle(usize);

    impl JobHandle {
        fn raw(&self) -> RawHandle {
            self.0 as RawHandle
        }
    }

    impl Drop for JobHandle {
        fn drop(&mut self) {
            unsafe {
                let _ = close_handle(self.raw());
            }
        }
    }

    fn jobs() -> &'static Mutex<HashMap<u32, JobHandle>> {
        static JOBS: OnceLock<Mutex<HashMap<u32, JobHandle>>> = OnceLock::new();
        JOBS.get_or_init(|| Mutex::new(HashMap::new()))
    }

    pub(super) fn register(process_id: u32) -> io::Result<()> {
        let raw_job = unsafe { create_job_object(ptr::null(), ptr::null()) };
        if raw_job.is_null() {
            return Err(io::Error::last_os_error());
        }
        let job = JobHandle(raw_job as usize);
        let mut limits = JobObjectExtendedLimitInformation::default();
        limits.basic_limit_information.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if unsafe {
            set_information_job_object(
                job.raw(),
                JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS,
                ptr::from_ref(&limits).cast(),
                size_of::<JobObjectExtendedLimitInformation>() as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }

        let process = unsafe { open_process(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, process_id) };
        if process.is_null() {
            return Err(io::Error::last_os_error());
        }
        let assigned = unsafe { assign_process_to_job_object(job.raw(), process) };
        let assign_error = (assigned == 0).then(io::Error::last_os_error);
        unsafe {
            let _ = close_handle(process);
        }
        if let Some(error) = assign_error {
            return Err(error);
        }

        jobs()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(process_id, job);
        Ok(())
    }

    pub(super) fn terminate(process_id: u32) -> io::Result<()> {
        let jobs = jobs()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let job = jobs.get(&process_id).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("no Windows Job Object owns process {process_id}"),
            )
        })?;
        if unsafe { terminate_job_object(job.raw(), 1) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    pub(super) fn cleanup(process_id: u32) {
        jobs()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&process_id);
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::{process::Stdio, time::SystemTime};

    use super::*;

    #[test]
    fn process_liveness_uses_the_operating_system_without_spawning_helpers() {
        assert!(process_is_alive(std::process::id()));
        assert!(!process_is_alive(u32::MAX));
    }

    #[test]
    fn process_identity_rejects_a_reused_pid_identity() {
        let process_id = std::process::id();
        let birth_identity = process_birth_identity(process_id)
            .expect("the current process should expose a birth identity");
        assert!(process_identity_is_live(process_id, &birth_identity));
        assert!(!process_identity_is_live(
            process_id,
            "different-process-lifetime"
        ));
    }

    #[test]
    fn group_signal_permission_error_recovers_only_after_the_owned_root_exits() {
        let mut exited = Command::new("/bin/sh")
            .args(["-c", "exit 7"])
            .spawn()
            .expect("spawn exited root");
        let expected = exited.wait().expect("wait for exited root");
        let recovered = recover_group_signal_race_after_root_exit(
            &mut exited,
            &mut Some(expected),
            io::Error::from_raw_os_error(libc::EPERM),
        )
        .expect("an already-reaped root makes the stale group race harmless");
        assert_eq!(recovered.code(), Some(7));

        let mut live = Command::new("/bin/sh")
            .args(["-c", "sleep 30"])
            .spawn()
            .expect("spawn live root");
        let error = recover_group_signal_race_after_root_exit(
            &mut live,
            &mut None,
            io::Error::from_raw_os_error(libc::EPERM),
        )
        .expect_err("a live root must retain the permission failure");
        assert_eq!(error.raw_os_error(), Some(libc::EPERM));
        live.kill().expect("kill live fixture");
        live.wait().expect("reap live fixture");
    }

    #[test]
    fn termination_kills_stubborn_group_members_after_root_exits() {
        let marker = std::env::temp_dir().join(format!(
            "xero-process-tree-stubborn-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        let mut command = Command::new("/bin/sh");
        command
            .arg("-c")
            .arg(format!(
                "(trap '' TERM; sleep 1 || sleep 1; printf leaked > '{}') & exit 0",
                marker.display()
            ))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        configure_process_tree_root(&mut command);
        let mut child = command.spawn().expect("spawn process tree");

        assert!(child.wait().expect("wait for root exit").success());
        let status = terminate_process_tree(&mut child).expect("terminate process group");
        assert!(status.success(), "root should retain its successful exit");
        thread::sleep(Duration::from_millis(1_100));
        assert!(
            !marker.exists(),
            "SIGKILL must reach a descendant that ignored the graceful signal"
        );
        let _ = std::fs::remove_file(marker);
    }
}
