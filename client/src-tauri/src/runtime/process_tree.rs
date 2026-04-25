use std::{
    io,
    process::{Child, Command},
    thread,
    time::{Duration, Instant},
};

const GRACEFUL_TERMINATION_TIMEOUT: Duration = Duration::from_millis(500);

pub(crate) fn configure_process_tree_root(command: &mut Command) {
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

pub(crate) fn terminate_process_tree(child: &mut Child) -> io::Result<std::process::ExitStatus> {
    let child_id = child.id();
    if let Some(status) = child.try_wait()? {
        cleanup_process_group_after_root_exit(child_id);
        return Ok(status);
    }

    terminate_process_tree_gracefully(child)?;
    if let Some(status) = wait_for_exit(child, GRACEFUL_TERMINATION_TIMEOUT)? {
        return Ok(status);
    }

    terminate_process_tree_forcefully(child)?;
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
        let _ = child_id;
    }
}

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
fn terminate_process_tree_gracefully(child: &mut Child) -> io::Result<()> {
    signal_process_group_id(child.id(), libc::SIGTERM)
}

#[cfg(unix)]
fn terminate_process_tree_forcefully(child: &mut Child) -> io::Result<()> {
    if let Err(error) = signal_process_group_id(child.id(), libc::SIGKILL) {
        child.kill().or(Err(error))?;
    }
    Ok(())
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
    taskkill_process_tree(child, false)
}

#[cfg(windows)]
fn terminate_process_tree_forcefully(child: &mut Child) -> io::Result<()> {
    if let Err(error) = taskkill_process_tree(child, true) {
        child.kill().or(Err(error))?;
    }
    Ok(())
}

#[cfg(windows)]
fn taskkill_process_tree(child: &Child, force: bool) -> io::Result<()> {
    let mut command = Command::new("taskkill");
    command.arg("/PID").arg(child.id().to_string()).arg("/T");
    if force {
        command.arg("/F");
    }
    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!("taskkill exited with status {status}"),
        ))
    }
}
