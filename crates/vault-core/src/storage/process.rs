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
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_default()
}
