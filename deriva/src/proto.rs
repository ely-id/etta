//  The protocol, and the single place that decides what each method does.
//
//  Versioned JSON Lines, like the agent's and for the same reasons: one line
//  is one message; stdout is only for data —warnings go to stderr, or a trace
//  becomes a malformed message, or worse, one that looks valid—and there is a
//  line limit, because without one the reader's buffer grows to whatever the
//  other side sends.
//
//  The same dispatcher handles the socket and the command line. Two different
//  paths to the same database lead to two behaviors for the same operation and
//  a test that covers only one.

use rusqlite::{Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};

//  The maximum size of a thumbnail arriving from outside. Half a megabyte is
//  the same limit `tools/web` applies when fetching it; repeating the limit
//  here does not mean we distrust that tool, but that this process does not
//  know who is writing to it.
const TOPE_MINIATURA: usize = 512 * 1024;

pub const VERSION: u32 = 1;
//  The largest payload that can travel in one line: a 64 MiB inline image grows
//  by a third when base64-encoded, plus the rest of the message.
pub const TOPE_LINEA: usize = 90 * 1024 * 1024;

#[derive(Debug, Deserialize)]
pub struct Peticion {
    #[serde(default)]
    pub v: u32,
    #[serde(default)]
    pub id: String,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct Respuesta {
    pub v: u32,
    pub id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Error>,
}

#[derive(Debug, Serialize)]
pub struct Error {
    pub code: String,
    pub message: String,
}

impl Respuesta {
    pub fn bien(id: &str, r: serde_json::Value) -> Self {
        Respuesta {
            v: VERSION,
            id: id.into(),
            ok: true,
            result: Some(r),
            error: None,
        }
    }
    pub fn mal(id: &str, code: &str, message: impl Into<String>) -> Self {
        Respuesta {
            v: VERSION,
            id: id.into(),
            ok: false,
            result: None,
            error: Some(Error {
                code: code.into(),
                message: message.into(),
            }),
        }
    }
}

//  What can be requested. This list is closed, and no method accepts an
//  arbitrary path except `export`: the plan says “arbitrary paths are not
//  accepted by agent methods,” and export is the exception because the user
//  chooses the path and the controller validates it before it gets here.
pub fn despachar(db: &Connection, p: &Peticion) -> Respuesta {
    if p.v != 0 && p.v != VERSION {
        return Respuesta::mal(&p.id, "version", format!("solo hablo la v{}", VERSION));
    }
    match p.method.as_str() {
        "ping" => Respuesta::bien(
            &p.id,
            serde_json::json!({ "pong": true, "schema": crate::db::VERSION }),
        ),

        "ingest" => match serde_json::from_value::<crate::ingest::Peticion>(p.params.clone()) {
            Ok(pet) => {
                let items = crate::ingest::ingerir(db, &pet);
                let guardadas = items.iter().filter(|r| r.status == "saved").count();
                let duplicadas = items.iter().filter(|r| r.status == "duplicate").count();
                Respuesta::bien(
                    &p.id,
                    serde_json::json!({
                        "items": items, "saved": guardadas, "duplicates": duplicadas,
                    }),
                )
            }
            Err(e) => Respuesta::mal(&p.id, "params", e.to_string()),
        },

        "search" => {
            let q = p.params.get("query").and_then(|v| v.as_str()).unwrap_or("");
            let n = p.params.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
            match crate::search::buscar(db, q, n) {
                Ok(items) => Respuesta::bien(&p.id, serde_json::json!({ "items": items })),
                Err(e) => Respuesta::mal(&p.id, "db", e.to_string()),
            }
        }

        "list" => {
            let n = p.params.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
            let desde = p.params.get("before").and_then(|v| v.as_i64());
            let f: crate::search::Filtro =
                serde_json::from_value(p.params.clone()).unwrap_or_default();
            match crate::search::listar(db, n, desde, &f) {
                Ok(items) => Respuesta::bien(&p.id, serde_json::json!({ "items": items })),
                Err(e) => Respuesta::mal(&p.id, "db", e.to_string()),
            }
        }

        //  Everything the page needs on startup: navigation and its counts,
        //  items that resurfaced, and the first page of the library. Three
        //  socket round trips to render one screen are three chances to
        //  render it only partially.
        "home" => {
            let n = p.params.get("limit").and_then(|v| v.as_u64()).unwrap_or(60) as usize;
            let f: crate::search::Filtro =
                serde_json::from_value(p.params.clone()).unwrap_or_default();
            let r = (|| -> rusqlite::Result<serde_json::Value> {
                Ok(serde_json::json!({
                    "sections": crate::search::secciones(db)?,
                    //  Include the reason. All items here returned for the
                    //  same reason —you snoozed them and they are due—but
                    //  the page displays it, and nobody can display a
                    //  reason that must be guessed by reading the code.
                    "surfaced": crate::search::reaparecidas(db, 3)?
                        .into_iter()
                        .map(|f| {
                            let mut v = serde_json::to_value(f).unwrap_or_default();
                            if let Some(o) = v.as_object_mut() {
                                o.insert("porque".into(),
                                         serde_json::json!("you snoozed this until today"));
                            }
                            v
                        })
                        .collect::<Vec<_>>(),
                    "items": crate::search::listar(db, n, None, &f)?,
                    "stats": crate::search::cuentas(db)?,
                }))
            })();
            match r {
                Ok(v) => Respuesta::bien(&p.id, v),
                Err(e) => Respuesta::mal(&p.id, "db", e.to_string()),
            }
        }

        //  ── folders ──────────────────────────────────────────────
        //
        //  Until now, a folder existed only if it contained something: it
        //  was created along the way in `poner_espacio`. That works for the
        //  three built-in folders, but not for user folders, which are
        //  created BEFORE there is anything to put in them —“I'll have one
        //  for the move”— and because a folder that disappears when its
        //  last item is removed is not a folder.
        //
        //  The ID comes from the name and does NOT change when renamed:
        //  that makes renaming a rename rather than creating another folder.
        "space_put" => {
            let nombre = p
                .params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .chars()
                .take(60)
                .collect::<String>();
            if nombre.is_empty() {
                return Respuesta::mal(&p.id, "sin_nombre", "a folder needs a name");
            }
            //  With an ID, rename the existing folder; without one, create
            //  a new folder.
            let id = p
                .params
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| crate::ingest::id_de_espacio(&nombre));
            if id.is_empty() {
                return Respuesta::mal(
                    &p.id,
                    "nombre_invalido",
                    "that name does not produce a valid identifier",
                );
            }
            let color = p
                .params
                .get("color")
                .and_then(|v| v.as_str())
                .filter(|c| c.starts_with('#') && c.len() <= 9)
                .unwrap_or("");
            let r = db.execute(
                "INSERT INTO spaces (id, name, color) VALUES (?1, ?2, ?3)
                 ON CONFLICT(id) DO UPDATE SET name = ?2,
                     color = CASE WHEN ?3 = '' THEN color ELSE ?3 END",
                rusqlite::params![id, nombre, color],
            );
            match r {
                Ok(_) => Respuesta::bien(&p.id, serde_json::json!({ "id": id, "name": nombre })),
                Err(e) => Respuesta::mal(&p.id, "db", e.to_string()),
            }
        }

        //  And remove it. Its contents are NOT deleted: they stay in the
        //  library without a folder, as anyone who has ever deleted a
        //  folder would expect. Removing a bookshelf does not burn the books.
        "space_delete" => {
            let id = p.params.get("id").and_then(|v| v.as_str()).unwrap_or("");
            if id.is_empty() {
                return Respuesta::mal(&p.id, "sin_id", "an ID is required");
            }
            let sueltas = db
                .execute("DELETE FROM capture_spaces WHERE space_id = ?1", [id])
                .unwrap_or(0);
            match db.execute("DELETE FROM spaces WHERE id = ?1", [id]) {
                Ok(0) => Respuesta::mal(&p.id, "no_existe", "no folder exists with that ID"),
                Ok(_) => {
                    Respuesta::bien(&p.id, serde_json::json!({ "id": id, "released": sueltas }))
                }
                Err(e) => Respuesta::mal(&p.id, "db", e.to_string()),
            }
        }

        //  The tags you already use, and how many items each one has.
        //
        //  The “Tags” section showed only the count, so there was no way to
        //  know which tags they were. This is needed for two things:
        //  displaying them and telling the AI “use these”—otherwise it
        //  invents a new taxonomy every batch, and a week later there are
        //  two hundred tags with one capture each.
        "tags" => {
            let r = (|| -> rusqlite::Result<Vec<serde_json::Value>> {
                let mut s = db.prepare(
                    "SELECT t.name, COUNT(ct.capture_id)
                       FROM tags t
                       LEFT JOIN capture_tags ct ON ct.tag_id = t.id
                       LEFT JOIN captures c ON c.id = ct.capture_id AND c.trashed_at IS NULL
                      GROUP BY t.id
                      HAVING COUNT(c.id) > 0
                      ORDER BY COUNT(c.id) DESC, t.name
                      LIMIT 200",
                )?;
                let v = s
                    .query_map([], |r| {
                        Ok(serde_json::json!({
                            "name": r.get::<_, String>(0)?,
                            "count": r.get::<_, i64>(1)?,
                        }))
                    })?
                    .filter_map(|x| x.ok())
                    .collect();
                Ok(v)
            })();
            match r {
                Ok(v) => Respuesta::bien(&p.id, serde_json::json!({ "tags": v })),
                Err(e) => Respuesta::mal(&p.id, "db", e.to_string()),
            }
        }

        "sections" => match crate::search::secciones(db) {
            Ok(v) => Respuesta::bien(&p.id, serde_json::json!({ "sections": v })),
            Err(e) => Respuesta::mal(&p.id, "db", e.to_string()),
        },

        //  Snooze: resurface when due. The caller calculates the date; this
        //  only stores it, because “in a week” depends on a calendar, and
        //  this component has none.
        "snooze" => {
            let id = p.params.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let hasta = p.params.get("until").and_then(|v| v.as_i64());
            match db.execute(
                "UPDATE captures SET snoozed_until = ?2 WHERE id = ?1",
                rusqlite::params![id, hasta],
            ) {
                Ok(0) => Respuesta::mal(&p.id, "no_existe", "nothing exists with that ID"),
                Ok(_) => Respuesta::bien(&p.id, serde_json::json!({ "id": id })),
                Err(e) => Respuesta::mal(&p.id, "db", e.to_string()),
            }
        }

        //  The personal note, tags, and favorite: the only capture fields
        //  the page writes. Content is not edited —what you saved is what
        //  was there—which is why there is no method to change it.
        "annotate" => {
            let id = p.params.get("id").and_then(|v| v.as_str()).unwrap_or("");
            if crate::search::una(db, id).ok().flatten().is_none() {
                return Respuesta::mal(&p.id, "no_existe", "nothing exists with that ID");
            }
            if let Some(n) = p.params.get("note").and_then(|v| v.as_str()) {
                if let Err(e) = db.execute(
                    "UPDATE captures SET note = ?2 WHERE id = ?1",
                    rusqlite::params![id, &n.chars().take(4000).collect::<String>()],
                ) {
                    return Respuesta::mal(&p.id, "db", e.to_string());
                }
            }
            //  Record that you opened it. This stops it from resurfacing:
            //  the resurfacing rule excludes items you viewed recently, and
            //  without this it would have no way to know.
            if p.params.get("opened").and_then(|v| v.as_bool()) == Some(true) {
                let _ = db.execute(
                    "UPDATE captures SET last_opened_at = ?2 WHERE id = ?1",
                    rusqlite::params![id, crate::util::ahora_ms()],
                );
            }
            if let Some(f) = p.params.get("favorite").and_then(|v| v.as_bool()) {
                let _ = db.execute(
                    "UPDATE captures SET favorite = ?2 WHERE id = ?1",
                    rusqlite::params![id, if f { 1 } else { 0 }],
                );
            }
            if let Some(ts) = p.params.get("tags").and_then(|v| v.as_array()) {
                let _ = db.execute("DELETE FROM capture_tags WHERE capture_id = ?1", [id]);
                for t in ts.iter().filter_map(|x| x.as_str()).take(32) {
                    let _ = crate::ingest::poner_etiqueta(db, id, t);
                }
            }
            if let Some(e) = p.params.get("space").and_then(|v| v.as_str()) {
                let _ = db.execute("DELETE FROM capture_spaces WHERE capture_id = ?1", [id]);
                if !e.is_empty() {
                    let _ = crate::ingest::poner_espacio(db, id, e);
                }
            }
            //  And the vector, which describes the same thing as the index:
            //  if the title or summary changes, so does what the capture
            //  “means.” Rebuilding it here avoids having two sources of truth.
            let _ = crate::search::revectorizar(db, id);

            match crate::db::reindexar(db, id) {
                Ok(()) => match crate::search::una(db, id) {
                    Ok(Some(f)) => Respuesta::bien(&p.id, serde_json::json!({ "item": f })),
                    _ => Respuesta::mal(&p.id, "db", "could not read the capture again"),
                },
                Err(e) => Respuesta::mal(&p.id, "db", e.to_string()),
            }
        }

        //  Items still missing a thumbnail, so it can be fetched. See
        //  `search::por_enriquecer`.
        "to_enrich" => {
            let n = p
                .params
                .get("limit")
                .and_then(|v| v.as_i64())
                .unwrap_or(12)
                .clamp(1, 60);
            match crate::search::por_enriquecer(db, n) {
                Ok(v) => Respuesta::bien(&p.id, serde_json::json!({ "items": v })),
                Err(e) => Respuesta::mal(&p.id, "db", e.to_string()),
            }
        }

        //  ── external metadata ───────────────────────────────────
        //
        //  The actual title, author, and thumbnail. The worker does NOT
        //  fetch these: it has no network access and will not get it. The
        //  host fetches them with `tools/web`, where the reviewed anti-SSRF
        //  rules live, and passes them in here.
        //
        //  The important part of this method is the rule for what can and
        //  cannot be overwritten:
        //
        //  It NEVER overwrites your data. The note, tags, favorite, and
        //  folder are not touched or even read.
        //
        //  The title is updated only if the existing one is a fallback.
        //  When saved, a link is titled with what can be derived from its
        //  address —the domain or slug—as a placeholder until the real one
        //  arrives. But if the drag supplied a title —and browsers do—,
        //  that came from the source site and is just as valid. Distinguishing
        //  the two is the only way to ensure enrichment is not just
        //  “overwrite what was there” under another name.
        "enrich" => {
            let id = p.params.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let Ok(Some(antes)) = crate::search::una(db, id) else {
                return Respuesta::mal(&p.id, "no_existe", "nothing exists with that ID");
            };
            let dice = |k: &str| -> String {
                p.params
                    .get(k)
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string()
            };

            //  Validate the thumbnail BEFORE writing anything.
            //
            //  The reverse —validating it where it is stored— would leave
            //  the title and author written when an unreadable image returns
            //  an error: the caller retries, and the saved data is neither
            //  what it was before nor what it should be.
            let foto = dice("preview_b64");
            let bytes_foto = if foto.is_empty() {
                None
            } else {
                match crate::util::base64(&foto) {
                    Some(b) if !b.is_empty() && b.len() <= TOPE_MINIATURA => Some(b),
                    _ => {
                        return Respuesta::mal(
                            &p.id,
                            "miniatura_invalida",
                            "la miniatura no se puede leer o pasa del tope",
                        )
                    }
                }
            };

            //  Also update where it came from.
            //
            //  Items saved before adapters existed have no `source`, and
            //  that is not fixed automatically: derive it from the URL,
            //  without network access, just as when saving. Do it here
            //  because this is when someone revisits an old capture;
            //  scanning the entire library during migration would mean
            //  reading a hundred thousand rows just in case.
            if antes.source.is_empty() {
                if let Some(u) = antes
                    .canonical_url
                    .as_deref()
                    .or(antes.source_url.as_deref())
                    .filter(|u| u.starts_with("http"))
                {
                    let l = crate::adaptadores::leer(u);
                    let _ = db.execute(
                        "UPDATE captures SET source = ?2 WHERE id = ?1",
                        rusqlite::params![id, l.fuente.id],
                    );
                    //  Update the type only if it was generic: a YouTube link
                    //  is a video. If it already said something else, someone
                    //  knew more.
                    if antes.tipo == "url" {
                        let _ = db.execute(
                            "UPDATE captures SET type = ?2 WHERE id = ?1",
                            rusqlite::params![id, l.fuente.tipo],
                        );
                    }
                }
            }

            let titulo = dice("title");
            if !titulo.is_empty()
                && crate::ingest::titulo_de_emergencia(
                    &antes.title,
                    antes.source_url.as_deref(),
                    antes.canonical_url.as_deref(),
                )
            {
                let _ = db.execute(
                    "UPDATE captures SET title = ?2 WHERE id = ?1",
                    rusqlite::params![id, &titulo.chars().take(300).collect::<String>()],
                );
            }
            //  Set the author and summary only if empty: there is nothing to
            //  overwrite, and if something is there, someone knew more.
            let autor = dice("author");
            if !autor.is_empty() && antes.author.is_empty() {
                let _ = db.execute(
                    "UPDATE captures SET author = ?2 WHERE id = ?1",
                    rusqlite::params![id, &autor.chars().take(160).collect::<String>()],
                );
            }
            let resumen = dice("excerpt");
            if !resumen.is_empty() && antes.excerpt.is_empty() {
                let _ = db.execute(
                    "UPDATE captures SET excerpt = ?2 WHERE id = ?1",
                    rusqlite::params![id, &resumen.chars().take(600).collect::<String>()],
                );
            }
            //  And the content: the page body or whatever could be extracted
            //  from the file.
            //
            //  Only if it was absent; that is the usual rule. Here there is
            //  an additional reason: the content is what was saved, and a
            //  capture whose text changes on its own stops being a copy of
            //  what you saw. Reading the page again in a month would yield
            //  something else.
            //
            //  Add it to the index, which is why it is read: without this,
            //  searching for a phrase inside a saved article finds nothing,
            //  and the library only knows about titles.
            let cuerpo = dice("text");
            if !cuerpo.is_empty()
                && antes.excerpt.len() < crate::ingest::TOPE_TEXTO
                && cuerpo.len() <= crate::ingest::TOPE_TEXTO
            {
                let vacio: i64 = db
                    .query_row(
                        "SELECT COUNT(*) FROM captures WHERE id = ?1 AND content_text = ''",
                        [id],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                if vacio == 1 {
                    //  Put it back in the AI queue —`summary_at = NULL`—: it
                    //  was processed with what was available then, perhaps
                    //  only a title, and now there is a whole article. This
                    //  is the only time an attempt is discarded, because
                    //  the material has changed.
                    let _ = db.execute(
                        "UPDATE captures SET content_text = ?2, content_hash = ?3,
                                summary_at = NULL
                          WHERE id = ?1",
                        rusqlite::params![id, &cuerpo, crate::util::hash_de_texto(&cuerpo)],
                    );
                    //  If there was no excerpt, use the beginning of the
                    //  text: it is what the card displays and what the model
                    //  sees.
                    if antes.excerpt.is_empty() {
                        let _ = db.execute(
                            "UPDATE captures SET excerpt = ?2 WHERE id = ?1",
                            rusqlite::params![
                                id,
                                cuerpo.trim().chars().take(280).collect::<String>()
                            ],
                        );
                    }
                }
            }

            //  And the canonical URL, which determines duplicates. Only if
            //  it was absent: changing it later would change what this
            //  matches.
            let canonica = dice("canonical_url");
            if !canonica.is_empty() && antes.canonical_url.is_none() {
                if let Some(c) = crate::util::canonizar_url(&canonica) {
                    let _ = db.execute(
                        "UPDATE captures SET canonical_url = ?2 WHERE id = ?1",
                        rusqlite::params![id, c],
                    );
                }
            }

            //  Store the thumbnail in the usual content-addressed store:
            //  the same cover across twenty captures takes up space once.
            if let Some(bytes) = bytes_foto {
                if antes.preview_path.is_none() {
                    match crate::blobs::guardar_bytes(&bytes) {
                        Ok(g) => {
                            let _ = db.execute(
                                "UPDATE captures SET preview_hash = ?2 WHERE id = ?1",
                                rusqlite::params![id, g.hash],
                            );
                        }
                        Err(e) => return Respuesta::mal(&p.id, "disco", e.to_string()),
                    }
                }
            }

            //  And the vector, which describes the same thing as the index:
            //  if the title or summary changes, so does what the capture
            //  “means.” Rebuilding it here avoids having two sources of truth.
            let _ = crate::search::revectorizar(db, id);

            match crate::db::reindexar(db, id) {
                Ok(()) => match crate::search::una(db, id) {
                    Ok(Some(f)) => Respuesta::bien(&p.id, serde_json::json!({ "item": f })),
                    _ => Respuesta::mal(&p.id, "db", "could not read the capture again"),
                },
                Err(e) => Respuesta::mal(&p.id, "db", e.to_string()),
            }
        }

        //  ── what the AI understood ──────────────────────────────
        //
        //  A summary, some tags, and at most a folder. The results arrive
        //  already written: there is no model here, just as there is no
        //  network. The host requests them with the user's engine and
        //  permission.
        //
        //  The rules for overwriting are stricter than those for `enrich`,
        //  by design: a title from a page is a fact, while this is a model's
        //  opinion.
        //
        //  NEVER overwrite the note: it belongs to you, and nobody else
        //  writes it.
        //  Set the summary only if it was absent: rewriting it during every
        //  review would make what you read yesterday say something else
        //  today, without you changing anything.
        //  Tags are ADDED, not replaced. `annotate` replaces them because
        //  you are the one writing there; deleting yours to add the model's
        //  would let the AI organize your closet to its liking.
        //  Set the folder only if there was none: moving something for you
        //  is the kind of help that makes you lose track of your things.
        "classify" => {
            let id = p.params.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let Ok(Some(antes)) = crate::search::una(db, id) else {
                return Respuesta::mal(&p.id, "no_existe", "nothing exists with that ID");
            };

            let resumen = p
                .params
                .get("summary")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .chars()
                .take(600)
                .collect::<String>();
            if !resumen.is_empty() && antes.summary.is_empty() {
                let _ = db.execute(
                    "UPDATE captures SET summary = ?2 WHERE id = ?1",
                    rusqlite::params![id, resumen],
                );
            }

            //  Up to six model-generated tags; yours stay as they are.
            let mut puestas = 0;
            if let Some(ts) = p.params.get("tags").and_then(|v| v.as_array()) {
                for t in ts.iter().filter_map(|x| x.as_str()) {
                    if puestas >= 6 {
                        break;
                    }
                    if crate::ingest::poner_etiqueta(db, id, t).is_ok() {
                        puestas += 1;
                    }
                }
            }

            //  Set the folder only if the capture was unfiled.
            let mut movida = false;
            if let Some(e) = p.params.get("space").and_then(|v| v.as_str()) {
                let suelta: i64 = db
                    .query_row(
                        "SELECT COUNT(*) FROM capture_spaces WHERE capture_id = ?1",
                        [id],
                        |r| r.get(0),
                    )
                    .unwrap_or(1);
                if suelta == 0 && !e.trim().is_empty() {
                    let _ = crate::ingest::poner_espacio(db, id, e);
                    movida = true;
                }
            }

            //  Record that it was processed, whatever the result.
            //
            //  This method is ALSO called when the model had nothing to say,
            //  which is the important case: without the marker, anything
            //  that cannot be summarized is requested again every twenty
            //  seconds forever. “Nothing could be said” is a response.
            let _ = db.execute(
                "UPDATE captures SET summary_at = ?2 WHERE id = ?1",
                rusqlite::params![id, crate::util::ahora_ms()],
            );

            //  Add it to the index, which is half the reason this exists: a
            //  summary that cannot be found by searching is useful neither
            //  to the AI nor to you.
            //  And the vector, which describes the same thing as the index:
            //  if the title or summary changes, so does what the capture
            //  “means.” Rebuilding it here avoids having two sources of truth.
            let _ = crate::search::revectorizar(db, id);

            match crate::db::reindexar(db, id) {
                Ok(()) => match crate::search::una(db, id) {
                    Ok(Some(f)) => Respuesta::bien(
                        &p.id,
                        serde_json::json!({ "item": f, "tagged": puestas, "moved": movida }),
                    ),
                    _ => Respuesta::mal(&p.id, "db", "could not read the capture again"),
                },
                Err(e) => Respuesta::mal(&p.id, "db", e.to_string()),
            }
        }

        //  Items nobody has processed yet. Same role as `to_enrich` and for
        //  the same reason: without a pending-items list, things saved
        //  before this was enabled would remain without a summary forever.
        "to_classify" => {
            let n = p
                .params
                .get("limit")
                .and_then(|v| v.as_i64())
                .unwrap_or(8)
                .clamp(1, 40);
            match crate::search::por_clasificar(db, n) {
                Ok(v) => Respuesta::bien(&p.id, serde_json::json!({ "items": v })),
                Err(e) => Respuesta::mal(&p.id, "db", e.to_string()),
            }
        }

        //  Similarity only, with its score. This is the diagnostic tool for
        //  all of this: `search` deliberately combines exact and similar
        //  matches, and when something looks wrong we need to see which one
        //  returned it and with what confidence. Without this, tuning the
        //  threshold would be guesswork.
        "similar" => {
            let q = p.params.get("query").and_then(|v| v.as_str()).unwrap_or("");
            let n = p
                .params
                .get("limit")
                .and_then(|v| v.as_u64())
                .unwrap_or(10)
                .clamp(1, 100) as usize;
            let minimo = p
                .params
                .get("min")
                .and_then(|v| v.as_f64())
                .map(|x| x as f32)
                .unwrap_or(crate::search::PARECIDO_MINIMO);
            match crate::search::parecidas_desde(db, q, n, minimo) {
                Ok(v) => Respuesta::bien(
                    &p.id,
                    serde_json::json!({
                        "model": crate::vectores::listo(),
                        "items": v.into_iter()
                            .map(|(id, p)| {
                                let titulo = crate::search::una(db, &id).ok().flatten()
                                    .map(|f| f.title).unwrap_or_default();
                                serde_json::json!({ "id": id, "title": titulo, "score": p })
                            })
                            .collect::<Vec<_>>()
                    }),
                ),
                Err(e) => Respuesta::mal(&p.id, "db", e.to_string()),
            }
        }

        //  Semantic-similarity vectors, N at a time.
        //
        //  There is no network here, nor is one needed: the model is on
        //  disk. Process items in batches so installing it with two
        //  thousand captures does not block the library for five minutes.
        //  The caller is the same review process that fetches covers and
        //  summaries.
        "embed" => {
            let n = p
                .params
                .get("limit")
                .and_then(|v| v.as_i64())
                .unwrap_or(64)
                .clamp(1, 512);
            match crate::search::vectorizar_pendientes(db, n) {
                Ok((hechas, faltan)) => Respuesta::bien(
                    &p.id,
                    serde_json::json!({
                        "done": hechas,
                        "pending": faltan,
                        "model": crate::vectores::listo(),
                    }),
                ),
                Err(e) => Respuesta::mal(&p.id, "db", e.to_string()),
            }
        }

        "get" => {
            let id = p.params.get("id").and_then(|v| v.as_str()).unwrap_or("");
            match crate::search::una(db, id) {
                Ok(Some(f)) => Respuesta::bien(&p.id, serde_json::json!({ "item": f })),
                Ok(None) => Respuesta::mal(&p.id, "no_existe", "nothing exists with that ID"),
                Err(e) => Respuesta::mal(&p.id, "db", e.to_string()),
            }
        }

        "stats" => match crate::search::cuentas(db) {
            Ok(c) => Respuesta::bien(&p.id, serde_json::to_value(c).unwrap_or_default()),
            Err(e) => Respuesta::mal(&p.id, "db", e.to_string()),
        },

        "integrity_check" => {
            let profundo = p
                .params
                .get("deep")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            match crate::mantenimiento::integridad(db, profundo) {
                Ok(i) => Respuesta::bien(&p.id, serde_json::to_value(i).unwrap_or_default()),
                Err(e) => Respuesta::mal(&p.id, "db", e.to_string()),
            }
        }

        "repair_index" => match crate::mantenimiento::reparar_indice(db) {
            Ok(n) => Respuesta::bien(&p.id, serde_json::json!({ "reindexed": n })),
            Err(e) => Respuesta::mal(&p.id, "db", e.to_string()),
        },

        "backup" => {
            let conservar = p.params.get("keep").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
            match crate::mantenimiento::respaldar(db, conservar) {
                Ok(c) => Respuesta::bien(&p.id, serde_json::to_value(c).unwrap_or_default()),
                Err(e) => Respuesta::mal(&p.id, "db", e.to_string()),
            }
        }

        "export" => {
            let dir = p
                .params
                .get("directory")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if dir.is_empty() || !std::path::Path::new(dir).is_absolute() {
                return Respuesta::mal(&p.id, "params", "an absolute path is required");
            }
            match crate::mantenimiento::exportar(db, std::path::Path::new(dir)) {
                Ok(x) => Respuesta::bien(&p.id, serde_json::to_value(x).unwrap_or_default()),
                Err(e) => Respuesta::mal(&p.id, "io", e),
            }
        }

        //  Move to trash and restore from it. Permanent deletion is
        //  different and has its own method: the plan separates export from
        //  delete, and for the same reason separates trashing from
        //  destruction.
        "trash" | "untrash" => {
            //  One by its `id`, as ever, or several at once in `ids`: clearing
            //  her page throws away everything it shows, in one transaction,
            //  so it is all or nothing. Immediate: the write lock first, so a
            //  busy library makes it wait instead of failing.
            let ids = ids_de(&p.params);
            if ids.is_empty() {
                return Respuesta::mal(&p.id, "params", "an id, or a list of them in ids");
            }
            let tirar = p.method == "trash";
            let cuando = if tirar {
                Some(crate::util::ahora_ms())
            } else {
                None
            };
            let r = (|| -> rusqlite::Result<Vec<String>> {
                let tx = rusqlite::Transaction::new_unchecked(db, TransactionBehavior::Immediate)?;
                let mut hechas = Vec::new();
                for id in &ids {
                    let n = tx.execute(
                        "UPDATE captures SET trashed_at = ?2 WHERE id = ?1",
                        rusqlite::params![id, cuando],
                    )?;
                    if n > 0 {
                        hechas.push(id.clone());
                    }
                }
                //  Update the index too: trashed items stop appearing in
                //  search, and restored items appear again.
                crate::db::desindexar_varias(&tx, &hechas)?;
                if !tirar {
                    for id in &hechas {
                        crate::db::indexar(&tx, id)?;
                    }
                }
                tx.commit()?;
                Ok(hechas)
            })();
            match r {
                Ok(hechas) if p.params.get("ids").is_none() => match hechas.first() {
                    Some(id) => Respuesta::bien(&p.id, serde_json::json!({ "id": id })),
                    None => Respuesta::mal(&p.id, "no_existe", "nothing exists with that ID"),
                },
                Ok(hechas) => Respuesta::bien(
                    &p.id,
                    serde_json::json!({ "count": hechas.len(), "ids": hechas }),
                ),
                Err(e) => Respuesta::mal(&p.id, "db", e.to_string()),
            }
        }

        //  Deleting for real, which is not throwing away: the rows, what the
        //  index says of them and, once no other capture points at them, their
        //  files and pictures. Only what is named by its id. The rows go
        //  first, in one immediate transaction that also works out which files
        //  nobody uses any more; the files after: a cut between the two leaves
        //  a file nobody owns —seen, counted and swept, like any other— and
        //  never a row pointing at a file that is gone. The files are the live
        //  library's: on another database (`--db`, a backup) only its rows go.
        "purge" => {
            let ids = ids_de(&p.params);
            if ids.is_empty() {
                return Respuesta::mal(&p.id, "params", "an id, or a list of them in ids");
            }
            let viva = es_la_viva(db);
            let r = (|| -> rusqlite::Result<(Vec<String>, u64, u64)> {
                let tx = rusqlite::Transaction::new_unchecked(db, TransactionBehavior::Immediate)?;
                let (mut hechas, mut hashes) = (Vec::new(), Vec::new());
                for id in &ids {
                    let fila: Option<(Option<String>, Option<String>)> = tx
                        .query_row(
                            "SELECT blob_hash, preview_hash FROM captures WHERE id = ?1",
                            [id],
                            |r| Ok((r.get(0)?, r.get(1)?)),
                        )
                        .optional()?;
                    let Some((blob, miniatura)) = fila else {
                        continue;
                    };
                    hashes.extend(blob);
                    hashes.extend(miniatura);
                    hechas.push(id.clone());
                }
                crate::db::desindexar_varias(&tx, &hechas)?;
                //  Its spaces, tags, vectors and the rest go with it: their
                //  foreign keys cascade.
                for id in &hechas {
                    tx.execute("DELETE FROM captures WHERE id = ?1", [id])?;
                }
                hashes.sort();
                hashes.dedup();
                let mut sueltos = Vec::new();
                for h in hashes {
                    //  A hash is hex, and long: anything else is not one of
                    //  ours, and is not turned into a path.
                    if h.len() < 16 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
                        continue;
                    }
                    let usado: i64 = tx.query_row(
                        "SELECT COUNT(*) FROM captures WHERE blob_hash = ?1 OR preview_hash = ?1",
                        [&h],
                        |r| r.get(0),
                    )?;
                    if usado == 0 {
                        sueltos.push(h);
                    }
                }
                //  The files go while the write lock is still held: an ingest
                //  of the same bytes waits for it, and finds the file gone
                //  when it checks after writing its row (see `ingest`).
                let (mut ficheros, mut bytes) = (0u64, 0u64);
                if viva {
                    for h in &sueltos {
                        let sitio = crate::paths::blob_de(h);
                        let peso = std::fs::metadata(&sitio).map(|m| m.len()).unwrap_or(0);
                        if std::fs::remove_file(&sitio).is_ok() {
                            ficheros += 1;
                            bytes += peso;
                        }
                    }
                }
                tx.commit()?;
                Ok((hechas, ficheros, bytes))
            })();
            match r {
                Ok((hechas, _, _)) if hechas.is_empty() && p.params.get("ids").is_none() => {
                    Respuesta::mal(&p.id, "no_existe", "nothing exists with that ID")
                }
                Ok((hechas, ficheros, bytes)) => Respuesta::bien(
                    &p.id,
                    serde_json::json!({
                        "count": hechas.len(), "ids": hechas, "files": ficheros, "bytes": bytes,
                        "files_kept": !viva,
                    }),
                ),
                Err(e) => Respuesta::mal(&p.id, "db", e.to_string()),
            }
        }

        //  ── resurfacing ─────────────────────────────────────────
        //
        //  The engine decides WHAT; the caller that displays it decides
        //  when and how often, because it knows whether you are recording
        //  or whether it is midnight. This provides the data needed to
        //  decide.
        "revisit" => {
            let espacio = p.params.get("space").and_then(|v| v.as_str());
            let r = (|| -> rusqlite::Result<serde_json::Value> {
                Ok(serde_json::json!({
                    "item": crate::reaparicion::siguiente(db, espacio)?,
                    "shown_today": crate::reaparicion::mostradas_hoy(db)?,
                    "dismissed_in_a_row": crate::reaparicion::descartes_seguidos(db)?,
                    "last_shown_at": crate::reaparicion::ultima_aparicion(db)?,
                }))
            })();
            match r {
                Ok(v) => Respuesta::bien(&p.id, v),
                Err(e) => Respuesta::mal(&p.id, "db", e.to_string()),
            }
        }

        //  Record that it was displayed, then what happened. These are two
        //  methods because time passes between them: displaying is now, and
        //  you may act a minute later or never.
        "revisit_shown" => {
            let id = p.params.get("id").and_then(|v| v.as_str()).unwrap_or("");
            let razon = p
                .params
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            match crate::reaparicion::apuntar_mostrada(db, id, razon) {
                Ok(evento) => Respuesta::bien(&p.id, serde_json::json!({ "event": evento })),
                Err(e) => Respuesta::mal(&p.id, "db", e.to_string()),
            }
        }
        "revisit_outcome" => {
            let evento = p.params.get("event").and_then(|v| v.as_i64()).unwrap_or(0);
            let cual = p
                .params
                .get("outcome")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            match crate::reaparicion::apuntar_resultado(db, evento, cual) {
                Ok(true) => Respuesta::bien(&p.id, serde_json::json!({ "ok": true })),
                Ok(false) => Respuesta::mal(
                    &p.id,
                    "params",
                    format!(
                        "resultado desconocido; los que hay son {:?}",
                        crate::reaparicion::RESULTADOS
                    ),
                ),
                Err(e) => Respuesta::mal(&p.id, "db", e.to_string()),
            }
        }

        //  ── importing existing items ────────────────────────────
        //
        //  Three entry points, all explicit: none runs on its own, watches
        //  a directory, or accesses the network. The user chooses the path
        //  and the controller validates it, just as for `export`.
        "import" => {
            let desde = p.params.get("directory").and_then(|v| v.as_str());
            let fichero = p.params.get("file").and_then(|v| v.as_str());
            let que = p.params.get("kind").and_then(|v| v.as_str()).unwrap_or("");
            let espacio = p.params.get("space").and_then(|v| v.as_str());
            let tope = p
                .params
                .get("limit")
                .and_then(|v| v.as_u64())
                .unwrap_or(5000) as usize;

            fn absoluta(r: Option<&str>) -> Option<&str> {
                r.filter(|x| !x.is_empty() && std::path::Path::new(x).is_absolute())
            }
            let r = match que {
                "folder" => match absoluta(desde) {
                    Some(d) => Ok(crate::importar::carpeta(
                        db,
                        std::path::Path::new(d),
                        tope.clamp(1, 50_000),
                    )),
                    None => Err("an absolute directory is required"),
                },
                "bookmarks" => match absoluta(fichero) {
                    Some(f) => Ok(crate::importar::marcadores(
                        db,
                        std::path::Path::new(f),
                        espacio,
                    )),
                    None => Err("an absolute file path is required"),
                },
                "deriva" => match absoluta(desde) {
                    Some(d) => Ok(crate::importar::desde_export(db, std::path::Path::new(d))),
                    None => Err("an absolute directory is required"),
                },
                _ => Err("unsupported import type; expected folder, bookmarks, or deriva"),
            };
            match r {
                Ok(v) => Respuesta::bien(&p.id, serde_json::to_value(v).unwrap_or_default()),
                Err(e) => Respuesta::mal(&p.id, "params", e),
            }
        }

        //  ── the health report ───────────────────────────────────
        //
        //  Everything needed to diagnose a problem, in one response. The
        //  first diagnostic question is always “what version of what is
        //  talking to what?”, and answering it requires checking four places.
        "doctor" => {
            //  A boolean, not the full report: the health report should be
            //  readable at a glance, and `integrity-check` provides the
            //  details.
            let entera = crate::mantenimiento::integridad(db, false)
                .map(|x| x.ok)
                .unwrap_or(false);
            let c = crate::search::cuentas(db);
            let esquema: i64 = db
                .query_row("PRAGMA user_version", [], |r| r.get(0))
                .unwrap_or(-1);
            let copias = std::fs::read_dir(crate::paths::backups())
                .map(|d| d.filter_map(|e| e.ok()).count())
                .unwrap_or(0);
            Respuesta::bien(
                &p.id,
                serde_json::json!({
                    "version": env!("CARGO_PKG_VERSION"),
                    "schema": esquema,
                    "schema_expected": crate::db::VERSION,
                    "base": crate::paths::base().to_string_lossy(),
                    "socket": crate::paths::socket().to_string_lossy(),
                    "backups": copias,
                    "stats": c.ok(),
                    "integrity": entera,
                }),
            )
        }

        "shutdown" => Respuesta::bien(&p.id, serde_json::json!({ "bye": true })),

        otro => Respuesta::mal(&p.id, "metodo", format!("unknown method '{}'", otro)),
    }
}

//  The captures a request names: one in `id`, or a list in `ids`.
fn ids_de(params: &serde_json::Value) -> Vec<String> {
    if let Some(lista) = params.get("ids").and_then(|v| v.as_array()) {
        return lista
            .iter()
            .filter_map(|x| x.as_str())
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect();
    }
    params
        .get("id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| vec![s.to_string()])
        .unwrap_or_default()
}

//  Whether this connection is the live library, whose files are in the blob
//  store: a backup opened with `--db` shares the store but not its rows, so
//  counting who uses a file there says nothing about the live ones.
fn es_la_viva(db: &Connection) -> bool {
    let Some(p) = db.path() else { return false };
    let viva = crate::paths::db_path();
    match (std::fs::canonicalize(p), std::fs::canonicalize(&viva)) {
        (Ok(a), Ok(b)) => a == b,
        _ => std::path::Path::new(p) == viva.as_path(),
    }
}
