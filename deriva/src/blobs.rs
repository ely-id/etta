//  The content-addressed byte store.
//
//  `blobs/ab/cd/<sha256>`. Using the hash as the name gives us three things for
//  free: saving the same file twice uses one copy, checking integrity means
//  hashing it again, and there are no names to choose —or user-provided names to
//  escape, which cause half the vulnerabilities in a file store.
//
//  Order matters, and the plan is explicit: **blob first, row second**. In the
//  reverse order, an interruption between the two leaves a visible capture in
//  the library pointing to a nonexistent file, which cannot be fixed by reading
//  the database; we would have to guess what it was. With this order, the worst
//  that can remain is an orphaned blob, which can be seen, counted, and swept.

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

//  What is not copied. This is not a disk-space policy: copying a file this
//  large takes long enough that Marea's confirmation would stop being a
//  confirmation and become a promise.
pub const TOPE_FICHERO: u64 = 2 * 1024 * 1024 * 1024;
//  And what can arrive inline through the protocol, which is different: it
//  travels in a JSON line and must be held entirely in memory twice.
pub const TOPE_EN_LINEA: usize = 64 * 1024 * 1024;

pub struct Guardado {
    pub hash: String,
    pub bytes: u64,
}

//  Write bytes that are already in memory.
pub fn guardar_bytes(datos: &[u8]) -> io::Result<Guardado> {
    let hash = crate::util::sha256(datos);
    let destino = crate::paths::blob_de(&hash);
    if destino.exists() {
        //  Already present: two different captures can share the same file,
        //  and since its name is the hash, there is nothing to write.
        return Ok(Guardado {
            hash,
            bytes: datos.len() as u64,
        });
    }
    escribir_atomico(&destino, |f| f.write_all(datos))?;
    Ok(Guardado {
        hash,
        bytes: datos.len() as u64,
    })
}

//  Copy a file from the user's disk. Hash it while reading, not by loading it
//  all at once: the file could be a video.
pub fn guardar_fichero(origen: &Path) -> io::Result<Guardado> {
    let meta = std::fs::metadata(origen)?;
    if !meta.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "no es un fichero",
        ));
    }
    if meta.len() > TOPE_FICHERO {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "demasiado grande",
        ));
    }
    let hash = crate::util::sha256_de_fichero(origen)?;
    let destino = crate::paths::blob_de(&hash);
    if destino.exists() {
        return Ok(Guardado {
            hash,
            bytes: meta.len(),
        });
    }
    let mut entrada = std::fs::File::open(origen)?;
    escribir_atomico(&destino, |f| {
        let mut buf = vec![0u8; 1 << 20];
        loop {
            let n = entrada.read(&mut buf)?;
            if n == 0 {
                return Ok(());
            }
            f.write_all(&buf[..n])?;
        }
    })?;
    Ok(Guardado {
        hash,
        bytes: meta.len(),
    })
}

//  Write it all to a temporary file, then rename it over the destination. A
//  `rename` within the same filesystem is atomic: either the complete blob is
//  there or it is not, never half a blob named with its hash —the only way an
//  integrity check could give a false positive.
fn escribir_atomico<F>(destino: &Path, escribir: F) -> io::Result<()>
where
    F: FnOnce(&mut std::fs::File) -> io::Result<()>,
{
    if let Some(p) = destino.parent() {
        std::fs::create_dir_all(p)?;
    }
    std::fs::create_dir_all(crate::paths::tmp())?;
    let mut b = [0u8; 8];
    crate::util::azar(&mut b);
    let temporal = crate::paths::tmp().join(format!("t{}", crate::util::hex(&b)));
    {
        let mut f = std::fs::File::create(&temporal)?;
        let r = escribir(&mut f);
        if r.is_err() {
            let _ = std::fs::remove_file(&temporal);
            return r;
        }
        //  Flush to disk before renaming. Without this, an interruption right
        //  afterward could leave the name pointing to a file whose contents
        //  were still in the system cache.
        f.sync_all()?;
    }
    std::fs::rename(&temporal, destino)?;
    Ok(())
}

//  What was left incomplete by a previous startup. Sweep it at startup, when
//  we know nobody is writing.
pub fn limpiar_temporales() -> io::Result<u64> {
    let d = crate::paths::tmp();
    if !d.exists() {
        return Ok(0);
    }
    let mut n = 0;
    for e in std::fs::read_dir(&d)? {
        let e = e?;
        if e.path().is_file() && std::fs::remove_file(e.path()).is_ok() {
            n += 1;
        }
    }
    Ok(n)
}

//  Every blob on disk. Used to count those that no longer have an owner.
pub fn todos() -> io::Result<Vec<(String, PathBuf, u64)>> {
    let mut fuera = Vec::new();
    let raiz = crate::paths::blobs();
    if !raiz.exists() {
        return Ok(fuera);
    }
    for a in std::fs::read_dir(&raiz)? {
        let a = a?;
        if !a.path().is_dir() || a.file_name() == ".tmp" {
            continue;
        }
        for b in std::fs::read_dir(a.path())? {
            let b = b?;
            if !b.path().is_dir() {
                continue;
            }
            for f in std::fs::read_dir(b.path())? {
                let f = f?;
                let meta = match f.metadata() {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                if !meta.is_file() {
                    continue;
                }
                let nombre = f.file_name().to_string_lossy().to_string();
                fuera.push((nombre, f.path(), meta.len()));
            }
        }
    }
    Ok(fuera)
}
