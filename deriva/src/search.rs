//  Search and read what was found.
//
//  FTS5 with BM25, as required by the plan: literal terms, phrases, prefixes,
//  and snippets. Semantic search is a later, optional index and does not
//  belong here; adding “sort of” search now would be worse than not having it,
//  because people learn what the search box can do on day one.
//
//  This file takes seriously that **user input is not an FTS5 query**. An
//  apostrophe, a stray asterisk, or an unclosed quote is syntax in that
//  language, and passing it through as-is produces a syntax error instead of
//  results. Searching for `it's` must not fail.

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Fila {
    pub id: String,
    #[serde(rename = "type")]
    pub tipo: String,
    pub title: String,
    pub author: String,
    pub excerpt: String,
    pub note: String,
    pub source_url: Option<String>,
    pub canonical_url: Option<String>,
    pub blob_hash: Option<String>,
    pub blob_bytes: Option<i64>,
    pub blob_mime: Option<String>,
    pub blob_name: Option<String>,
    //  The file's fully resolved path. The `blobs/ab/cd/<hash>` rule lives
    //  in one place, and the thumbnail renderer does not need to know it:
    //  duplicating it in QML means it will be duplicated incorrectly when
    //  it changes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob_path: Option<String>,
    //  The thumbnail, if it has one. Resolve it the same way as above and
    //  for the same reason: the database stores the hash, not an expiring
    //  path.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview_path: Option<String>,
    pub status: String,
    //  The source from the adapter. The interface uses it to show what it
    //  is —a YouTube video, a Reddit thread—instead of “a link.”
    pub source: String,
    //  What the AI understood, if you asked it. Separate from the note —
    //  that is yours—and the excerpt —that is the original.
    pub summary: String,
    pub captured_at: i64,
    pub last_opened_at: Option<i64>,
    pub snoozed_until: Option<i64>,
    pub favorite: bool,
    pub tags: Vec<String>,
    //  The snippet where the search term matches, with the matching part
    //  highlighted. Only for searches: there is nothing to highlight in a
    //  listing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
}

//  Convert the input into a query FTS5 understands.
//
//  Quote each word —so it becomes text, not syntax—and append `*` for prefix
//  matching: typing “libr” finds “library” without waiting for you to
//  finish. Text enclosed in quotes by the user is preserved as an exact phrase
//  and does not get a prefix.
pub fn consulta_fts(bruta: &str) -> Option<String> {
    let mut partes: Vec<String> = Vec::new();
    let mut resto = bruta.trim();
    while !resto.is_empty() {
        if let Some(sin) = resto.strip_prefix('"') {
            //  A phrase. Read up to the next quote, or to the end if it was
            //  left open —which is normal while typing.
            let (frase, cola) = match sin.find('"') {
                Some(i) => (&sin[..i], &sin[i + 1..]),
                None => (sin, ""),
            };
            let limpia = frase.replace('"', "");
            if !limpia.trim().is_empty() {
                partes.push(format!("\"{}\"", limpia.trim()));
            }
            resto = cola.trim_start();
            continue;
        }
        let (palabra, cola) = match resto.find(char::is_whitespace) {
            Some(i) => (&resto[..i], &resto[i..]),
            None => (resto, ""),
        };
        //  No syntax: only letters, numbers, and characters inside a word.
        //  Discard the rest rather than escaping it, because incorrectly
        //  escaping a query language is how injections get through.
        let limpia: String = palabra
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
            .collect();
        if !limpia.is_empty() {
            partes.push(format!("\"{}\"*", limpia));
        }
        resto = cola.trim_start();
    }
    if partes.is_empty() {
        None
    } else {
        //  No operator between them: FTS5 treats that as an implicit AND, which
        //  is what someone entering two words expects.
        Some(partes.join(" "))
    }
}

//  Words that do not say much on their own. Used only for fallback search; see
//  `consulta_fts_floja` for why.
//
//  Without accents because that is how they arrive: the index tokenizer
//  removes them, and these are compared against its output.
const DE_RELLENO: &[&str] = &[
    //  Spanish: questions about saved things almost always use these words.
    "algo",
    "algun",
    "alguna",
    "alguno",
    "algunos",
    "algunas",
    "como",
    "cosa",
    "cosas",
    "cual",
    "cuales",
    "cuando",
    "donde",
    "esta",
    "este",
    "esto",
    "estos",
    "estas",
    "guarde",
    "guardado",
    "guardada",
    "hace",
    "hacer",
    "para",
    "pero",
    "porque",
    "pues",
    "sobre",
    "tenia",
    "tengo",
    "tiene",
    "todo",
    "todos",
    "todas",
    "unos",
    "unas",
    "acerca",
    "busca",
    "buscar",
    "encuentra",
    "quiero",
    "puedes",
    "podrias",
    "dime",
    "sabes",
    "recuerda",
    "recuerdas",
    //  English: half of the saved content is in English, and queries are
    //  sometimes phrased that way.
    "about",
    "anything",
    "does",
    "have",
    "something",
    "that",
    "the",
    "there",
    "these",
    "this",
    "those",
    "what",
    "when",
    "where",
    "which",
    "with",
    "your",
    "find",
    "search",
    "show",
    "tell",
    "remember",
    "some",
    "from",
    "into",
];

//  The same query, but matching ANY of the words.
//
//  The implicit AND is correct as long as it finds something, and the worst
//  possible behavior when it does not: with four words, just one missing word
//  makes the answer “nothing saved looks like that,” which is false. This
//  happens just when there is the most content, because the larger the library,
//  the longer the query.
pub fn consulta_fts_floja(bruta: &str) -> Option<String> {
    let q = consulta_fts(bruta)?;
    //  A single word was already searched as-is; repeating it finds nothing
    //  new.
    if !q.contains(' ') {
        return None;
    }
    //  Remove words that do not say much.
    //
    //  They are poison in an OR: each one matches independently, so “of” —
    //  which appears in everything you save—would return the entire
    //  library. They did not matter with AND because every word had to match.
    //
    //  Remove them for two reasons. SHORT words of three letters or less
    //  rarely carry meaning; there is some cost —“go” and “rs” are real
    //  tags—but they are lost only in fallback search.
    //
    //  Also remove FILLER words, which are long in Spanish: “tengo,” “sobre,”
    //  “algo,” “cuando.” Without them, asking “do I have anything about
    //  wallpapers?” would return half the library because of “have” and
    //  “anything.” The list is short, covers the two languages used here,
    //  and lives HERE rather than in the AND query because they do not get
    //  in the way there: if someone quotes “about,” they are searching for
    //  it literally.
    let largas: Vec<&str> = q
        .split(' ')
        .filter(|t| {
            //  Fold to lowercase and remove accents before comparing: the
            //  list is written without them because that is how the index
            //  returns words. Without this, “tenía” differs from “tenia”
            //  and still slips through.
            let limpia = crate::util::plegar(
                &t.chars()
                    .filter(|c| c.is_alphanumeric())
                    .collect::<String>(),
            );
            limpia.chars().count() > 3 && !DE_RELLENO.contains(&limpia.as_str())
        })
        .collect();
    //  If none remain, search them all again: more is better than nothing.
    let piezas: Vec<&str> = if largas.len() >= 2 {
        largas
    } else {
        q.split(' ').collect()
    };
    if piezas.len() < 2 {
        return None;
    }
    Some(piezas.join(" OR "))
}

//  The same query with whole words: without the prefix `*`.
pub fn consulta_fts_exacta(bruta: &str) -> Option<String> {
    Some(consulta_fts(bruta)?.replace("\"*", "\""))
}

pub fn buscar(db: &Connection, texto: &str, limite: usize) -> rusqlite::Result<Vec<Fila>> {
    let Some(q) = consulta_fts(texto) else {
        return Ok(Vec::new());
    };
    //  Whole-word matches first, followed by matches that only share a
    //  prefix.
    //
    //  Prefix matching is for words still being typed. It used to make
    //  “pan” match equally in “homemade bread” and “screenshot.png,” which
    //  ranked first because its title was shorter. While the last word is
    //  incomplete, exact search finds nothing and all results come from
    //  the prefix, so typing still finds matches letter by letter.
    let mut filas = match consulta_fts_exacta(texto) {
        Some(e) if e != q => buscar_con(db, &e, limite)?,
        _ => Vec::new(),
    };
    {
        let ya: std::collections::HashSet<String> = filas.iter().map(|f| f.id.clone()).collect();
        for f in buscar_con(db, &q, limite)? {
            if filas.len() >= limite {
                break;
            }
            if !ya.contains(&f.id) {
                filas.push(f);
            }
        }
    }
    //  Try all words first. If that finds nothing, try any of them: a
    //  partial match is better than an empty screen, and `bm25` ranks
    //  higher what matches in more places.
    if filas.is_empty() {
        if let Some(floja) = consulta_fts_floja(texto) {
            filas = buscar_con(db, &floja, limite)?;
        }
    }

    //  Finally, what is SIMILAR.
    //
    //  Keep it separate, not mixed: results containing your words are what
    //  you asked for and always come first. Similar results fill in behind
    //  them, and only when a model is available. Combining both rankings
    //  into one score sounds better, but in practice can push exact results
    //  down to third place.
    if filas.len() < limite {
        let ya: std::collections::HashSet<String> = filas.iter().map(|f| f.id.clone()).collect();
        for (id, _) in parecidas(db, texto, limite)? {
            if filas.len() >= limite {
                break;
            }
            if ya.contains(&id) {
                continue;
            }
            if let Some(f) = una(db, &id)? {
                filas.push(f);
            }
        }
    }
    Ok(filas)
}

fn buscar_con(db: &Connection, q: &str, limite: usize) -> rusqlite::Result<Vec<Fila>> {
    let limite = limite.clamp(1, 200);
    let mut s = db.prepare(
        "SELECT f.capture_id,
                snippet(captures_fts, -1, '[', ']', '…', 12)
           FROM captures_fts f
          WHERE captures_fts MATCH ?1
          ORDER BY bm25(captures_fts, 0.0, 8.0, 4.0, 2.0, 5.0, 4.0, 1.0, 6.0, 3.0)
          LIMIT ?2",
    )?;
    //  One weight per column, including `capture_id`, which is not indexed
    //  but is first. Without its 0.0, all weights shifted by one, making the
    //  title weight 4 and body 6, so a passing mention in an article beat
    //  the title.
    //  And the source, last, with a weight of 3.
    //
    //  The `bm25` weights are not decoration. By default all columns are
    //  equal, so a stray word in the middle of a ten-thousand-word article
    //  competes with the same word in the title. The title leads, tags come
    //  close —you added them by hand—and the body matters least.
    //
    //  The summary weighs almost as much as a tag for the same reason: its
    //  forty words were chosen to say what this is about, so a match there
    //  is worth much more than one in the middle of an article.
    let ids: Vec<(String, String)> = s
        .query_map(params![q, limite as i64], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .filter_map(|x| x.ok())
        .collect();

    let mut fuera = Vec::with_capacity(ids.len());
    for (id, trozo) in ids {
        if let Some(mut f) = una(db, &id)? {
            f.snippet = Some(trozo);
            fuera.push(f);
        }
    }
    Ok(fuera)
}

pub fn una(db: &Connection, id: &str) -> rusqlite::Result<Option<Fila>> {
    let fila = db
        .query_row(
            "SELECT id, type, title, author, excerpt, note, source_url, canonical_url,
                    blob_hash, blob_bytes, blob_mime, blob_name, preview_hash,
                    status, source, summary,
                    captured_at, last_opened_at, snoozed_until, favorite
               FROM captures WHERE id = ?1",
            [id],
            |r| {
                Ok(Fila {
                    id: r.get(0)?,
                    tipo: r.get(1)?,
                    title: r.get(2)?,
                    author: r.get(3)?,
                    excerpt: r.get(4)?,
                    note: r.get(5)?,
                    source_url: r.get(6)?,
                    canonical_url: r.get(7)?,
                    blob_hash: r.get(8)?,
                    blob_bytes: r.get(9)?,
                    blob_mime: r.get(10)?,
                    blob_name: r.get(11)?,
                    blob_path: None,
                    preview_path: r.get::<_, Option<String>>(12)?,
                    status: r.get(13)?,
                    source: r.get(14)?,
                    summary: r.get(15)?,
                    captured_at: r.get(16)?,
                    last_opened_at: r.get(17)?,
                    snoozed_until: r.get(18)?,
                    favorite: r.get::<_, i64>(19)? != 0,
                    tags: Vec::new(),
                    snippet: None,
                })
            },
        )
        .optional()?;
    let Some(mut f) = fila else { return Ok(None) };
    f.blob_path = f
        .blob_hash
        .as_ref()
        .map(|h| crate::paths::blob_de(h).to_string_lossy().to_string());
    //  The thumbnail hash goes in, and its path comes out. The field stays
    //  the same because the renderer has no use for a hash.
    f.preview_path = f
        .preview_path
        .as_ref()
        .map(|h| crate::paths::blob_de(h).to_string_lossy().to_string());
    let mut s = db.prepare(
        "SELECT t.name FROM tags t JOIN capture_tags ct ON ct.tag_id = t.id
          WHERE ct.capture_id = ?1 ORDER BY t.name",
    )?;
    f.tags = s
        .query_map([id], |r| r.get::<_, String>(0))?
        .filter_map(|x| x.ok())
        .collect();
    Ok(Some(f))
}

//  What is incomplete.
//
//  Enrichment happens when saving, which leaves out two things: items saved
//  before this existed, and items for which there was no network at the time.
//  Without a pending list, a month-old library would keep raw URLs as titles
//  forever, which is exactly what enrichment is meant to fix.
//
//  An item is incomplete if it is missing a thumbnail, still has a fallback
//  title, has no known source, or has no text. These are four separate
//  conditions: an image can have a thumbnail and still be called
//  “example.com.”
//
//  Return only the URL to visit and/or file to inspect: whoever accesses the
//  network does not need the entire capture.
#[derive(Debug, Serialize)]
pub struct PorEnriquecer {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    //  The resolved file path. As with `blob_path`, the database stores the
    //  hash, and the caller opening the file has no use for it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

pub fn por_enriquecer(db: &Connection, limite: i64) -> rusqlite::Result<Vec<PorEnriquecer>> {
    let mut s = db.prepare(
        "SELECT id, title, source_url, canonical_url, preview_hash, source,
                LENGTH(content_text), blob_hash
           FROM captures
          WHERE trashed_at IS NULL
            AND (COALESCE(canonical_url, source_url) LIKE 'http%'
                 OR blob_hash IS NOT NULL)
          ORDER BY captured_at DESC
          LIMIT 600",
    )?;
    let filas = s.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, Option<String>>(4)?,
            r.get::<_, String>(5)?,
            r.get::<_, i64>(6)?,
            r.get::<_, Option<String>>(7)?,
        ))
    })?;
    let mut fuera = Vec::new();
    for f in filas.flatten() {
        if fuera.len() as i64 >= limite.max(0) {
            break;
        }
        let (id, titulo, origen, canonica, foto, sitio, cuerpo, blob) = f;
        let falta = foto.is_none()
            || sitio.is_empty()
            || cuerpo == 0
            || crate::ingest::titulo_de_emergencia(&titulo, origen.as_deref(), canonica.as_deref());
        if !falta {
            continue;
        }
        //  Canonical URL first: that is the one to visit.
        let url = canonica.or(origen).filter(|u| u.starts_with("http"));
        //  Return the file only if its text is missing: reopening a PDF
        //  that was already read is wasted work, and takes seconds each time.
        let path = blob
            .filter(|_| cuerpo == 0)
            .map(|h| crate::paths::blob_de(&h).to_string_lossy().to_string());
        if url.is_none() && path.is_none() {
            continue;
        }
        fuera.push(PorEnriquecer { id, url, path });
    }
    Ok(fuera)
}

// ── semantic similarity ───────────────────────────────────────────
//
//  FTS finds what contains your words. This finds things that mean the same,
//  which is different and becomes useful as the library grows: with twenty
//  captures you remember their names; with two thousand, you do not.
//
//  None of this exists without a model, and the library then searches exactly
//  as before. Deliberately optional: it takes 506 MB on someone's disk.

//  Read from the database what text represents a capture.
fn texto_para_vector(db: &Connection, id: &str) -> rusqlite::Result<String> {
    let (titulo, resumen, extracto, cuerpo) = db.query_row(
        "SELECT title, summary, excerpt, content_text FROM captures WHERE id = ?1",
        [id],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        },
    )?;
    let mut s = db.prepare(
        "SELECT t.name FROM tags t JOIN capture_tags ct ON ct.tag_id = t.id
          WHERE ct.capture_id = ?1 ORDER BY t.name",
    )?;
    let etiquetas: Vec<String> = s
        .query_map([id], |r| r.get::<_, String>(0))?
        .filter_map(|x| x.ok())
        .collect();
    Ok(crate::vectores::texto_de(
        &titulo,
        &resumen,
        &etiquetas.join(", "),
        &extracto,
        &cuerpo,
    ))
}

//  Rebuild a capture's vector. Called after saving it and whenever its
//  description changes: title, summary, or tags.
//  How much material is enough to make a vector worthwhile.
//
//  This is the most important number here, and it came from a bug. A Reddit
//  capture —which returns no text unless JavaScript runs—had only six
//  characters: its vector represented the word “Reddit” and landed in a
//  generic area, making it 0.26 similar to “baking” and 0.24 similar to
//  “video editing.” It appeared in EVERY search.
//
//  A vector made from one word summarizes nothing; it is noise shaped like a
//  result. Without enough material, there is no vector, and the capture can
//  still be found by its name, the only thing actually known about it.
pub const MATERIAL_MINIMO: usize = 24;

pub fn revectorizar(db: &Connection, id: &str) -> rusqlite::Result<bool> {
    if !crate::vectores::listo() {
        return Ok(false);
    }
    let texto = texto_para_vector(db, id)?;
    if texto.trim().chars().count() < MATERIAL_MINIMO || texto.split_whitespace().count() < 4 {
        return Ok(false);
    }
    let Some(v) = crate::vectores::vector(&texto) else {
        return Ok(false);
    };
    db.execute(
        "INSERT INTO capture_vectors (capture_id, dim, hecho_en, v) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(capture_id) DO UPDATE SET dim = ?2, hecho_en = ?3, v = ?4",
        rusqlite::params![
            id,
            v.len() as i64,
            crate::util::ahora_ms(),
            crate::vectores::a_bytes(&v)
        ],
    )?;
    Ok(true)
}

//  Captures without a vector, or with one of a different dimension —that is,
//  from another model. Rebuild them N at a time so installing a model with two
//  thousand captures does not block the library for five minutes.
pub fn vectorizar_pendientes(db: &Connection, cuantas: i64) -> rusqlite::Result<(usize, i64)> {
    if !crate::vectores::listo() {
        return Ok((0, 0));
    }
    let medida = crate::vectores::medida() as i64;
    let pendientes = |db: &Connection| -> rusqlite::Result<i64> {
        db.query_row(
            "SELECT COUNT(*) FROM captures c
              WHERE c.trashed_at IS NULL
                AND LENGTH(c.title) + LENGTH(c.summary) + LENGTH(c.excerpt)
                    + LENGTH(c.content_text) >= ?2
                AND NOT EXISTS (SELECT 1 FROM capture_vectors v
                                 WHERE v.capture_id = c.id AND v.dim = ?1)",
            rusqlite::params![medida, MATERIAL_MINIMO as i64],
            |r| r.get(0),
        )
    };
    let mut s = db.prepare(
        //  Also require enough material here: otherwise captures that cannot
        //  produce a vector are selected in every pass forever, without ever
        //  getting one.
        "SELECT c.id FROM captures c
          WHERE c.trashed_at IS NULL
            AND LENGTH(c.title) + LENGTH(c.summary) + LENGTH(c.excerpt)
                + LENGTH(c.content_text) >= ?3
            AND NOT EXISTS (SELECT 1 FROM capture_vectors v
                             WHERE v.capture_id = c.id AND v.dim = ?1)
          ORDER BY c.captured_at DESC
          LIMIT ?2",
    )?;
    let ids: Vec<String> = s
        .query_map(
            rusqlite::params![medida, cuantas.max(0), MATERIAL_MINIMO as i64],
            |r| r.get::<_, String>(0),
        )?
        .filter_map(|x| x.ok())
        .collect();
    let mut hechas = 0;
    for id in &ids {
        if revectorizar(db, id).unwrap_or(false) {
            hechas += 1;
        }
    }
    Ok((hechas, pendientes(db)?))
}

//  What most resembles what you wrote.
//
//  Compare all vectors directly, without an index, by design: a personal
//  library has thousands of captures, not millions, and multiplying two
//  thousand vectors of 256 numbers takes less time than opening an index. If
//  that stops being enough, the table is already separate so one can be added.
//  Below this, it is not similar: it resembles a little bit of everything,
//  which is how vectors say “I don't know.” Without a threshold, any search
//  returns the entire library in a random order.
//
//  This number comes from real results, not an article: with this model —
//  static embeddings, no transformer—cosines are lower than with larger
//  models, and the copied 0.35 threshold excluded clear matches.
pub const PARECIDO_MINIMO: f32 = 0.22;

pub fn parecidas(
    db: &Connection,
    texto: &str,
    limite: usize,
) -> rusqlite::Result<Vec<(String, f32)>> {
    parecidas_desde(db, texto, limite, PARECIDO_MINIMO)
}

pub fn parecidas_desde(
    db: &Connection,
    texto: &str,
    limite: usize,
    minimo: f32,
) -> rusqlite::Result<Vec<(String, f32)>> {
    let Some(q) = crate::vectores::vector(texto) else {
        return Ok(Vec::new());
    };
    let mut s = db.prepare(
        "SELECT v.capture_id, v.v FROM capture_vectors v
           JOIN captures c ON c.id = v.capture_id
          WHERE c.trashed_at IS NULL AND v.dim = ?1",
    )?;
    let mut todas: Vec<(String, f32)> = s
        .query_map(rusqlite::params![q.len() as i64], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
        })?
        .filter_map(|x| x.ok())
        .map(|(id, b)| {
            let p = crate::vectores::parecido(&q, &crate::vectores::de_bytes(&b));
            (id, p)
        })
        .filter(|(_, p)| *p >= minimo)
        .collect();
    todas.sort_by(|a, b| b.1.total_cmp(&a.1));
    todas.truncate(limite);
    Ok(todas)
}

//  What has not been summarized yet.
//
//  Return just enough for a model to say what something is about: its title,
//  source, and a snippet from the beginning. **Not** the entire content; this
//  is a privacy decision, not a size limit. Only the minimum needed for the
//  task should leave the device, and a link can be summarized from its title
//  and description.
#[derive(Debug, Serialize)]
pub struct PorClasificar {
    pub id: String,
    pub title: String,
    pub source: String,
    #[serde(rename = "type")]
    pub tipo: String,
    pub excerpt: String,
    //  Also include the file if it is an image that must be inspected.
    //
    //  There is nothing to summarize about an image from its metadata: its
    //  title is the filename and its excerpt is empty. It must be inspected
    //  to know what it is, which is why the path is included.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_path: Option<String>,
}

pub fn por_clasificar(db: &Connection, limite: i64) -> rusqlite::Result<Vec<PorClasificar>> {
    let mut s = db.prepare(
        //  Items not yet processed, not items without a summary: those are
        //  different. Nothing can be said about a page that returns no
        //  text, and without recording the attempt it will be requested
        //  again every twenty seconds forever, at a cost.
        "SELECT id, title, source, type, excerpt, blob_hash, blob_mime
           FROM captures
          WHERE trashed_at IS NULL AND summary = '' AND summary_at IS NULL
          ORDER BY captured_at DESC
          LIMIT ?1",
    )?;
    let v = s
        .query_map([limite.max(0)], |r| {
            Ok(PorClasificar {
                id: r.get(0)?,
                title: r.get(1)?,
                source: r.get(2)?,
                tipo: r.get(3)?,
                //  Truncate here, not in the caller: the limit applies to
                //  what leaves the device, and the caller does not decide
                //  that.
                excerpt: r.get::<_, String>(4)?.chars().take(400).collect(),
                image_path: {
                    let hash: Option<String> = r.get(5)?;
                    let mime: Option<String> = r.get(6)?;
                    match (hash, mime) {
                        (Some(h), Some(m)) if m.starts_with("image/") => {
                            Some(crate::paths::blob_de(&h).to_string_lossy().to_string())
                        }
                        _ => None,
                    }
                },
            })
        })?
        .filter_map(|x| x.ok())
        .collect();
    Ok(v)
}

//  The usual list: most recently saved, excluding trashed items. This is what
//  the page shows when the search box is empty.
//
//  The navigation filter: a folder, a tag, or “today.” Pass it as-is and build
//  the query here instead of letting the caller compose SQL; that is the
//  difference between a filter and an injection.
#[derive(Debug, Default, serde::Deserialize)]
pub struct Filtro {
    pub space: Option<String>,
    pub tag: Option<String>,
    #[serde(default)]
    pub today: bool,
    #[serde(default)]
    pub trashed: bool,
}

pub fn listar(
    db: &Connection,
    limite: usize,
    desde: Option<i64>,
    f: &Filtro,
) -> rusqlite::Result<Vec<Fila>> {
    let limite = limite.clamp(1, 500);
    let mut donde = String::from(if f.trashed {
        "trashed_at IS NOT NULL"
    } else {
        "trashed_at IS NULL"
    });
    if desde.is_some() {
        donde.push_str(" AND captured_at < :desde");
    }
    if f.today {
        donde.push_str(" AND captured_at >= :hoy");
    }
    if f.space.is_some() {
        donde
            .push_str(" AND id IN (SELECT capture_id FROM capture_spaces WHERE space_id = :space)");
    }
    if f.tag.is_some() {
        donde.push_str(" AND id IN (SELECT capture_id FROM capture_tags WHERE tag_id = :tag)");
    }
    let sql = format!(
        "SELECT id FROM captures WHERE {} ORDER BY captured_at DESC LIMIT :limite",
        donde
    );
    let mut s = db.prepare(&sql)?;
    let hoy = comienzo_del_dia();
    let mut nombrados: Vec<(&str, &dyn rusqlite::ToSql)> = vec![(":limite", &limite)];
    if let Some(d) = &desde {
        nombrados.push((":desde", d));
    }
    if f.today {
        nombrados.push((":hoy", &hoy));
    }
    if let Some(e) = &f.space {
        nombrados.push((":space", e));
    }
    if let Some(t) = &f.tag {
        nombrados.push((":tag", t));
    }
    let ids: Vec<String> = s
        .query_map(&nombrados[..], |r| r.get::<_, String>(0))?
        .filter_map(|x| x.ok())
        .collect();
    let mut fuera = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(f) = una(db, &id)? {
            fuera.push(f);
        }
    }
    Ok(fuera)
}

//  Today's midnight in the device's timezone. SQLite calculates it with
//  `date(...,'localtime')`; doing this in Rust would require an entire date
//  crate.
fn comienzo_del_dia() -> i64 {
    (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
        / 86400)
        * 86400
        * 1000
}

//  What has resurfaced: items that were snoozed and are now due.
//
//  Nothing else, by design. Other reasons to resurface —not opened for a long
//  time, associated with the current project—belong to D6. Filling this area
//  with “recently saved” in the meantime would be a lie under this heading:
//  “resurfaced” does not mean “from yesterday.”
pub fn reaparecidas(db: &Connection, limite: usize) -> rusqlite::Result<Vec<Fila>> {
    let limite = limite.clamp(1, 20);
    let mut s = db.prepare(
        "SELECT id FROM captures
          WHERE trashed_at IS NULL AND snoozed_until IS NOT NULL AND snoozed_until <= ?2
          ORDER BY snoozed_until DESC LIMIT ?1",
    )?;
    let ids: Vec<String> = s
        .query_map(params![limite as i64, crate::util::ahora_ms()], |r| {
            r.get::<_, String>(0)
        })?
        .filter_map(|x| x.ok())
        .collect();
    let mut fuera = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(f) = una(db, &id)? {
            fuera.push(f);
        }
    }
    Ok(fuera)
}

//  The left-side navigation and its counts. Read it from the database, not a
//  list in QML: users create folders, and a hard-coded sidebar would stop
//  counting them once they created a fourth.
#[derive(Debug, Serialize)]
pub struct Seccion {
    pub id: String,
    pub name: String,
    pub kind: String, //  inbox · today · space · tags
    pub count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

pub fn secciones(db: &Connection) -> rusqlite::Result<Vec<Seccion>> {
    let uno = |sql: &str, p: &[&dyn rusqlite::ToSql]| -> rusqlite::Result<i64> {
        db.query_row(sql, p, |r| r.get(0))
    };
    let mut fuera = vec![
        Seccion {
            id: "inbox".into(),
            name: "Entrada".into(),
            kind: "inbox".into(),
            count: uno(
                "SELECT COUNT(*) FROM captures WHERE trashed_at IS NULL",
                &[],
            )?,
            color: None,
        },
        Seccion {
            id: "today".into(),
            name: "Hoy".into(),
            kind: "today".into(),
            count: uno(
                "SELECT COUNT(*) FROM captures WHERE trashed_at IS NULL AND captured_at >= ?1",
                &[&comienzo_del_dia()],
            )?,
            color: None,
        },
    ];
    let mut s = db.prepare(
        "SELECT s.id, s.name, s.color,
                (SELECT COUNT(*) FROM capture_spaces cs
                  JOIN captures c ON c.id = cs.capture_id
                 WHERE cs.space_id = s.id AND c.trashed_at IS NULL)
           FROM spaces s ORDER BY s.name",
    )?;
    let espacios: Vec<Seccion> = s
        .query_map([], |r| {
            Ok(Seccion {
                id: r.get(0)?,
                name: r.get(1)?,
                kind: "space".into(),
                color: {
                    let c: String = r.get(2)?;
                    if c.is_empty() {
                        None
                    } else {
                        Some(c)
                    }
                },
                count: r.get(3)?,
            })
        })?
        .filter_map(|x| x.ok())
        .collect();
    fuera.extend(espacios);
    fuera.push(Seccion {
        id: "tags".into(),
        name: "Etiquetas".into(),
        kind: "tags".into(),
        count: uno("SELECT COUNT(*) FROM tags", &[])?,
        color: None,
    });
    Ok(fuera)
}

#[derive(Debug, Serialize)]
pub struct Cuentas {
    pub captures: i64,
    pub trashed: i64,
    //  How many still lack a summary. This is the only count describing
    //  remaining WORK rather than the library itself. The page already
    //  requests counts on every refresh, so this number costs no extra
    //  call; without it, the only way to know whether this works was to
    //  inspect the database manually.
    pub pending_summary: i64,
    pub tags: i64,
    pub spaces: i64,
    pub blobs: i64,
    pub blob_bytes: i64,
}

pub fn cuentas(db: &Connection) -> rusqlite::Result<Cuentas> {
    let uno = |sql: &str| -> rusqlite::Result<i64> { db.query_row(sql, [], |r| r.get(0)) };
    Ok(Cuentas {
        captures: uno("SELECT COUNT(*) FROM captures WHERE trashed_at IS NULL")?,
        trashed: uno("SELECT COUNT(*) FROM captures WHERE trashed_at IS NOT NULL")?,
        pending_summary: uno("SELECT COUNT(*) FROM captures
              WHERE trashed_at IS NULL AND summary = '' AND summary_at IS NULL")?,
        tags: uno("SELECT COUNT(*) FROM tags")?,
        spaces: uno("SELECT COUNT(*) FROM spaces")?,
        blobs: uno("SELECT COUNT(DISTINCT blob_hash) FROM captures WHERE blob_hash IS NOT NULL")?,
        blob_bytes: uno("SELECT COALESCE(SUM(blob_bytes), 0) FROM
               (SELECT DISTINCT blob_hash, blob_bytes FROM captures WHERE blob_hash IS NOT NULL)")?,
    })
}
