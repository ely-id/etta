//  El esquema y sus migraciones.
//
//  El esquema es el del plan, letra por letra. Lo que añade este fichero es
//  cómo se llega a él y cómo se sigue llegando cuando cambie: `user_version`,
//  que es el contador que SQLite ya trae y que no hay que inventarse una tabla
//  para llevar.
//
//  Una migración es una función que solo sube. No hay bajada: deshacer un
//  cambio de esquema sobre datos reales es una operación que se prueba una vez
//  y se ejecuta nunca, y tenerla escrita da la falsa impresión de que hay red.
//  Para volver atrás está la copia de seguridad, que sí se prueba.
//
//  El índice de búsqueda NO se mantiene con disparadores. Las etiquetas viven
//  en otra tabla, así que un disparador sobre `captures` no las vería y harían
//  falta tres disparadores más sobre `capture_tags` que reconstruyeran la misma
//  fila. Se hace en `reindexar`, en un sitio, y se llama después de cualquier
//  cambio. Hay una prueba de que cambiar una etiqueta se busca.

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
    //  WAL para que leer no bloquee escribir: la página busca mientras el
    //  arrastre de al lado guarda, y con el diario clásico una de las dos
    //  espera. `synchronous=NORMAL` es lo que recomienda SQLite junto a WAL:
    //  un corte de corriente puede perder la última transacción, pero no
    //  corrompe la base, y aquí lo que se pierde es una captura que se puede
    //  volver a arrastrar.
    db.pragma_update(None, "journal_mode", "WAL")?;
    db.pragma_update(None, "synchronous", "NORMAL")?;
    db.pragma_update(None, "foreign_keys", true)?;
    //  Cinco segundos antes de rendirse con «database is locked». El worker es
    //  uno, pero los comandos de diagnóstico abren la misma base desde fuera.
    db.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(())
}

fn migrar(db: &Connection) -> Result<()> {
    let v: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if v >= VERSION {
        return Ok(());
    }

    //  Copia ANTES de migrar, y solo si había algo que copiar.
    //
    //  Es la regla del plan —«escrituras atómicas y copias antes de migrar»— y
    //  es la que convierte una migración en algo reversible. Una migración que
    //  sale mal sin copia deja una biblioteca de años en un estado que nadie
    //  sabe deshacer; con copia, deshacerla es mover un fichero.
    //
    //  En una base recién creada no hay nada que salvar, y guardar una copia de
    //  un fichero vacío solo confunde a quien mire la carpeta de copias
    //  buscando su biblioteca.
    //
    //  La pregunta es «¿hay algo dentro?», no «¿qué versión dice ser?»: una
    //  base de antes de que existiera `user_version` diría 0 teniendo dentro
    //  años de cosas, y esa es justo la que más falta hace copiar.
    let hay_algo: i64 = db
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if hay_algo > 0 {
        if let Err(e) = copia_antes_de_migrar(db, v) {
            //  Si no se puede copiar, NO se migra. Preferir migrar sin red
            //  sería exactamente al revés de para qué existe la copia.
            eprintln!("deriva-worker: no se pudo copiar antes de migrar: {}", e);
            return Err(e);
        }
    }

    if v < 1 {
        db.execute_batch(ESQUEMA_1)?;
    }
    //  A la 2: de qué sitio viene.
    //
    //  Va con `v >= 1` y no solo con `v < 2` porque `ESQUEMA_1` ya la trae: una
    //  base recién creada pasa por el bloque de arriba y llega aquí con la
    //  columna puesta, y `ADD COLUMN` sobre una que ya está es un error. Lo que
    //  se migra es lo que se creó ANTES de que existiera.
    //
    //  Sin esto, una biblioteca de ayer se abre sin la columna y todo lo que
    //  lee una captura falla con «no such column: source». O sea: la biblioteca
    //  entera vacía, y por una columna que se añadió a mano al esquema.
    if v >= 1 && v < 2 {
        db.execute_batch(A_LA_2)?;
    }
    //  A la 3: el resumen de la IA, que va en su propia columna Y en el
    //  índice.
    //
    //  No en `note` —esa es tuya y no la escribe nadie más— ni en `excerpt`,
    //  que es el principio del texto tal cual y sirve para saber qué había de
    //  verdad. Un resumen es otra cosa: lo ha escrito un modelo, puede
    //  equivocarse, y hay que poder distinguirlo de lo que dice el original.
    //  A la 4: cuándo se miró.
    //
    //  Sin esto, lo que no se puede resumir se pide otra vez cada veinte
    //  segundos, para siempre y pagando. Pasó de verdad: una página de Reddit
    //  que no da texto a quien no ejecuta JavaScript se quedó de pendiente
    //  eterna, y el modelo hacía bien en callarse cada vez.
    //
    //  Se apunta el intento, no el resultado. «No se pudo decir nada» es una
    //  respuesta y hay que recordar que ya se preguntó.
    //  A la 5: los vectores del parecido por significado.
    //
    //  En su propia tabla y no en una columna de `captures` porque son otra
    //  cosa: se pueden borrar enteros y rehacerlos con otro modelo sin tocar ni
    //  una captura, y eso va a pasar el día que haya un modelo mejor.
    if v >= 1 && v < 5 {
        db.execute_batch(A_LA_5)?;
    }
    if v >= 1 && v < 4 {
        db.execute_batch(A_LA_4)?;
    }
    if v >= 1 && v < 3 {
        //  El índice se rehace entero, y no hay otra manera: a una tabla FTS5
        //  no se le añade una columna. Se tira y se vuelve a llenar, que en una
        //  biblioteca de miles de filas son unas décimas y pasa una sola vez.
        //
        //  Y hay que rehacerlo: un resumen que no está en el índice es un
        //  resumen que no encuentra nadie, ni tú ni la IA, y entonces no sirve
        //  para lo único para lo que se escribió.
        //  Se rellena en la 6, que vuelve a cambiarle las columnas: llenarlo
        //  aquí con el `reindexar` de hoy escribiría una columna que esta
        //  tabla todavía no tiene.
        db.execute_batch(A_LA_3)?;
    }
    //  A la 6: de qué sitio es cada enlace, en el índice.
    //
    //  «el vídeo de youtube» no encontraba ningún vídeo de YouTube: la
    //  dirección estaba guardada pero no se buscaba en ella, y el título de un
    //  vídeo casi nunca dice dónde está. Otra columna, así que otra vez tirar
    //  la tabla y volver a llenarla.
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
    //  `VACUUM INTO` y no copiar el fichero: con WAL, parte de lo último
    //  escrito vive en el `-wal` y copiar a mano da una copia rota que además
    //  parece buena.
    db.execute("VACUUM INTO ?1", [destino.to_string_lossy().to_string()])?;
    eprintln!(
        "deriva-worker: migrando de v{} a v{}; copia en {}",
        desde,
        VERSION,
        destino.display()
    );
    Ok(())
}

//  El esquema del plan, más lo que hace falta para que funcione: los índices
//  por los que se pregunta de verdad y la tabla de búsqueda.
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
    --  La miniatura, por su hash y en el mismo almacén que todo lo demás.
    --
    --  Antes era una ruta —`preview_path`—, y una ruta guardada en la base es
    --  un dato que caduca: la carpeta se mueve con `MAREA_DERIVA_DIR`, y la
    --  base se lleva a otro equipo en una copia. El hash no se mueve; la ruta
    --  se calcula al leer, igual que la del fichero original.
    preview_hash TEXT,
    status TEXT NOT NULL DEFAULT 'saved',
    --  De qué sitio viene: youtube, reddit, github… Lo saca el adaptador de la
    --  propia URL, sin pedirle nada a nadie, y la interfaz lo usa para
    --  enseñarlo como lo que es en vez de como «un enlace».
    source TEXT NOT NULL DEFAULT '',
    --  Lo que la IA entendió de esto, si se lo has pedido. En su propia
    --  columna: `note` es tuya y `excerpt` es el original recortado, y un
    --  resumen escrito por un modelo no es ninguna de las dos cosas.
    summary TEXT NOT NULL DEFAULT '',
    --  Cuándo se le pidió a la IA que lo mirara, salga lo que salga. No es lo
    --  mismo que tener resumen: hay cosas de las que no se puede decir nada, y
    --  sin apuntar el intento se vuelven a preguntar para siempre.
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

--  El parecido por significado.
--
--  Tabla aparte y no una columna de `captures`: son de un modelo concreto, y el
--  día que haya uno mejor esto se vacía y se rehace sin tocar ni una captura.
--  `dim` va con cada uno porque otro modelo da otra medida, y comparar vectores
--  de medidas distintas no da un error: da un parecido inventado.
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

--  Por lo que se pregunta de verdad. El duplicado se busca por estos tres, y
--  sin índice cada arrastre leería la biblioteca entera para contestar.
CREATE INDEX captures_canonical ON captures(canonical_url) WHERE canonical_url IS NOT NULL;
CREATE INDEX captures_content_hash ON captures(content_hash) WHERE content_hash IS NOT NULL;
CREATE INDEX captures_blob_hash ON captures(blob_hash) WHERE blob_hash IS NOT NULL;
--  Y la lista, que siempre va por fecha y sin lo tirado.
CREATE INDEX captures_recientes ON captures(trashed_at, captured_at DESC);

--  La búsqueda. Guarda su propia copia del texto en vez de apuntar a
--  `captures` con `content=`: las etiquetas no están en esa tabla, así que una
--  tabla de contenido externo no las podría indexar. Ocupa más y es correcto;
--  el truco de ahorrar espacio dejaría fuera justo lo que el plan pide indexar.
--  Los tres espacios que nombra el plan. Se siembran porque la navegación de
--  la lámina los enseña por su nombre, y una biblioteca recién hecha con la
--  barra lateral vacía no se parece a lo que se diseñó. Se pueden borrar; lo
--  que no se puede es empezar sin ninguno y que la página tenga sentido.
INSERT INTO spaces (id, name, color) VALUES
    ('proyectos',  'Proyectos',   '#8fbfa8'),
    ('leer-luego', 'Leer luego',  '#8aa4d6'),
    ('inspiracion','Inspiración', '#d8a244');

CREATE VIRTUAL TABLE captures_fts USING fts5(
    capture_id UNINDEXED,
    title, author, excerpt, summary, note, content_text, tags, site,
    tokenize = "unicode61 remove_diacritics 2"
);
"#;

//  Vuelca al índice lo que ahora mismo dice la base sobre una captura. Se llama
//  después de crearla, de editarla y de tocarle las etiquetas.
pub fn reindexar(db: &Connection, id: &str) -> Result<()> {
    db.execute("DELETE FROM captures_fts WHERE capture_id = ?1", [id])?;
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
    //  Lo tirado a la basura no se indexa. Es lo que hace que buscar no lo
    //  encuentre sin tener que filtrar en cada consulta.
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
        rusqlite::params![id, title, author, excerpt, summary, note, texto, etiquetas,
                          crate::util::sitio_buscable(&direccion)],
    )?;
    Ok(())
}
