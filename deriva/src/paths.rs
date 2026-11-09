//  Where the library lives.
//
//  The locations from the plan, and no others: data in `XDG_DATA_HOME`, the
//  socket in `XDG_RUNTIME_DIR`. Nothing in `XDG_STATE_HOME` —only the plugin's
//  small state goes there, written by QML— and nothing alongside the code.
//
//  `ETTA_DERIVA_DIR` moves everything. It exists for tests: a test that writes
//  to the user's library is not a test, but an accident waiting for someone to
//  run it on their laptop.

use std::io;
use std::path::PathBuf;

pub fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()))
}

pub fn base() -> PathBuf {
    if let Ok(d) = std::env::var("ETTA_DERIVA_DIR") {
        if !d.is_empty() {
            return PathBuf::from(d);
        }
    }
    let datos = std::env::var("XDG_DATA_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".local/share"));
    datos.join("etta/deriva")
}

pub fn db_path() -> PathBuf {
    base().join("library.sqlite3")
}
pub fn blobs() -> PathBuf {
    base().join("blobs")
}
pub fn previews() -> PathBuf {
    base().join("previews")
}
pub fn exports() -> PathBuf {
    base().join("exports")
}
pub fn backups() -> PathBuf {
    base().join("backups")
}
//  Temporary files go INSIDE the blobs directory, not in `/tmp`: a blob is
//  written in full to its temporary file and renamed over the destination, and
//  `rename` is only atomic within the same filesystem. With `/tmp` on tmpfs, as
//  is usual, the copy silently stops being atomic.
pub fn tmp() -> PathBuf {
    blobs().join(".tmp")
}

//  The socket. In `XDG_RUNTIME_DIR` because it is cleared at logout and already
//  has user-only permissions; in `/tmp`, anyone with an account on the machine
//  could see it.
pub fn socket() -> PathBuf {
    if let Ok(s) = std::env::var("ETTA_DERIVA_SOCKET") {
        if !s.is_empty() {
            return PathBuf::from(s);
        }
    }
    let run = std::env::var("XDG_RUNTIME_DIR")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", unsafe { libc_getuid() })));
    run.join("etta/deriva.sock")
}

//  No `libc` dependency: this is the only call we needed, and bringing in a
//  whole crate for `getuid` is more than it is worth.
unsafe fn libc_getuid() -> u32 {
    extern "C" {
        fn getuid() -> u32;
    }
    getuid()
}

//  Everything that must exist before writing anything, with the right
//  permissions. 0700 on the directory: the library holds what you have saved
//  over months, and other users on the machine have no reason to read it.
pub fn preparar() -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    for d in [base(), blobs(), previews(), exports(), backups(), tmp()] {
        std::fs::create_dir_all(&d)?;
        let mut p = std::fs::metadata(&d)?.permissions();
        p.set_mode(0o700);
        let _ = std::fs::set_permissions(&d, p);
    }
    Ok(())
}

//  Where a blob goes based on its hash: `blobs/ab/cd/<sha256>`. Two levels of
//  two characters keep a directory from containing a hundred thousand entries.
pub fn blob_de(hash: &str) -> PathBuf {
    blobs().join(&hash[0..2]).join(&hash[2..4]).join(hash)
}
