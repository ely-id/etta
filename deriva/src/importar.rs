//  Traerse lo que ya tenías.
//
//  Tres puertas, y las tres existen por la misma razón: una biblioteca que solo
//  se llena arrastrando cosas de una en una tarda un año en ser útil, y hasta
//  entonces no hay motivo para abrirla.
//
//  Y las tres son EXPLÍCITAS. Ninguna se ejecuta sola, ninguna vigila una
//  carpeta y ninguna sale a la red. Importar es una operación que se pide, como
//  exportar; el plan las separa de borrar por lo mismo.
//
//  Lo que NO hace ninguna: adivinar. Un fichero que no se sabe leer se cuenta y
//  se dice; no se guarda a medias con la esperanza de que sirva.

use rusqlite::Connection;
use serde::Serialize;

#[derive(Debug, Serialize, Default)]
pub struct Resumen {
    pub saved: usize,
    pub duplicates: usize,
    pub skipped: usize,
    //  Los primeros motivos por los que algo no entró. No todos: un informe con
    //  cuatro mil líneas no lo lee nadie, y las cuatro mil dicen lo mismo.
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

//  ── una carpeta de ficheros ──────────────────────────────────────
//
//  Recursiva, con tope de profundidad y de cuántos. Los dos topes son de
//  seguridad y no de gusto: una carpeta con un enlace a `/` y sin tope de
//  profundidad es un bucle infinito, y «importar mi disco entero» no es una
//  operación que nadie quiera de verdad.
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
            //  Los ocultos no. Quien importa una carpeta quiere sus cosas, no
            //  su `.git` ni su `.cache`.
            if p.file_name()
                .map(|n| n.to_string_lossy().starts_with('.'))
                .unwrap_or(false)
            {
                continue;
            }
            //  `symlink_metadata` y no `metadata`: un enlace a un directorio de
            //  más arriba es como se recorre un disco entero sin querer.
            let Ok(meta) = std::fs::symlink_metadata(&p) else { continue };
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

//  De qué es, por su extensión. La misma pista que usa el enrutador de
//  arrastres, y por la misma razón: es para elegir el icono, no una verdad
//  sobre el contenido.
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
        "js" | "mjs" | "ts" | "py" | "rs" | "go" | "c" | "h" | "cpp" | "qml" | "sh"
        | "json" | "yaml" | "yml" | "toml" | "html" | "css" => "code",
        _ => "file",
    }
    .to_string()
}

//  ── marcadores de navegador ──────────────────────────────────────
//
//  El formato Netscape, que es el que exportan TODOS: Chrome, Firefox, Safari,
//  Zen. Es HTML de 1994 y se lee con dos expresiones tontas, y esa es
//  exactamente la razón de elegirlo en vez de leer la base de datos de cada
//  navegador: un fichero que exportas tú no depende de que nadie mantenga un
//  esquema, y no hace falta cerrar el navegador para leerlo.
//
//  No se interpreta como HTML. Se buscan las etiquetas `<A HREF=...>` y se saca
//  su texto; el resto del fichero no se ejecuta ni se representa.
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

//  A mano y sin caja de expresiones regulares: son dos búsquedas de texto y una
//  caja entera para esto es una dependencia que hay que mantener para siempre.
fn enlaces(html: &str) -> Vec<(String, String)> {
    let mut fuera = Vec::new();
    let bajo = html.to_lowercase();
    let mut i = 0usize;
    while let Some(rel) = bajo[i..].find("<a ") {
        let inicio = i + rel;
        let Some(fin_etiqueta) = bajo[inicio..].find('>') else { break };
        let etiqueta = &html[inicio..inicio + fin_etiqueta];
        i = inicio + fin_etiqueta + 1;

        let Some(url) = atributo(etiqueta, "href") else { continue };
        //  Solo web. Un marcador a `javascript:` es un «bookmarklet» —código
        //  que el navegador ejecuta— y no tiene nada que hacer en una
        //  biblioteca de cosas leídas.
        if !url.starts_with("http://") && !url.starts_with("https://") {
            continue;
        }
        //  El texto hasta el cierre.
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

//  El texto de dentro, sin marcas y con las cuatro entidades que salen de
//  verdad en un fichero de marcadores.
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

//  ── una exportación de Deriva ────────────────────────────────────
//
//  La vuelta de `export`. Es lo que convierte «puedes irte» en «puedes volver»:
//  una exportación que no se puede reimportar es un archivo muerto con formato
//  bonito, y hasta que no se prueba el viaje de ida y vuelta no se sabe si lo
//  exportado estaba completo.
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
                if crate::ingest::TIPOS.contains(&t.as_str()) { t } else { "file".into() }
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
        //  Y el adjunto, si lo hubiera y siguiera al lado. La ruta viene del
        //  manifiesto y se resuelve DENTRO de la carpeta exportada: una ruta que
        //  apuntara fuera sería una manera de que un fichero de importación
        //  eligiera qué copiar de tu disco.
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
