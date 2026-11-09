//  Import what you already have.
//
//  Three entry points, all for the same reason: a library filled only by
//  dragging things in one at a time takes a year to become useful, and until
//  then there is no reason to open it.
//
//  All three are EXPLICIT. None runs on its own, watches a directory, or
//  accesses the network. Import is an operation that must be requested, like
//  export; for the same reason, the plan separates both from deletion.
//
//  What none of them do is guess. A file that cannot be read is counted and
//  reported; it is not saved incompletely in the hope that it will be useful.

use rusqlite::Connection;
use serde::Serialize;

#[derive(Debug, Serialize, Default)]
pub struct Resumen {
    pub saved: usize,
    pub duplicates: usize,
    pub skipped: usize,
    //  The first reasons an item was not imported. Not every reason: nobody
    //  reads a report with four thousand lines, and all four thousand say the
    //  same thing.
    pub reasons: Vec<String>,
}

impl Resumen {
    fn contar(&mut self, r: &crate::ingest::Resultado) {
        match r.status.as_str() {
            "saved" => self.saved += 1,
            "duplicate" => self.duplicates += 1,
            _ => {
                self.skipped += 1;
                if self.reasons.len() < 8 {
                    let m = r.reason.clone().unwrap_or_else(|| "sin motivo".into());
                    if !self.reasons.contains(&m) {
                        self.reasons.push(m);
                    }
                }
            }
        }
    }
}

//  ── a directory of files ─────────────────────────────────────────
//
//  Recursive, with limits on depth and item count. Both limits are for safety,
//  not preference: a directory with a link to `/` and no depth limit is an
//  infinite loop, and “import my entire disk” is not an operation anyone
//  actually wants.
pub fn carpeta(db: &Connection, dir: &std::path::Path, tope: usize) -> Resumen {
    let mut r = Resumen::default();
    let mut pendientes = vec![(dir.to_path_buf(), 0usize)];
    let mut vistos = 0usize;

    while let Some((d, hondura)) = pendientes.pop() {
        if hondura > 6 || vistos >= tope {
            break;
        }
        let Ok(entradas) = std::fs::read_dir(&d) else {
            r.skipped += 1;
            continue;
        };
        for e in entradas.filter_map(|x| x.ok()) {
            if vistos >= tope {
                break;
            }
            let p = e.path();
            //  Skip hidden items. Someone importing a directory wants their
            //  files, not its `.git` or `.cache`.
            if p.file_name()
                .map(|n| n.to_string_lossy().starts_with('.'))
                .unwrap_or(false)
            {
                continue;
            }
            //  Use `symlink_metadata`, not `metadata`: a link to a parent
            //  directory is how to accidentally traverse an entire disk.
            let Ok(meta) = std::fs::symlink_metadata(&p) else {
                continue;
            };
            if meta.is_symlink() {
                continue;
            }
            if meta.is_dir() {
                pendientes.push((p, hondura + 1));
                continue;
            }
            vistos += 1;
            let pet = crate::ingest::Peticion {
                tipo: tipo_de(&p),
                paths: vec![p.to_string_lossy().to_string()],
                ..Default::default()
            };
            for x in crate::ingest::ingerir(db, &pet) {
                r.contar(&x);
            }
        }
    }
    r
}

//  Determine the type from its extension. This is the same clue used by the
//  drag router, for the same reason: it selects an icon, not a truth about the
//  content.
fn tipo_de(p: &std::path::Path) -> String {
    let ext = p
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "avif" | "bmp" => "image",
        "mp4" | "webm" | "mkv" | "mov" => "video",
        "mp3" | "flac" | "opus" | "ogg" | "wav" => "audio",
        "pdf" | "epub" | "odt" | "docx" | "rtf" | "md" | "txt" => "document",
        "js" | "mjs" | "ts" | "py" | "rs" | "go" | "c" | "h" | "cpp" | "qml" | "sh" | "json"
        | "yaml" | "yml" | "toml" | "html" | "css" => "code",
        _ => "file",
    }
    .to_string()
}

//  ── browser bookmarks ────────────────────────────────────────────
//
//  The Netscape format, exported by EVERY browser: Chrome, Firefox, Safari, and
//  Zen. It is HTML from 1994 and can be read with two simple expressions. That
//  is exactly why we chose it instead of reading each browser's database: a
//  file you export does not depend on anyone maintaining a schema, and the
//  browser does not need to be closed to read it.
//
//  Do not parse it as HTML. Find the `<A HREF=...>` tags and extract their text;
//  do not execute or render the rest of the file.
pub fn marcadores(db: &Connection, fichero: &std::path::Path, espacio: Option<&str>) -> Resumen {
    let mut r = Resumen::default();
    let Ok(texto) = std::fs::read_to_string(fichero) else {
        r.skipped += 1;
        r.reasons.push("no_se_puede_leer".into());
        return r;
    };

    for (url, titulo) in enlaces(&texto) {
        let mut p = crate::ingest::Peticion {
            tipo: "url".into(),
            source_url: Some(url),
            ..Default::default()
        };
        if !titulo.is_empty() {
            p.title = Some(titulo);
        }
        if let Some(e) = espacio {
            p.space = Some(e.to_string());
        }
        for x in crate::ingest::ingerir(db, &p) {
            r.contar(&x);
        }
    }
    r
}

//  Do it by hand, without a regular-expression crate: this needs two text
//  searches, and a whole crate for that is a dependency to maintain forever.
fn enlaces(html: &str) -> Vec<(String, String)> {
    let mut fuera = Vec::new();
    let bajo = html.to_lowercase();
    let mut i = 0usize;
    while let Some(rel) = bajo[i..].find("<a ") {
        let inicio = i + rel;
        let Some(fin_etiqueta) = bajo[inicio..].find('>') else {
            break;
        };
        let etiqueta = &html[inicio..inicio + fin_etiqueta];
        i = inicio + fin_etiqueta + 1;

        let Some(url) = atributo(etiqueta, "href") else {
            continue;
        };
        //  Web only. A `javascript:` bookmark is a “bookmarklet” —code
        //  executed by the browser— and has no place in a library of things
        //  to read.
        if !url.starts_with("http://") && !url.starts_with("https://") {
            continue;
        }
        //  The text up to the closing tag.
        let titulo = match bajo[i..].find("</a>") {
            Some(n) => sin_etiquetas(&html[i..i + n]),
            None => String::new(),
        };
        fuera.push((url, titulo));
    }
    fuera
}

fn atributo(etiqueta: &str, nombre: &str) -> Option<String> {
    let bajo = etiqueta.to_lowercase();
    let clave = format!("{}=", nombre);
    let pos = bajo.find(&clave)? + clave.len();
    let resto = &etiqueta[pos..];
    let (comilla, resto) = match resto.chars().next()? {
        c @ ('"' | '\'') => (c, &resto[1..]),
        _ => return resto.split_whitespace().next().map(|s| s.to_string()),
    };
    resto.find(comilla).map(|n| resto[..n].to_string())
}

//  The inner text, without markup, decoding the four entities that actually
//  appear in a bookmark file.
fn sin_etiquetas(s: &str) -> String {
    let mut fuera = String::new();
    let mut dentro = false;
    for c in s.chars() {
        match c {
            '<' => dentro = true,
            '>' => dentro = false,
            _ if !dentro => fuera.push(c),
            _ => {}
        }
    }
    fuera
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .trim()
        .chars()
        .take(200)
        .collect()
}

//  ── a Deriva export ──────────────────────────────────────────────
//
//  The counterpart to `export`. It turns “you can leave” into “you can come
//  back”: an export that cannot be reimported is a dead archive with a pretty
//  format, and we cannot know whether it is complete until the round trip is
//  tested.
pub fn desde_export(db: &Connection, dir: &std::path::Path) -> Resumen {
    let mut r = Resumen::default();
    let jsonl = dir.join("captures.jsonl");
    let Ok(texto) = std::fs::read_to_string(&jsonl) else {
        r.skipped += 1;
        r.reasons.push("sin_captures_jsonl".into());
        return r;
    };

    for linea in texto.lines() {
        if linea.trim().is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(linea) else {
            r.skipped += 1;
            if r.reasons.len() < 8 && !r.reasons.iter().any(|x| x == "linea_ilegible") {
                r.reasons.push("linea_ilegible".into());
            }
            continue;
        };
        let cadena = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
        let mut p = crate::ingest::Peticion {
            tipo: {
                let t = cadena("type");
                if crate::ingest::TIPOS.contains(&t.as_str()) {
                    t
                } else {
                    "file".into()
                }
            },
            title: Some(cadena("title")).filter(|x| !x.is_empty()),
            author: Some(cadena("author")).filter(|x| !x.is_empty()),
            excerpt: Some(cadena("excerpt")).filter(|x| !x.is_empty()),
            note: Some(cadena("note")).filter(|x| !x.is_empty()),
            text: Some(cadena("content_text")).filter(|x| !x.is_empty()),
            source_url: Some(cadena("source_url")).filter(|x| !x.is_empty()),
            tags: v
                .get("tags")
                .and_then(|x| x.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|t| t.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default(),
            ..Default::default()
        };
        //  Include the attachment too, if one exists and is still alongside
        //  it. The path comes from the manifest and is resolved INSIDE the
        //  exported directory: a path pointing outside would let an import
        //  file choose what to copy from your disk.
        let fichero = cadena("file");
        if !fichero.is_empty() && !fichero.contains("..") && !fichero.starts_with('/') {
            let ruta = dir.join(&fichero);
            if ruta.is_file() {
                p.paths = vec![ruta.to_string_lossy().to_string()];
            }
        }
        for x in crate::ingest::ingerir(db, &p) {
            r.contar(&x);
        }
    }
    r
}
