//  El socket.
//
//  Unix y privado, en `XDG_RUNTIME_DIR`, con permisos de solo el dueño. No hay
//  puerto TCP ni lo va a haber: lo que se guarda aquí es lo que has ido
//  recogiendo durante meses, y un puerto —aunque sea en `localhost`— lo alcanza
//  cualquier proceso de la máquina, incluida cualquier pestaña con un `fetch`.
//
//  Un hilo por conexión, cada uno con su propia conexión a SQLite. Con WAL eso
//  es correcto: leer no bloquea escribir. Compartir una `Connection` entre
//  hilos no se puede —no es `Sync`— y serializarlo todo en un hilo haría que un
//  cliente colgado dejara al resto esperando.

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

    //  Un socket que quedó de un arranque anterior. Si alguien contesta detrás,
    //  ya hay un worker y este sobra —arrancar el segundo daría dos procesos
    //  escribiendo la misma base—. Si no contesta nadie, el fichero es basura y
    //  se quita.
    if ruta.exists() {
        if UnixStream::connect(&ruta).is_ok() {
            eprintln!("deriva-worker: ya hay uno escuchando en {}", ruta.display());
            return Err(std::io::Error::new(
                std::io::ErrorKind::AddrInUse,
                "ya hay un worker",
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

    //  Lo que quedó a medias de la vez anterior, antes de aceptar a nadie: es
    //  el único momento en que se sabe que nadie está escribiendo blobs.
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
                eprintln!("deriva-worker: conexión terminada: {}", e);
            }
        });
    }
    let _ = std::fs::remove_file(&ruta);
    Ok(())
}

fn atender(
    flujo: UnixStream,
    parando: &AtomicBool,
    ruta: &std::path::Path,
) -> std::io::Result<()> {
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
            //  No se intenta interpretar: si la línea no cabe, el `id` que
            //  llevaba dentro tampoco se puede leer con garantías.
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
            //  Se contesta ANTES de parar, y se despierta al `accept` que está
            //  bloqueado conectándose al propio socket. Sin eso el proceso se
            //  quedaría esperando una conexión que no va a llegar.
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
