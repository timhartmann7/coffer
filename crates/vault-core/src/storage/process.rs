//! Whether a process id still names a running process.
//!
//! Used to tell a lock file left by a live editor from one left by a process
//! that died, and to sweep temporary files abandoned by a crashed save.

/// True when a signal could be delivered to `pid`, which is what "still
/// running" means on a POSIX system. A process owned by another user answers
/// `EPERM`, which is also proof that it exists.
pub(crate) fn is_alive(pid: u32) -> bool {
    // `kill` reads zero and negative process ids as process groups, so a lock
    // file claiming one of those is not naming a process at all. Treat it as
    // alive: clearing somebody's lock on the strength of a malformed field is
    // worse than leaving a lock nobody holds.
    if pid == 0 {
        return true;
    }

    let Ok(pid) = i32::try_from(pid) else {
        return true;
    };

    // SAFETY: signal 0 performs the permission and existence checks without
    // delivering anything, so this cannot affect the target process.
    let result = unsafe { libc::kill(pid, 0) };

    if result == 0 {
        return true;
    }

    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

pub(crate) fn current() -> u32 {
    std::process::id()
}

/// The host this process runs on, used to decide whether a lock file's process
/// id means anything to us. A lock written on another machine, on a shared
/// volume, is never stale as far as we are concerned.
pub(crate) fn hostname() -> String {
    // Bytes, and the pointer cast once at the call. `c_char` is signed on x86_64
    // and on every Apple target and unsigned on aarch64 Linux, so an array of it
    // needs a per-element cast that is a compile error on one of the two and an
    // unnecessary-cast warning on the other. A name is bytes either way.
    let mut buffer = [0u8; 256];

    // SAFETY: the buffer is valid for the length passed, and the result is only
    // read up to the first NUL.
    let result = unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len() - 1) };

    if result != 0 {
        return String::new();
    }

    let name: Vec<u8> = buffer.iter().copied().take_while(|&c| c != 0).collect();

    String::from_utf8_lossy(&name).into_owned()
}

/// The account this process runs as. Only ever shown to the user, to name who
/// is holding a lock.
pub(crate) fn username() -> String {
    // The environment first: it is what the user set, and an account with more
    // than one name should show up under the one they chose.
    let named = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .ok()
        .filter(|name| !name.is_empty());

    // And the password database when it is silent, which it is more often than
    // a shell suggests - a process started by launchd, and a container, both
    // arrive with neither variable set. A lock naming nobody is a worse answer
    // than the true one.
    named.or_else(from_password_database).unwrap_or_default()
}

fn from_password_database() -> Option<String> {
    let mut record: libc::passwd = unsafe { std::mem::zeroed() };
    let mut buffer = [0u8; 1024];
    let mut found: *mut libc::passwd = std::ptr::null_mut();

    // SAFETY: the record and the buffer outlive the call and the length passed
    // is the buffer's own. `found` is only read after the call has reported
    // success, which is when it has been written.
    let result = unsafe {
        libc::getpwuid_r(
            libc::geteuid(),
            &mut record,
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            &mut found,
        )
    };

    if result != 0 || found.is_null() || record.pw_name.is_null() {
        return None;
    }

    // SAFETY: a successful call points `pw_name` into `buffer`, which is still
    // alive here, and terminates it.
    let name = unsafe { std::ffi::CStr::from_ptr(record.pw_name) };

    Some(name.to_string_lossy().into_owned()).filter(|name| !name.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fallback runs only when neither variable is set, which is a state a
    /// test cannot arrange: the environment belongs to the whole process and
    /// these run in threads beside each other. So it is called directly, which
    /// is also the only way the unsafe block inside it is ever exercised on a
    /// developer's machine.
    #[test]
    fn the_password_database_names_the_account_this_process_runs_as() {
        let name = from_password_database().unwrap_or_default();

        assert!(
            !name.is_empty(),
            "the password database did not name this process's account"
        );
        assert!(
            !name.contains('\0'),
            "the name ran past the end of what the call wrote"
        );
    }

    #[test]
    fn a_process_group_is_never_read_as_a_dead_process() {
        assert!(is_alive(0), "a lock claiming pid 0 was called stale");
    }

    #[test]
    fn this_process_is_alive_and_a_pid_that_cannot_exist_is_not() {
        assert!(is_alive(current()), "this process was called dead");
        assert!(
            !is_alive(i32::MAX as u32 - 1),
            "a pid no system has assigned was called alive"
        );
    }
}
