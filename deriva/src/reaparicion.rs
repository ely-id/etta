//  What resurfaces for you, and why that one.
//
//  The engine is LOCAL AND DETERMINISTIC, as required by the plan; this is not
//  an implementation detail. It means the reason something resurfaced can be
//  explained in one sentence, and the same library at the same moment returns
//  the same result. A model choosing what to remind you about would be smarter
//  and completely unauditable.
//
//  It also infers nothing about you. It records what happened with each
//  appearance —opened, snoozed, dismissed—and uses that to STOP INSISTING. The
//  plan explicitly says: “it is not used to infer the user's emotional state.”

use rusqlite::{params, Connection};
use serde::Serialize;

//  How long since you opened it for it to stop counting as recent.
const RECIENTE_DIAS: i64 = 7;
//  How many times something can be shown without an action before it goes quiet.
const INSISTIR_MAX: i64 = 3;
//  What counts as a “forgotten favorite” and a “short pending read.”
const OLVIDADO_DIAS: i64 = 30;
const LECTURA_CORTA: i64 = 2000; //  caracteres

#[derive(Debug, Serialize)]
pub struct Devuelta {
    pub item: crate::search::Fila,
    //  Why this one. This is a closed enumeration exposed directly to the
    //  interface: if the reason cannot be named, the capture is not eligible.
    pub reason: String,
    //  Also expose it as text, so QML does not have to translate enum values.
    pub why: String,
}

//  The plan's complete eligibility rule, in one place.
//
//  Deliberately express it as SQL rather than a Rust filter: there are five
//  conditions on the same table, and doing this in memory would mean reading
//  the entire library to discard 99%.
fn elegibles() -> &'static str {
    "c.trashed_at IS NULL
     AND (c.snoozed_until IS NULL OR c.snoozed_until <= :ahora)
     AND (c.last_opened_at IS NULL OR c.last_opened_at < :hace_poco)
     AND (SELECT COUNT(*) FROM revisit_events r
           WHERE r.capture_id = c.id AND r.outcome IS NULL) < :insistir
     AND c.id NOT IN (
         SELECT cs.capture_id FROM capture_spaces cs
          JOIN spaces s ON s.id = cs.space_id
         WHERE s.color = 'silenciado')"
}

//  The priority order, also from the plan:
//
//      explicit date > associated project or application > ignored favorite
//        > short pending read > random old item with diversity
//
//  Evaluate them in order and stop at the first one that returns a result. An
//  `ORDER BY` mixing them all might return the same item but would not explain
//  it; this way, the displayed reason is literally the query that succeeded.
pub fn siguiente(
    db: &Connection,
    espacio_de_la_app: Option<&str>,
) -> rusqlite::Result<Option<Devuelta>> {
    let ahora = crate::util::ahora_ms();
    let hace_poco = ahora - RECIENTE_DIAS * 86_400_000;
    let olvidado = ahora - OLVIDADO_DIAS * 86_400_000;

    //  1. Explicit date: you snoozed it until today. This is the only one you
    //     asked for.
    if let Some(id) = uno(
        db,
        &format!(
            "SELECT c.id FROM captures c
              WHERE {} AND c.snoozed_until IS NOT NULL
              ORDER BY c.snoozed_until DESC LIMIT 1",
            elegibles()
        ),
        &[
            (":ahora", &ahora),
            (":hace_poco", &hace_poco),
            (":insistir", &INSISTIR_MAX),
        ],
    )? {
        return armar(db, &id, "scheduled", "you snoozed this until today");
    }

    //  2. The folder associated with what is in front of you. Association is
    //     opt-in and comes from outside; no windows are inspected here.
    if let Some(espacio) = espacio_de_la_app {
        if !espacio.is_empty() {
            if let Some(id) = uno(
                db,
                &format!(
                    "SELECT c.id FROM captures c
                      JOIN capture_spaces cs ON cs.capture_id = c.id
                     WHERE {} AND cs.space_id = :espacio
                     ORDER BY c.captured_at DESC LIMIT 1",
                    elegibles()
                ),
                &[
                    (":ahora", &ahora),
                    (":hace_poco", &hace_poco),
                    (":insistir", &INSISTIR_MAX),
                    (":espacio", &espacio),
                ],
            )? {
                return armar(db, &id, "app_context", "related to what you have open");
            }
        }
    }

    //  3. Forgotten favorite: you marked it and have not looked at it for a month.
    if let Some(id) = uno(
        db,
        &format!(
            "SELECT c.id FROM captures c
              WHERE {} AND c.favorite = 1
                AND (c.last_opened_at IS NULL OR c.last_opened_at < :olvidado)
              ORDER BY c.captured_at LIMIT 1",
            elegibles()
        ),
        &[
            (":ahora", &ahora),
            (":hace_poco", &hace_poco),
            (":insistir", &INSISTIR_MAX),
            (":olvidado", &olvidado),
        ],
    )? {
        return armar(
            db,
            &id,
            "forgotten_favorite",
            "you marked this and have not opened it in a month",
        );
    }

    //  4. Short pending read. Deliberately short: resurfacing a ten-thousand-
    //     word essay while you work is not help, but a well-intentioned
    //     interruption.
    if let Some(id) = uno(
        db,
        &format!(
            "SELECT c.id FROM captures c
              JOIN capture_spaces cs ON cs.capture_id = c.id
             WHERE {} AND cs.space_id = 'leer-luego'
               AND LENGTH(c.content_text) BETWEEN 1 AND :corta
             ORDER BY c.captured_at LIMIT 1",
            elegibles()
        ),
        &[
            (":ahora", &ahora),
            (":hace_poco", &hace_poco),
            (":insistir", &INSISTIR_MAX),
            (":corta", &LECTURA_CORTA),
        ],
    )? {
        return armar(db, &id, "short_read", "a short read you saved for later");
    }

    //  5. Something old, with diversity.
    //
    //  Truly “random” selection would repeat the same area: if you have four
    //  hundred architecture captures and twelve about cooking, pure random
    //  selection would almost always show architecture. Choose the least-
    //  recently viewed folder first, then its oldest item, so the entire
    //  library rotates instead of just one corner.
    if let Some(id) = uno(
        db,
        &format!(
            "SELECT c.id FROM captures c
              LEFT JOIN capture_spaces cs ON cs.capture_id = c.id
             WHERE {}
             ORDER BY (SELECT COUNT(*) FROM revisit_events r2
                        JOIN capture_spaces cs2 ON cs2.capture_id = r2.capture_id
                       WHERE cs2.space_id = COALESCE(cs.space_id, '')) ASC,
                      c.captured_at ASC
             LIMIT 1",
            elegibles()
        ),
        &[
            (":ahora", &ahora),
            (":hace_poco", &hace_poco),
            (":insistir", &INSISTIR_MAX),
        ],
    )? {
        return armar(db, &id, "old_random", "llevaba mucho sin salir");
    }

    Ok(None)
}

fn uno(
    db: &Connection,
    sql: &str,
    p: &[(&str, &dyn rusqlite::ToSql)],
) -> rusqlite::Result<Option<String>> {
    use rusqlite::OptionalExtension;
    db.query_row(sql, p, |r| r.get::<_, String>(0)).optional()
}

fn armar(
    db: &Connection,
    id: &str,
    razon: &str,
    porque: &str,
) -> rusqlite::Result<Option<Devuelta>> {
    Ok(crate::search::una(db, id)?.map(|item| Devuelta {
        item,
        reason: razon.into(),
        why: porque.into(),
    }))
}

//  Record that it was shown, with NO outcome yet. This lets us stop insisting
//  on something that is shown but never touched.
pub fn apuntar_mostrada(db: &Connection, id: &str, razon: &str) -> rusqlite::Result<i64> {
    db.execute(
        "INSERT INTO revisit_events (capture_id, reason, shown_at) VALUES (?1, ?2, ?3)",
        params![id, razon, crate::util::ahora_ms()],
    )?;
    Ok(db.last_insert_rowid())
}

//  What you did with it. Closed set: `opened`, `snoozed`, `dismissed`,
//  `saved_to_project`.
pub const RESULTADOS: &[&str] = &["opened", "snoozed", "dismissed", "saved_to_project"];

pub fn apuntar_resultado(db: &Connection, evento: i64, resultado: &str) -> rusqlite::Result<bool> {
    if !RESULTADOS.contains(&resultado) {
        return Ok(false);
    }
    let n = db.execute(
        "UPDATE revisit_events SET outcome = ?2 WHERE id = ?1 AND outcome IS NULL",
        params![evento, resultado],
    )?;
    Ok(n > 0)
}

//  How many consecutive appearances were ignored or dismissed, so resurfacing
//  can go quiet until tomorrow. The worker keeps the count because it has the
//  history; the caller applies the quiet period because it knows the user's
//  local time.
pub fn descartes_seguidos(db: &Connection) -> rusqlite::Result<i64> {
    let mut s = db.prepare(
        //  Sort by `id` as well as time: two appearances in the same
        //  millisecond were ordered randomly, making “consecutive” meaningless.
        //  The `id` is a counter and always increases.
        //
        //  And do NOT require `outcome IS NOT NULL`, which broke the entire
        //  circuit breaker. Filtering by outcome only showed items the user
        //  had manually dismissed, and manually dismissing one means opening
        //  the page, choosing the card, and pressing a 26-pixel button. Someone
        //  who does not want something does not make that trip; they ignore it.
        //  So the count stayed at zero forever —fifteen consecutive
        //  appearances, none with an outcome—and the promised quiet period
        //  never started.
        //
        //  Ignoring is the most common response, and now counts as what it is.
        "SELECT outcome FROM revisit_events
          WHERE shown_at >= ?1 ORDER BY shown_at DESC, id DESC LIMIT 8",
    )?;
    let ultimos: Vec<Option<String>> = s
        .query_map([inicio_de_hoy()], |r| r.get::<_, Option<String>>(0))?
        .filter_map(|x| x.ok())
        .collect();
    let mut n = 0;
    for o in ultimos {
        //  No outcome means “I showed it and nothing happened”; `dismissed`
        //  means “I've seen it.” Both mean there is no need to insist. Opening
        //  or snoozing breaks the streak because both mean it was useful.
        match o.as_deref() {
            None | Some("dismissed") => n += 1,
            _ => break,
        }
    }
    Ok(n)
}

//  When “today” starts. Keep this in one place because it is used by both the
//  daily limit and the quiet period, which the plan promises lasts “until the
//  next day”; without this line it would last forever.
fn inicio_de_hoy() -> i64 {
    (crate::util::ahora_ms() / 86_400_000) * 86_400_000
}

//  How many have been shown today, for the daily limit.
pub fn mostradas_hoy(db: &Connection) -> rusqlite::Result<i64> {
    db.query_row(
        "SELECT COUNT(*) FROM revisit_events WHERE shown_at >= ?1",
        [inicio_de_hoy()],
        |r| r.get(0),
    )
}

//  When the last one was shown, for the minimum gap between appearances.
//
//  The caller that displays them asks for this. It had the value in a QML
//  property and lost it on every reload. It has been on disk here all along:
//  the history table already stores it, so all we needed was to return it.
pub fn ultima_aparicion(db: &Connection) -> rusqlite::Result<i64> {
    db.query_row(
        "SELECT COALESCE(MAX(shown_at), 0) FROM revisit_events",
        [],
        |r| r.get(0),
    )
}
