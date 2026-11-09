//  Save something. This is the core operation.
//
//  A request can contain several items —a batch of files— and produces one
//  capture for each, with its own result. Reporting the entire batch as one
//  “success” or “failure” would be misleading in the common case: of sixty
//  dragged files, fifty-nine new and one duplicate is not a failure.
//
//  The steps follow the plan's order, which is not negotiable:
//
//      identify → hash → check whether it already exists → write the blob →
//      commit to SQLite → index
//
//  Blob before row. An interruption between the two leaves, at worst, orphaned
//  bytes on disk; the reverse would leave a visible capture pointing to a
//  nonexistent file, which cannot be repaired by reading the database.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

//  The types that can be saved. Deliberately closed: `type` determines icons
//  and filters in the interface, and an open list would let any client invent
//  a category the page cannot render.
pub const TIPOS: &[&str] = &[
    "text", "url", "image", "document", "code", "video", "audio", "file",
];

//  Capture text has a limit. Not because of disk space —SQLite can handle it—
//  but because all of `content_text` goes into the search index and every
//  result that returns it.
pub const TOPE_TEXTO: usize = 4 * 1024 * 1024;

#[derive(Debug, Deserialize, Default)]
pub struct Peticion {
    #[serde(rename = "type")]
    pub tipo: String,
    pub source_url: Option<String>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub excerpt: Option<String>,
    pub note: Option<String>,
    pub text: Option<String>,
    //  Files from the user's disk. One capture for each.
    #[serde(default)]
    pub paths: Vec<String>,
    //  Or inline bytes for an item without a file: an image dragged from a
    //  page.
    pub inline: Option<EnLinea>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub space: Option<String>,
    //  For resurfacing: if provided, the capture stays dormant until that
    //  date.
    pub snoozed_until: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct EnLinea {
    pub mime: Option<String>,
    pub name: Option<String>,
    pub base64: String,
}

#[derive(Debug, Serialize)]
pub struct Resultado {
    pub status: String, //  saved · duplicate · rejected · failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    //  The saved source address, if any; use the canonical one when
    //  available.
    //
    //  This lets the caller fetch the thumbnail immediately after saving
    //  without requesting the entire capture again: it knows the ID, but
    //  the ID does not say which page to visit, and a `get` just to read a
    //  field the caller already had would be a wasted round trip.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    //  Also return where the file ended up, for the same reason: it can be
    //  opened and its text extracted without waiting for the next review. A
    //  PDF you just dropped in should be searchable by its contents right
    //  away.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duplicate_of: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl Resultado {
    fn guardado(id: String) -> Self {
        Resultado {
            status: "saved".into(),
            id: Some(id),
            url: None,
            path: None,
            duplicate_of: None,
            reason: None,
        }
    }
    fn duplicado(de: String) -> Self {
        Resultado {
            status: "duplicate".into(),
            id: None,
            url: None,
            path: None,
            duplicate_of: Some(de),
            reason: None,
        }
    }
    fn rechazado(por: &str) -> Self {
        Resultado {
            status: "rejected".into(),
            id: None,
            url: None,
            path: None,
            duplicate_of: None,
            reason: Some(por.into()),
        }
    }
    fn fallido(por: String) -> Self {
        Resultado {
            status: "failed".into(),
            id: None,
            url: None,
            path: None,
            duplicate_of: None,
            reason: Some(por),
        }
    }
}

pub fn ingerir(db: &Connection, p: &Peticion) -> Vec<Resultado> {
    if !TIPOS.contains(&p.tipo.as_str()) {
        return vec![Resultado::rechazado("tipo_desconocido")];
    }
    let mut fuera = Vec::new();

    //  Process files one by one.
    for ruta in &p.paths {
        fuera.push(match una_de_fichero(db, p, ruta) {
            Ok(r) => r,
            Err(e) => Resultado::fallido(e),
        });
    }

    //  Also handle items not provided as files: inline bytes or text. Only
    //  if there were no files; a drag provides one or the other, and saving
    //  both would duplicate the same capture in two different forms.
    if p.paths.is_empty() {
        fuera.push(match una_suelta(db, p) {
            Ok(r) => r,
            Err(e) => Resultado::fallido(e),
        });
    }
    fuera
}

// ── a capture from a file ────────────────────────────────────────

fn una_de_fichero(db: &Connection, p: &Peticion, ruta: &str) -> Result<Resultado, String> {
    let real = match resolver(ruta) {
        Ok(r) => r,
        Err(e) => return Ok(Resultado::rechazado(e)),
    };
    let hash = crate::util::sha256_de_fichero(&real).map_err(|e| e.to_string())?;

    if let Some(id) = ya_estaba(db, None, None, Some(&hash)).map_err(|e| e.to_string())? {
        return Ok(Resultado::duplicado(id));
    }

    //  Write the blob first.
    let g = crate::blobs::guardar_fichero(&real).map_err(|e| e.to_string())?;
    let nombre = real
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    //  Determine the type from the extension. It is a CLUE for rendering —
    //  should this be shown as a thumbnail or an icon?—not a truth about
    //  the content; whoever opens it determines that. Without it, a photo
    //  saved from the file manager appeared with a generic “file” icon, the
    //  most useless thing to put on top of a photo.
    let mime = mime_por_extension(&nombre);
    let titulo = p.title.clone().unwrap_or_else(|| nombre.clone());

    let id = escribir(
        db,
        Nueva {
            tipo: &p.tipo,
            source_url: Some(&format!("file://{}", real.display())),
            canonical: None,
            title: &titulo,
            author: p.author.as_deref().unwrap_or(""),
            excerpt: p.excerpt.as_deref().unwrap_or(""),
            note: p.note.as_deref().unwrap_or(""),
            content_text: "",
            content_hash: None,
            blob_hash: Some(&g.hash),
            blob_bytes: Some(g.bytes as i64),
            blob_mime: mime,
            blob_name: Some(&nombre),
            snoozed_until: p.snoozed_until,
            tags: &p.tags,
            space: p.space.as_deref(),
            fuente: "",
        },
    )
    .map_err(|e| e.to_string())?;
    //  A `purge` of the same bytes may have taken the file between the
    //  moment it was found already there and this row: put it back.
    if !crate::paths::blob_de(&g.hash).exists() {
        crate::blobs::guardar_fichero(&real).map_err(|e| e.to_string())?;
    }
    let mut r = Resultado::guardado(id);
    //  Where it ended up, so its contents can be inspected without waiting
    //  for the next review: a newly dropped PDF should be searchable today.
    r.path = Some(crate::paths::blob_de(&g.hash).to_string_lossy().to_string());
    Ok(r)
}

//  A path that can be copied, or why it cannot.
//
//  Resolve symlinks BEFORE inspecting anything: the plan requires it, and
//  without this, a symlink to `/etc/shadow` named like a photo would be copied
//  into the library without a second thought.
fn resolver(ruta: &str) -> Result<std::path::PathBuf, &'static str> {
    let bruta = std::path::Path::new(ruta.trim_start_matches("file://"));
    if !bruta.is_absolute() {
        return Err("ruta_relativa");
    }
    let real = std::fs::canonicalize(bruta).map_err(|_| "no_existe")?;
    let meta = std::fs::metadata(&real).map_err(|_| "no_existe")?;
    if !meta.is_file() {
        return Err("no_es_un_fichero");
    }
    if meta.len() > crate::blobs::TOPE_FICHERO {
        return Err("demasiado_grande");
    }
    //  Filesystem entries that are not regular files. Copying from `/proc`
    //  gives zero-size files whose contents change while being read.
    for prohibido in ["/proc", "/sys", "/dev"] {
        if real.starts_with(prohibido) {
            return Err("ruta_del_sistema");
        }
    }
    //  Do not let the library copy itself.
    if real.starts_with(crate::paths::base()) {
        return Err("ya_es_de_la_biblioteca");
    }
    Ok(real)
}

// ── a capture that is not from a file ────────────────────────────

//  Is its title one of the fallback titles?
//
//  Needed for enrichment: the page's actual title can replace “example.com,”
//  but not a title provided by the drag —the browser read it from the same
//  page—or, of course, one entered by hand.
//
//  Check by recalculating the fallback and comparing it. This is more work
//  than storing a boolean column, but it is the right approach: a boolean
//  would need to be migrated, kept up to date, and trusted; this cannot get out
//  of sync with the rule because it IS the rule.
pub fn titulo_de_emergencia(titulo: &str, origen: Option<&str>, canonica: Option<&str>) -> bool {
    let t = titulo.trim();
    if t.is_empty() {
        return true;
    }
    let Some(u) = origen else {
        return false;
    };
    if !u.starts_with("http") {
        return false;
    }
    //  The address as-is, in either of its forms.
    if t == u || canonica == Some(t) {
        return true;
    }
    //  Also include what the adapter would derive from that URL today: the
    //  domain, thread slug, “@user,” “owner/repo,” or filename.
    crate::adaptadores::leer(u).titulo.as_deref() == Some(t)
}

fn una_suelta(db: &Connection, p: &Peticion) -> Result<Resultado, String> {
    //  Where it came from. Do not access the network: this is everything
    //  that can be learned from the URL, which is quite a lot. Anything
    //  that must be fetched externally —the actual title or thumbnail—is
    //  fetched by the host, which has reviewed anti-SSRF protections.
    let leido = p
        .source_url
        .as_deref()
        .filter(|u| u.starts_with("http"))
        .map(crate::adaptadores::leer);

    //  The adapter's canonical URL takes precedence over the generic one:
    //  `youtu.be/ID` and `youtube.com/watch?v=ID&t=90` are the same video,
    //  which the general rule cannot know. Without this, the same video
    //  from a phone and desktop becomes two captures, and duplicate
    //  detection misses it.
    let canonical = leido
        .as_ref()
        .and_then(|l| l.canonica.clone())
        .or_else(|| p.source_url.as_deref().and_then(crate::util::canonizar_url));
    let texto = p.text.as_deref().unwrap_or("");
    if texto.len() > TOPE_TEXTO {
        return Ok(Resultado::rechazado("texto_demasiado_largo"));
    }

    //  Inline bytes, if any.
    let mut blob: Option<crate::blobs::Guardado> = None;
    if let Some(en_linea) = &p.inline {
        let datos = match crate::util::base64(&en_linea.base64) {
            Some(d) => d,
            None => return Ok(Resultado::rechazado("base64_invalido")),
        };
        if datos.is_empty() {
            return Ok(Resultado::rechazado("sin_datos"));
        }
        if datos.len() > crate::blobs::TOPE_EN_LINEA {
            return Ok(Resultado::rechazado("demasiado_grande"));
        }
        let hash = crate::util::sha256(&datos);
        if let Some(id) =
            ya_estaba(db, canonical.as_deref(), None, Some(&hash)).map_err(|e| e.to_string())?
        {
            return Ok(Resultado::duplicado(id));
        }
        blob = Some(crate::blobs::guardar_bytes(&datos).map_err(|e| e.to_string())?);
    }

    let content_hash = if texto.trim().is_empty() {
        None
    } else {
        Some(crate::util::hash_de_texto(texto))
    };

    //  Nothing to save is not a failure; there was simply nothing there.
    if blob.is_none() && content_hash.is_none() && canonical.is_none() {
        return Ok(Resultado::rechazado("sin_contenido"));
    }

    if let Some(id) = ya_estaba(db, canonical.as_deref(), content_hash.as_deref(), None)
        .map_err(|e| e.to_string())?
    {
        return Ok(Resultado::duplicado(id));
    }

    //  The default title: what the adapter provides, the first line of
    //  text, or the URL, in that order. “@user” or “owner/repo” say more
    //  than a full address, and a full address says more than nothing.
    let titulo = p.title.clone().unwrap_or_else(|| {
        leido
            .as_ref()
            .and_then(|l| l.titulo.clone())
            .or_else(|| texto.lines().next().map(|l| recortar(l, 120)))
            .or_else(|| canonical.clone())
            .unwrap_or_default()
    });
    //  If no excerpt is provided, use the beginning of the text. This
    //  supplies the card without reading all of `content_text`.
    let extracto = p
        .excerpt
        .clone()
        .unwrap_or_else(|| recortar(texto.trim(), 280));

    //  The adapter's type overrides only the generic type. If the caller
    //  already said “document,” it knows more than we do; if it said “url,”
    //  that is what any dragged link says, and the adapter knows more: a
    //  YouTube link is a video even if the URL does not end in `.mp4`.
    let tipo = leido
        .as_ref()
        .filter(|_| p.tipo == "url")
        .map(|l| l.fuente.tipo.to_string())
        .unwrap_or_else(|| p.tipo.clone());
    let fuente = leido.as_ref().map(|l| l.fuente.id).unwrap_or("");

    let g = blob.as_ref();
    let id = escribir(
        db,
        Nueva {
            tipo: &tipo,
            source_url: p.source_url.as_deref(),
            canonical: canonical.as_deref(),
            title: &titulo,
            author: p.author.as_deref().unwrap_or(""),
            excerpt: &extracto,
            note: p.note.as_deref().unwrap_or(""),
            content_text: texto,
            content_hash: content_hash.as_deref(),
            blob_hash: g.map(|x| x.hash.as_str()),
            blob_bytes: g.map(|x| x.bytes as i64),
            blob_mime: p.inline.as_ref().and_then(|x| x.mime.as_deref()),
            blob_name: p.inline.as_ref().and_then(|x| x.name.as_deref()),
            snoozed_until: p.snoozed_until,
            tags: &p.tags,
            space: p.space.as_deref(),
            fuente: fuente,
        },
    )
    .map_err(|e| e.to_string())?;
    //  A `purge` of the same bytes may have taken the file between the
    //  moment it was found already there and this row: put it back.
    if let (Some(g), Some(en_linea)) = (blob.as_ref(), &p.inline) {
        if !crate::paths::blob_de(&g.hash).exists() {
            if let Some(d) = crate::util::base64(&en_linea.base64) {
                crate::blobs::guardar_bytes(&d).map_err(|e| e.to_string())?;
            }
        }
    }
    let mut r = Resultado::guardado(id);
    //  Prefer the canonical URL over the source URL: it is the one to visit,
    //  while the source may contain a long string of campaign parameters.
    r.url = canonical.clone().or_else(|| p.source_url.clone());
    Ok(r)
}

//  Just enough to know whether it can be displayed. Deliberately closed: this
//  is not a type detector, but a list of what this interface can render.
fn mime_por_extension(nombre: &str) -> Option<&'static str> {
    let n = nombre.to_lowercase();
    let ext = n
        .rsplit_once('.')
        .map(|(_, e)| e.to_string())
        .unwrap_or_default();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "pdf" => "application/pdf",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "mp3" => "audio/mpeg",
        "flac" => "audio/flac",
        "opus" | "ogg" => "audio/ogg",
        "txt" | "md" => "text/plain",
        _ => return None,
    })
}

fn recortar(s: &str, n: usize) -> String {
    //  Count characters, not bytes: cutting UTF-8 in half breaks it.
    let mut fuera: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        fuera.push('…');
    }
    fuera
}

// ── existing items ───────────────────────────────────────────────
//
//  The plan says “the same `canonical_url` or hash exists.” Check all three in
//  one place and in this order: the URL takes precedence over content because
//  a page can change its text between visits and still be the same page.
//
//  Trashed items do not count. Saving something you deleted must work again,
//  or deletion would be a life sentence.
fn ya_estaba(
    db: &Connection,
    canonical: Option<&str>,
    content_hash: Option<&str>,
    blob_hash: Option<&str>,
) -> rusqlite::Result<Option<String>> {
    for (columna, valor) in [
        ("canonical_url", canonical),
        ("content_hash", content_hash),
        ("blob_hash", blob_hash),
    ] {
        let Some(v) = valor else { continue };
        if v.is_empty() {
            continue;
        }
        let sql = format!(
            "SELECT id FROM captures WHERE {} = ?1 AND trashed_at IS NULL LIMIT 1",
            columna
        );
        if let Some(id) = db
            .query_row(&sql, [v], |r| r.get::<_, String>(0))
            .optional()?
        {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

// ── write the row ─────────────────────────────────────────────────

struct Nueva<'a> {
    tipo: &'a str,
    source_url: Option<&'a str>,
    canonical: Option<&'a str>,
    title: &'a str,
    author: &'a str,
    excerpt: &'a str,
    note: &'a str,
    content_text: &'a str,
    content_hash: Option<&'a str>,
    blob_hash: Option<&'a str>,
    blob_bytes: Option<i64>,
    blob_mime: Option<&'a str>,
    blob_name: Option<&'a str>,
    snoozed_until: Option<i64>,
    tags: &'a [String],
    space: Option<&'a str>,
    //  Source from the adapter. Empty for items that do not come from a URL.
    fuente: &'a str,
}

fn escribir(db: &Connection, n: Nueva) -> rusqlite::Result<String> {
    let id = crate::util::nuevo_id();
    //  Everything in one transaction: the row, its tags, its folder, and
    //  its index entry.
    //  A partial capture —saved but not indexed—is a capture that cannot be
    //  found, which is the same as a lost capture to the user.
    let tx = db.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO captures
           (id, type, source_url, canonical_url, title, author, excerpt, note,
            content_text, content_hash, blob_hash, blob_bytes, blob_mime, blob_name,
            status, captured_at, snoozed_until, source)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)",
        params![
            id,
            n.tipo,
            n.source_url,
            n.canonical,
            n.title,
            n.author,
            n.excerpt,
            n.note,
            n.content_text,
            n.content_hash,
            n.blob_hash,
            n.blob_bytes,
            n.blob_mime,
            n.blob_name,
            "saved",
            crate::util::ahora_ms(),
            n.snoozed_until,
            n.fuente,
        ],
    )?;
    for t in n.tags {
        poner_etiqueta(&tx, &id, t)?;
    }
    if let Some(s) = n.space {
        poner_espacio(&tx, &id, s)?;
    }
    crate::db::reindexar(&tx, &id)?;
    tx.commit()?;
    Ok(id)
}

pub fn poner_etiqueta(db: &Connection, capture: &str, nombre: &str) -> rusqlite::Result<()> {
    let limpio = nombre.trim().to_lowercase();
    if limpio.is_empty() {
        return Ok(());
    }
    //  Create the tag if it does not exist. Its ID is the normalized name:
    //  two tags with the same name are the same tag, whereas random IDs
    //  would require checks everywhere tags are written.
    db.execute(
        "INSERT OR IGNORE INTO tags (id, name) VALUES (?1, ?1)",
        [&limpio],
    )?;
    db.execute(
        "INSERT OR IGNORE INTO capture_tags (capture_id, tag_id) VALUES (?1, ?2)",
        params![capture, limpio],
    )?;
    Ok(())
}

//  A folder ID derived from its name.
//
//  Lowercase, without accents, and hyphenated: “Home stuff” → `home-stuff`.
//  This is needed because the ID travels in page filters and command-line
//  arguments, and an ID with spaces and accents would need escaping everywhere
//  it goes.
//
//  Once assigned, it does NOT change when the folder is renamed: that makes a
//  rename a rename rather than creating another folder.
pub fn id_de_espacio(nombre: &str) -> String {
    let mut fuera = String::new();
    let mut guion = false;
    for c in nombre.trim().to_lowercase().chars() {
        let c = match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            otro => otro,
        };
        if c.is_ascii_alphanumeric() {
            fuera.push(c);
            guion = false;
        } else if !guion && !fuera.is_empty() {
            fuera.push('-');
            guion = true;
        }
    }
    while fuera.ends_with('-') {
        fuera.pop();
    }
    fuera.chars().take(48).collect()
}

//  Put a capture in a folder, creating the folder if needed.
//
//  Accept either the ID or the name: the page caller sends the ID from the
//  list, while an AI tool caller sends “read later,” as a person would write
//  it. Both must work, and checking the ID first prevents “Read later” from
//  creating a new folder beside the existing one.
pub fn poner_espacio(db: &Connection, capture: &str, nombre: &str) -> rusqlite::Result<()> {
    let limpio = nombre.trim();
    if limpio.is_empty() {
        return Ok(());
    }
    let id = id_de_espacio(limpio);
    if id.is_empty() {
        return Ok(());
    }
    //  If a folder with that ID already exists, use it and keep its name;
    //  otherwise, moving something to “read-later” would rename the folder
    //  to “read-later.”
    db.execute(
        "INSERT OR IGNORE INTO spaces (id, name) VALUES (?1, ?2)",
        params![id, limpio],
    )?;
    db.execute(
        "INSERT OR IGNORE INTO capture_spaces (capture_id, space_id) VALUES (?1, ?2)",
        params![capture, id],
    )?;
    Ok(())
}
