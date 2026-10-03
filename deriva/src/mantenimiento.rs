//  Integridad, copia de seguridad y exportación.
//
//  Las tres son la misma promesa dicha de tres maneras: lo que guardaste sigue
//  ahí, se puede recuperar si algo se rompe, y se puede sacar de aquí sin
//  pedirme permiso. El plan lo dice al revés y con más razón: «exportar y
//  borrar son operaciones explícitas, separadas».

use rusqlite::Connection;
use serde::Serialize;
use std::io::Write;

// ── integridad ───────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct Informe {
    pub sqlite: String,
    //  Capturas cuyo blob no está en el disco. Es el fallo que de verdad
    //  importa: la biblioteca enseña algo que ya no se puede abrir.
    pub missing_blobs: Vec<String>,
    //  Blobs sin dueño. Ocupan sitio y no rompen nada, pero hay que poder
    //  contarlos antes de barrerlos.
    pub orphan_blobs: usize,
    pub orphan_bytes: u64,
    //  Capturas que no están en el índice de búsqueda: guardadas y no
    //  encontrables, que para quien busca es lo mismo que perdidas.
    pub unindexed: Vec<String>,
    //  Blobs cuyo contenido ya no da su hash. Solo con `--deep`.
    pub corrupt_blobs: Vec<String>,
    pub deep: bool,
    pub ok: bool,
}

pub fn integridad(db: &Connection, profundo: bool) -> rusqlite::Result<Informe> {
    let sqlite: String = db.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;

    //  Los que la base dice tener.
    let mut s = db.prepare(
        "SELECT DISTINCT blob_hash FROM captures
          WHERE blob_hash IS NOT NULL AND trashed_at IS NULL",
    )?;
    let esperados: Vec<String> = s
        .query_map([], |r| r.get::<_, String>(0))?
        .filter_map(|x| x.ok())
        .collect();

    let mut faltan = Vec::new();
    let mut rotos = Vec::new();
    for h in &esperados {
        let p = crate::paths::blob_de(h);
        if !p.exists() {
            faltan.push(h.clone());
            continue;
        }
        if profundo {
            match crate::util::sha256_de_fichero(&p) {
                //  Un blob cuyo contenido ya no da su nombre. El nombre ES el
                //  hash, así que esto solo puede ser corrupción del disco o
                //  alguien escribiendo dentro a mano.
                Ok(real) if &real != h => rotos.push(h.clone()),
                Err(_) => rotos.push(h.clone()),
                _ => {}
            }
        }
    }

    let en_disco = crate::blobs::todos().unwrap_or_default();
    let mut huerfanos = 0usize;
    let mut bytes_huerfanos = 0u64;
    for (nombre, _, bytes) in &en_disco {
        if !esperados.iter().any(|h| h == nombre) {
            huerfanos += 1;
            bytes_huerfanos += bytes;
        }
    }

    let mut s = db.prepare(
        "SELECT id FROM captures
          WHERE trashed_at IS NULL
            AND id NOT IN (SELECT capture_id FROM captures_fts)",
    )?;
    let sin_indexar: Vec<String> = s
        .query_map([], |r| r.get::<_, String>(0))?
        .filter_map(|x| x.ok())
        .collect();

    Ok(Informe {
        ok: sqlite == "ok" && faltan.is_empty() && rotos.is_empty() && sin_indexar.is_empty(),
        sqlite,
        missing_blobs: faltan,
        orphan_blobs: huerfanos,
        orphan_bytes: bytes_huerfanos,
        unindexed: sin_indexar,
        corrupt_blobs: rotos,
        deep: profundo,
    })
}

//  Y arreglar lo que se puede arreglar solo: reindexar lo que falte. Los blobs
//  que faltan no se inventan; eso se dice y se deja al usuario.
pub fn reparar_indice(db: &Connection) -> rusqlite::Result<usize> {
    let mut s = db.prepare(
        "SELECT id FROM captures
          WHERE trashed_at IS NULL
            AND id NOT IN (SELECT capture_id FROM captures_fts)",
    )?;
    let ids: Vec<String> = s
        .query_map([], |r| r.get::<_, String>(0))?
        .filter_map(|x| x.ok())
        .collect();
    for id in &ids {
        crate::db::reindexar(db, id)?;
    }
    Ok(ids.len())
}

// ── copia de seguridad ───────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct Copia {
    pub path: String,
    pub bytes: u64,
    pub kept: usize,
}

//  `VACUUM INTO` y no copiar el fichero.
//
//  Con WAL, la base en disco no está completa: parte de lo último escrito vive
//  en `library.sqlite3-wal`. Copiar los tres ficheros a mano mientras alguien
//  escribe da una copia rota que además parece buena. `VACUUM INTO` le pide a
//  SQLite una base entera y consistente en otro sitio, que es exactamente lo
//  que hace falta.
pub fn respaldar(db: &Connection, conservar: usize) -> rusqlite::Result<Copia> {
    crate::paths::preparar().ok();
    let nombre = format!("library-{}.sqlite3", crate::util::ahora_ms());
    let destino = crate::paths::backups().join(&nombre);
    db.execute("VACUUM INTO ?1", [destino.to_string_lossy().to_string()])?;
    let bytes = std::fs::metadata(&destino).map(|m| m.len()).unwrap_or(0);

    //  Y se tiran las viejas. Una carpeta de copias que solo crece acaba
    //  ocupando más que la biblioteca, y entonces alguien la borra entera.
    let mut copias: Vec<std::path::PathBuf> = std::fs::read_dir(crate::paths::backups())
        .map(|d| {
            d.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| {
                    p.file_name()
                        .map(|n| n.to_string_lossy().starts_with("library-"))
                        .unwrap_or(false)
                })
                .collect()
        })
        .unwrap_or_default();
    copias.sort();
    let conservar = conservar.max(1);
    let mut tiradas = 0;
    while copias.len() > conservar {
        let vieja = copias.remove(0);
        if std::fs::remove_file(&vieja).is_ok() {
            tiradas += 1;
        }
    }
    let _ = tiradas;
    Ok(Copia { path: destino.to_string_lossy().to_string(), bytes, kept: copias.len() })
}

// ── exportación ──────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct Exportado {
    pub directory: String,
    pub captures: usize,
    pub files: usize,
    pub bytes: u64,
}

//  Todo lo tuyo, en una carpeta que puedas abrir sin este programa: un JSON
//  Lines con una captura por línea y los ficheros al lado con su nombre real.
//
//  El nombre real y no el hash. Un archivo llamado `9f2a…` es tuyo igual, pero
//  no lo puedes usar; el sentido de exportar es poder irte.
pub fn exportar(db: &Connection, dir: &std::path::Path) -> Result<Exportado, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let ficheros = dir.join("files");
    std::fs::create_dir_all(&ficheros).map_err(|e| e.to_string())?;

    let mut jsonl =
        std::fs::File::create(dir.join("captures.jsonl")).map_err(|e| e.to_string())?;
    let mut n = 0usize;
    let mut copiados = 0usize;
    let mut bytes = 0u64;
    let mut vistos: Vec<String> = Vec::new();

    let mut s = db
        .prepare("SELECT id FROM captures WHERE trashed_at IS NULL ORDER BY captured_at")
        .map_err(|e| e.to_string())?;
    let ids: Vec<String> = s
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?
        .filter_map(|x| x.ok())
        .collect();

    for id in ids {
        let Some(f) = crate::search::una(db, &id).map_err(|e| e.to_string())? else {
            continue;
        };
        //  El texto completo va en la exportación aunque no vaya en la lista:
        //  irse con los extractos no es irse con tus cosas.
        let texto: String = db
            .query_row("SELECT content_text FROM captures WHERE id = ?1", [&id], |r| r.get(0))
            .unwrap_or_default();

        let mut fichero_exportado: Option<String> = None;
        if let Some(h) = &f.blob_hash {
            let origen = crate::paths::blob_de(h);
            if origen.exists() {
                let nombre = nombre_para(&f, h);
                let destino = ficheros.join(&nombre);
                if !vistos.contains(&nombre) {
                    if let Ok(b) = std::fs::copy(&origen, &destino) {
                        copiados += 1;
                        bytes += b;
                        vistos.push(nombre.clone());
                    }
                }
                fichero_exportado = Some(format!("files/{}", nombre));
            }
        }

        let linea = serde_json::json!({
            "id": f.id, "type": f.tipo, "title": f.title, "author": f.author,
            "excerpt": f.excerpt, "note": f.note, "content_text": texto,
            "source_url": f.source_url, "canonical_url": f.canonical_url,
            "captured_at": f.captured_at, "favorite": f.favorite,
            "tags": f.tags, "file": fichero_exportado,
        });
        writeln!(jsonl, "{}", linea).map_err(|e| e.to_string())?;
        n += 1;
    }

    //  Y un manifiesto, para que dentro de tres años se sepa qué es esto.
    let manifiesto = serde_json::json!({
        "producer": "deriva-worker",
        "schema_version": crate::db::VERSION,
        "exported_at": crate::util::ahora_ms(),
        "captures": n,
        "files": copiados,
        "layout": "captures.jsonl · una captura por línea; files/ · los adjuntos",
    });
    std::fs::write(
        dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifiesto).unwrap_or_default(),
    )
    .map_err(|e| e.to_string())?;

    Ok(Exportado {
        directory: dir.to_string_lossy().to_string(),
        captures: n,
        files: copiados,
        bytes,
    })
}

//  Un nombre de fichero que se pueda escribir en cualquier sitio. Lo que trae
//  el usuario puede tener barras, saltos de línea o cuatrocientos caracteres.
fn nombre_para(f: &crate::search::Fila, hash: &str) -> String {
    let bruto = f
        .blob_name
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| f.title.clone());
    let limpio: String = bruto
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .take(80)
        .collect();
    let limpio = limpio.trim_matches('-').to_string();
    //  Con los ocho primeros del hash delante: dos ficheros pueden llamarse
    //  igual y ser distintos, y una exportación que pisa un fichero con otro
    //  pierde datos en silencio.
    if limpio.is_empty() {
        hash[..16].to_string()
    } else {
        format!("{}-{}", &hash[..8], limpio)
    }
}
