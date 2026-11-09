//  The socket.
//
//  Unix and private, in `XDG_RUNTIME_DIR`, with owner-only permissions. There
//  is no TCP port and there will not be one: this stores what you have collected
//  over months, and any process on the machine —including any browser tab using
//  `fetch`— can reach a port, even on `localhost`.
//
//  One thread per connection, each with its own SQLite connection. With WAL
//  this is correct: reads do not block writes. A `Connection` cannot be shared
//  between threads —it is not `Sync`— and serializing everything on one thread
//  would let a hung client hold up the rest.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub fn servir() -> std::io::Result<()> {
    let ruta = crate::paths::socket();
    if let Some(p) = ruta.parent() {
        std::fs::create_dir_all(p)?;
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(p)?.permissions();
        perm.set_mode(0o700);
        let _ = std::fs::set_permissions(p, perm);
    }

    //  A socket left over from a previous startup. If someone answers, a
    //  worker is already running and this one is unnecessary —starting a
    //  second would mean two processes writing to the same database. If
    //  nobody answers, the file is stale and can be removed.
    if ruta.exists() {
        if UnixStream::connect(&ruta).is_ok() {
            eprintln!(
                "deriva-worker: another worker is already listening at {}",
                ruta.display()
            );
            return Err(std::io::Error::new(
                std::io::ErrorKind::AddrInUse,
                "another worker is already running",
            ));
        }
        let _ = std::fs::remove_file(&ruta);
    }

    let escucha = UnixListener::bind(&ruta)?;
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(&ruta)?.permissions();
        perm.set_mode(0o600);
        let _ = std::fs::set_permissions(&ruta, perm);
    }

    //  Anything left incomplete from last time, before accepting connections:
    //  this is the only time we know nobody is writing blobs.
    let barridos = crate::blobs::limpiar_temporales().unwrap_or(0);
    eprintln!(
        "deriva-worker: escuchando en {} · esquema v{} · {} temporales barridos",
        ruta.display(),
        crate::db::VERSION,
        barridos
    );

    let parando = Arc::new(AtomicBool::new(false));
    for conexion in escucha.incoming() {
        if parando.load(Ordering::SeqCst) {
            break;
        }
        let Ok(flujo) = conexion else { continue };
        let parando = parando.clone();
        let ruta = ruta.clone();
        std::thread::spawn(move || {
            if let Err(e) = atender(flujo, &parando, &ruta) {
                eprintln!("deriva-worker: connection ended: {}", e);
            }
        });
    }
    let _ = std::fs::remove_file(&ruta);
    Ok(())
}

fn atender(flujo: UnixStream, parando: &AtomicBool, ruta: &std::path::Path) -> std::io::Result<()> {
    let db = crate::db::abrir()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    let mut salida = flujo.try_clone()?;
    let lector = BufReader::new(flujo);

    for linea in lector.split(b'\n') {
        let linea = linea?;
        if linea.is_empty() {
            continue;
        }
        if linea.len() > crate::proto::TOPE_LINEA {
            //  Do not try to parse it: if the line does not fit, its `id`
            //  cannot be read reliably either.
            let r = crate::proto::Respuesta::mal("", "linea_larga", "no cabe");
            contestar(&mut salida, &r)?;
            continue;
        }
        let texto = String::from_utf8_lossy(&linea);
        let peticion: crate::proto::Peticion = match serde_json::from_str(&texto) {
            Ok(p) => p,
            Err(e) => {
                let r = crate::proto::Respuesta::mal("", "json", e.to_string());
                contestar(&mut salida, &r)?;
                continue;
            }
        };
        let apagar = peticion.method == "shutdown";
        let r = crate::proto::despachar(&db, &peticion);
        contestar(&mut salida, &r)?;
        if apagar {
            //  Reply BEFORE stopping, and wake the `accept` call blocked by
            //  connecting to the socket itself. Otherwise the process would
            //  wait for a connection that will never arrive.
            parando.store(true, Ordering::SeqCst);
            let _ = UnixStream::connect(ruta);
            break;
        }
    }
    Ok(())
}

fn contestar(salida: &mut UnixStream, r: &crate::proto::Respuesta) -> std::io::Result<()> {
    let mut linea = serde_json::to_string(r).unwrap_or_else(|_| {
        "{\"v\":1,\"id\":\"\",\"ok\":false,\"error\":{\"code\":\"serde\",\"message\":\"\"}}".into()
    });
    linea.push('\n');
    salida.write_all(linea.as_bytes())?;
    salida.flush()
}
