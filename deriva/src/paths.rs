//  Dónde vive la biblioteca.
//
//  Las del plan y ninguna más: los datos en `XDG_DATA_HOME`, el socket en
//  `XDG_RUNTIME_DIR`. Nada en `XDG_STATE_HOME` —ahí solo va el estado pequeño
//  del plugin, que escribe QML— y nada junto al código.
//
//  `MAREA_DERIVA_DIR` lo mueve todo. Existe para las pruebas: una prueba que
//  escribe en la biblioteca del usuario no es una prueba, es un accidente
//  esperando a que alguien la ejecute en su portátil.

use std::io;
use std::path::PathBuf;

pub fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()))
}

pub fn base() -> PathBuf {
    if let Ok(d) = std::env::var("MAREA_DERIVA_DIR") {
        if !d.is_empty() {
            return PathBuf::from(d);
        }
    }
    let datos = std::env::var("XDG_DATA_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".local/share"));
    datos.join("proyecto-marea/deriva")
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
//  Los temporales van DENTRO de la carpeta de blobs, no en `/tmp`: un blob se
//  escribe entero en su temporal y se renombra encima, y `rename` solo es
//  atómico dentro del mismo sistema de ficheros. Con `/tmp` en tmpfs, que es lo
//  normal, la copia deja de ser atómica sin avisar.
pub fn tmp() -> PathBuf {
    blobs().join(".tmp")
}

//  El socket. En `XDG_RUNTIME_DIR` porque es lo que se borra al cerrar sesión y
//  ya viene con permisos de solo el usuario; en `/tmp` lo vería cualquiera con
//  cuenta en la máquina.
pub fn socket() -> PathBuf {
    if let Ok(s) = std::env::var("MAREA_DERIVA_SOCKET") {
        if !s.is_empty() {
            return PathBuf::from(s);
        }
    }
    let run = std::env::var("XDG_RUNTIME_DIR")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", unsafe { libc_getuid() })));
    run.join("proyecto-marea/deriva.sock")
}

//  Sin `libc` como dependencia: es la única llamada que hacía falta y traerse
//  una caja entera para un `getuid` es pagar de más.
unsafe fn libc_getuid() -> u32 {
    extern "C" {
        fn getuid() -> u32;
    }
    getuid()
}

//  Todo lo que tiene que existir antes de escribir nada, con los permisos que
//  toca. 0700 en la carpeta: la biblioteca es lo que has ido guardando durante
//  meses y no tiene por qué leerla otro usuario de la máquina.
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

//  Dónde cae un blob por su hash: `blobs/ab/cd/<sha256>`. Dos niveles de dos
//  caracteres, que es lo que evita un directorio con cien mil entradas dentro.
pub fn blob_de(hash: &str) -> PathBuf {
    blobs().join(&hash[0..2]).join(&hash[2..4]).join(hash)
}
