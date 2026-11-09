//  The schema and its migrations.
//
//  The schema follows the plan, word for word. This file adds how to get to it
//  and how to keep getting there as it changes: `user_version`, the counter
//  SQLite already provides, so there is no need to invent a table to track it.
//
//  A migration is a function that only moves forward. There is no downgrade:
//  reversing a schema change on real data is an operation that gets tested
//  once and never run, and having one written down gives the false impression
//  that it is safe. Backups are the tested way to go back.
//
//  The search index is NOT maintained with triggers. Tags live in another
//  table, so a trigger on `captures` would not see them, and three more
//  triggers on `capture_tags` would be needed to rebuild the same row. This is
//  handled in one place, `reindexar`, which is called after every change. A
//  test verifies that changing a tag affects search.

use rusqlite::{Connection, Result};

pub const VERSION: i64 = 6;

pub fn abrir() -> Result<Connection> {
    crate::paths::preparar().ok();
    let db = Connection::open(crate::paths::db_path())?;
    preparar_conexion(&db)?;
    migrar(&db)?;
    Ok(db)
}

pub fn abrir_en(path: &std::path::Path) -> Result<Connection> {
    let db = Connection::open(path)?;
    preparar_conexion(&db)?;
    migrar(&db)?;
    Ok(db)
}

fn preparar_conexion(db: &Connection) -> Result<()> {
    //  WAL lets reads avoid blocking writes: the page can search while a
    //  nearby drag saves, whereas with the classic journal one has to wait.
    //  SQLite recommends `synchronous=NORMAL` with WAL: a power outage may
    //  lose the last transaction but will not corrupt the database, and
    //  what could be lost here is a capture that can be dragged in again.
    db.pragma_update(None, "journal_mode", "WAL")?;
    db.pragma_update(None, "synchronous", "NORMAL")?;
    db.pragma_update(None, "foreign_keys", true)?;
    //  Wait five seconds before giving up with “database is locked.” There
    //  is one worker, but diagnostic commands open the same database from
    //  outside it.
    db.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(())
}

fn migrar(db: &Connection) -> Result<()> {
    let v: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if v >= VERSION {
        return Ok(());
    }

    //  Copy BEFORE migrating, and only if there is something to copy.
    //
    //  This is the plan's rule —“atomic writes and backups before
    //  migrations”— and what makes a migration reversible. A failed
    //  migration without a backup leaves years of library data in a state
    //  nobody knows how to undo; with a backup, undoing it means moving a
    //  file.
    //
    //  A newly created database has nothing to save, and backing up an
    //  empty file would only confuse someone looking in the backup directory
    //  for their library.
    //
    //  The question is “is there anything inside?”, not “what version does
    //  it claim to be?” A database from before `user_version` existed would
    //  report 0 despite containing years of data, and that is exactly the
    //  one most in need of a backup.
    let hay_algo: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if hay_algo > 0 {
        if let Err(e) = copia_antes_de_migrar(db, v) {
            //  If it cannot be copied, do NOT migrate. Migrating without a
            //  backup would defeat the whole purpose of having one.
            eprintln!(
                "deriva-worker: could not create a pre-migration backup: {}",
                e
            );
            return Err(e);
        }
    }

    if v < 1 {
        db.execute_batch(ESQUEMA_1)?;
    }
    //  Migration 2: where the link came from.
    //
    //  Use `v >= 1`, not just `v < 2`, because `ESQUEMA_1` already includes
    //  it: a newly created database passes through the block above and
    //  arrives here with the column in place, and `ADD COLUMN` on an
    //  existing column is an error. This migrates databases created before
    //  the column existed.
    //
    //  Without this, yesterday's library opens without the column and every
    //  capture read fails with “no such column: source.” In other words, the
    //  entire library appears empty because a column was added manually to
    //  the schema.
    if v >= 1 && v < 2 {
        db.execute_batch(A_LA_2)?;
    }
    //  Migration 3: the AI summary, stored in its own column AND in the
    //  index.
    //
    //  Not in `note` —that belongs to you and nobody else writes it—or
    //  `excerpt`, which is the original beginning of the text and shows
    //  what was really there. A summary is different: a model wrote it, it
    //  can be wrong, and it must be distinguishable from the original.
    //  Migration 4: when it was processed.
    //
    //  Without this, anything that cannot be summarized is requested again
    //  every twenty seconds, forever and at a cost. This actually happened:
    //  a Reddit page that returns no text unless JavaScript runs stayed
    //  pending forever, and the model was right to say nothing each time.
    //
    //  Record the attempt, not the result. “Nothing could be said” is a
    //  response, and we must remember that we already asked.
    //  Migration 5: vectors for semantic similarity.
    //
    //  They have their own table, rather than a column in `captures`,
    //  because they are a separate concern: all of them can be deleted and
    //  rebuilt with another model without touching any capture, which will
    //  happen when a better model becomes available.
    if v >= 1 && v < 5 {
        db.execute_batch(A_LA_5)?;
    }
    if v >= 1 && v < 4 {
        db.execute_batch(A_LA_4)?;
    }
    if v >= 1 && v < 3 {
        //  Rebuild the entire index; there is no other way to add a column
        //  to an FTS5 table. Drop it and fill it again. For a library with
        //  thousands of rows this takes a fraction of a second and happens
        //  only once.
        //
        //  It must be rebuilt: a summary that is not in the index cannot be
        //  found by anyone, you or the AI, and then it is useless for the
        //  only purpose it was written for.
        //  Populate it in migration 6, which changes the columns again:
        //  using today's `reindexar` here would write a column this table
        //  does not have yet.
        db.execute_batch(A_LA_3)?;
    }
    //  Migration 6: the source of each link in the index.
    //
    //  “the youtube video” could not find any YouTube videos: the URL was
    //  saved but not searched, and a video's title rarely says where it is.
    //  Another column means dropping and refilling the table again.
    if v >= 1 && v < 6 {
        db.execute_batch(A_LA_6)?;
        crate::mantenimiento::reparar_indice(db)?;
    }
    db.pragma_update(None, "user_version", VERSION)?;
    Ok(())
}

fn copia_antes_de_migrar(db: &Connection, desde: i64) -> Result<()> {
    crate::paths::preparar().ok();
    let destino = crate::paths::backups().join(format!(
        "antes-de-v{}-{}.sqlite3",
        VERSION,
        crate::util::ahora_ms()
    ));
    //  Use `VACUUM INTO`, not a file copy: with WAL, some of the latest
    //  writes live in `-wal`, and copying by hand produces a broken backup
    //  that looks valid.
    db.execute("VACUUM INTO ?1", [destino.to_string_lossy().to_string()])?;
    eprintln!(
        "deriva-worker: migrando de v{} a v{}; copia en {}",
        desde,
        VERSION,
        destino.display()
    );
    Ok(())
}

//  The schema from the plan, plus what is needed to make it work: the indexes
//  actually queried and the search table.
const A_LA_2: &str = r#"
ALTER TABLE captures ADD COLUMN source TEXT NOT NULL DEFAULT '';
ALTER TABLE captures DROP COLUMN preview_path;
ALTER TABLE captures ADD COLUMN preview_hash TEXT;
"#;

const A_LA_5: &str = r#"
CREATE TABLE capture_vectors (
    capture_id TEXT PRIMARY KEY REFERENCES captures(id) ON DELETE CASCADE,
    dim INTEGER NOT NULL,
    hecho_en INTEGER NOT NULL,
    v BLOB NOT NULL
);
"#;

const A_LA_4: &str = r#"
ALTER TABLE captures ADD COLUMN summary_at INTEGER;
"#;

const A_LA_6: &str = r#"
DROP TABLE captures_fts;
CREATE VIRTUAL TABLE captures_fts USING fts5(
    capture_id UNINDEXED,
    title, author, excerpt, summary, note, content_text, tags, site,
    tokenize = "unicode61 remove_diacritics 2"
);
"#;

const A_LA_3: &str = r#"
ALTER TABLE captures ADD COLUMN summary TEXT NOT NULL DEFAULT '';
DROP TABLE captures_fts;
CREATE VIRTUAL TABLE captures_fts USING fts5(
    capture_id UNINDEXED,
    title, author, excerpt, summary, note, content_text, tags,
    tokenize = "unicode61 remove_diacritics 2"
);
"#;

const ESQUEMA_1: &str = r#"
CREATE TABLE captures (
    id TEXT PRIMARY KEY,
    type TEXT NOT NULL,
    source_url TEXT,
    canonical_url TEXT,
    title TEXT NOT NULL DEFAULT '',
    author TEXT NOT NULL DEFAULT '',
    excerpt TEXT NOT NULL DEFAULT '',
    note TEXT NOT NULL DEFAULT '',
    content_text TEXT NOT NULL DEFAULT '',
    content_hash TEXT,
    blob_hash TEXT,
    blob_bytes INTEGER,
    blob_mime TEXT,
    blob_name TEXT,
    --  The preview, identified by its hash and kept in the same store as
    --
    --  everything else. It used to be a path (`preview_path`), but a path
    --  stored in the database becomes stale when the directory moves via
    --  `ETTA_DERIVA_DIR` or the database is copied to another machine. The
    --  hash remains valid; the path is calculated when reading, as it is
    --  for the original file.
    preview_hash TEXT,
    status TEXT NOT NULL DEFAULT 'saved',
    --  The source site (YouTube, Reddit, GitHub, and others). The adapter
    --  derives it from the URL without querying anyone, and the interface uses
    --  it to show the capture as what it is rather than just "a link."
    source TEXT NOT NULL DEFAULT '',
    --  What the AI understood, if asked. Kept in its own column: `note` is
    --  yours and `excerpt` is the original text, while a model-written
    --  summary is neither of those.
    summary TEXT NOT NULL DEFAULT '',
    --  When the AI was asked to process it, regardless of the result. This
    --  differs from having a summary: some items cannot be summarized, and
    --  without recording the attempt they would be requested forever.
    summary_at INTEGER,
    captured_at INTEGER NOT NULL,
    last_opened_at INTEGER,
    snoozed_until INTEGER,
    favorite INTEGER NOT NULL DEFAULT 0,
    trashed_at INTEGER
);

CREATE TABLE spaces (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    color TEXT NOT NULL DEFAULT ''
);

CREATE TABLE capture_spaces (
    capture_id TEXT NOT NULL REFERENCES captures(id) ON DELETE CASCADE,
    space_id TEXT NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
    PRIMARY KEY (capture_id, space_id)
);

CREATE TABLE tags (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);

--  Semantic similarity.
--
--  A separate table instead of a `captures` column: vectors belong to a
--  specific model and can be rebuilt with a better model without touching any
--  captures. Each vector stores its `dim` because other models have different
--  dimensions; comparing vectors of different dimensions does not fail, it
--  produces a fabricated similarity.
CREATE TABLE capture_vectors (
    capture_id TEXT PRIMARY KEY REFERENCES captures(id) ON DELETE CASCADE,
    dim INTEGER NOT NULL,
    hecho_en INTEGER NOT NULL,
    v BLOB NOT NULL
);

CREATE TABLE capture_tags (
    capture_id TEXT NOT NULL REFERENCES captures(id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (capture_id, tag_id)
);

CREATE TABLE tab_sessions (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    browser TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE tab_session_items (
    session_id TEXT NOT NULL REFERENCES tab_sessions(id) ON DELETE CASCADE,
    capture_id TEXT NOT NULL REFERENCES captures(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    group_name TEXT,
    PRIMARY KEY (session_id, position)
);

CREATE TABLE revisit_events (
    id INTEGER PRIMARY KEY,
    capture_id TEXT NOT NULL REFERENCES captures(id) ON DELETE CASCADE,
    reason TEXT NOT NULL,
    shown_at INTEGER NOT NULL,
    outcome TEXT
);

--  The fields used for duplicate detection. Without an index, every drop would
--  scan the entire library.
CREATE INDEX captures_canonical ON captures(canonical_url) WHERE canonical_url IS NOT NULL;
CREATE INDEX captures_content_hash ON captures(content_hash) WHERE content_hash IS NOT NULL;
CREATE INDEX captures_blob_hash ON captures(blob_hash) WHERE blob_hash IS NOT NULL;
--  The list is always ordered by date and excludes trashed captures.
CREATE INDEX captures_recientes ON captures(trashed_at, captured_at DESC);

--  Search keeps its own copy of the text instead of indexing `captures` with
--  `content=`: tags are not in that table, so an external-content table could
--  not index them. It uses more space, but saving space would leave out exactly
--  what the plan requires us to index.
--  The three spaces named in the plan. They are seeded because the design's
--  navigation shows them by name, and a new library with an empty sidebar
--  would not match the design. They can be deleted, but the page needs at
--  least one space to make sense.
INSERT INTO spaces (id, name, color) VALUES
    ('proyectos',  'Projects',    '#8fbfa8'),
    ('leer-luego', 'Read later',  '#8aa4d6'),
    ('inspiracion','Inspiration', '#d8a244');

CREATE VIRTUAL TABLE captures_fts USING fts5(
    capture_id UNINDEXED,
    title, author, excerpt, summary, note, content_text, tags, site,
    tokenize = "unicode61 remove_diacritics 2"
);
"#;

//  Write the database's current representation of a capture to the index. Call
//  after creating it, editing it, or changing its tags.
pub fn reindexar(db: &Connection, id: &str) -> Result<()> {
    db.execute("DELETE FROM captures_fts WHERE capture_id = ?1", [id])?;
    indexar(db, id)
}

//  The index rows of many captures, in one statement. `capture_id` is not an
//  indexed column of the FTS table, so each delete by id reads the whole
//  table: five hundred of them, one by one, held the write lock for seconds
//  and a drop arriving meanwhile failed with «database is locked».
pub fn desindexar_varias(db: &Connection, ids: &[String]) -> Result<()> {
    let lista = serde_json::to_string(ids).unwrap_or_else(|_| "[]".into());
    db.execute(
        "DELETE FROM captures_fts WHERE capture_id IN (SELECT value FROM json_each(?1))",
        [lista],
    )?;
    Ok(())
}

//  Writes the index row of a capture whose old one is already gone.
pub fn indexar(db: &Connection, id: &str) -> Result<()> {
    let fila = db.query_row(
        "SELECT title, author, excerpt, summary, note, content_text,
                COALESCE(canonical_url, source_url, '') FROM captures
         WHERE id = ?1 AND trashed_at IS NULL",
        [id],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
            ))
        },
    );
    //  Trashed items are not indexed. This keeps them out of search without
    //  requiring a filter on every query.
    let (title, author, excerpt, summary, note, texto, direccion) = match fila {
        Ok(f) => f,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(()),
        Err(e) => return Err(e),
    };
    let etiquetas: String = {
        let mut s = db.prepare(
            "SELECT t.name FROM tags t JOIN capture_tags ct ON ct.tag_id = t.id
             WHERE ct.capture_id = ?1 ORDER BY t.name",
        )?;
        let v: Vec<String> = s
            .query_map([id], |r| r.get::<_, String>(0))?
            .filter_map(|x| x.ok())
            .collect();
        v.join(" ")
    };
    db.execute(
        "INSERT INTO captures_fts
         (capture_id, title, author, excerpt, summary, note, content_text, tags, site)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            id,
            title,
            author,
            excerpt,
            summary,
            note,
            texto,
            etiquetas,
            crate::util::sitio_buscable(&direccion)
        ],
    )?;
    Ok(())
}
