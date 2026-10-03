//  Buscar, y leer lo encontrado.
//
//  FTS5 con BM25, que es lo que pide el plan: literal, frases, prefijos y
//  fragmentos. La búsqueda semántica es un índice opcional posterior y no está
//  aquí; poner un «casi» ahora sería peor que no tenerla, porque la gente
//  aprende lo que la caja sabe hacer el primer día.
//
//  Lo que este fichero se toma en serio es que **lo que escribe el usuario no
//  es una consulta FTS5**. Un apóstrofo, un asterisco suelto o unas comillas
//  sin cerrar son sintaxis en ese lenguaje, y pasarlas tal cual da un error de
//  sintaxis en vez de resultados. Buscar `it's` no puede fallar.

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
    //  Dónde está el fichero, ya montado. La regla —`blobs/ab/cd/<hash>`— vive
    //  en un sitio, y quien dibuja una miniatura no tiene por qué conocerla:
    //  duplicarla en QML es duplicarla mal el día que cambie.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob_path: Option<String>,
    //  Y la miniatura, si la tiene. Se calcula igual que la de arriba y por lo
    //  mismo: en la base vive el hash, no una ruta que caduca.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview_path: Option<String>,
    pub status: String,
    //  De qué sitio viene, del adaptador. La interfaz lo usa para enseñarlo
    //  como lo que es —un vídeo de YouTube, un hilo de Reddit— en vez de como
    //  «un enlace».
    pub source: String,
    //  Lo que la IA entendió, si se lo pediste. Aparte de la nota —esa es
    //  tuya— y del extracto —eso es el original—.
    pub summary: String,
    pub captured_at: i64,
    pub last_opened_at: Option<i64>,
    pub snoozed_until: Option<i64>,
    pub favorite: bool,
    pub tags: Vec<String>,
    //  El trozo donde encaja lo buscado, con la parte que encaja marcada. Solo
    //  en las búsquedas: en una lista no hay nada que resaltar.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
}

//  Lo escrito, convertido en una consulta que FTS5 entiende.
//
//  Cada palabra va entrecomillada —así deja de ser sintaxis y pasa a ser
//  texto— y con `*` detrás, que es el prefijo: escribir «biblio» encuentra
//  «biblioteca» sin esperar a que termines. Un trozo entre comillas en lo que
//  escribió el usuario se respeta como frase exacta y no se le pone prefijo.
pub fn consulta_fts(bruta: &str) -> Option<String> {
    let mut partes: Vec<String> = Vec::new();
    let mut resto = bruta.trim();
    while !resto.is_empty() {
        if let Some(sin) = resto.strip_prefix('"') {
            //  Una frase. Hasta la siguiente comilla, o hasta el final si se
            //  dejó abierta —que es lo normal mientras se escribe—.
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
        //  Nada de sintaxis: solo letras, números y lo que quede dentro de una
        //  palabra. El resto se tira, no se escapa, porque escapar mal en un
        //  lenguaje de consulta es cómo se cuelan las inyecciones.
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
        //  Sin operador entre ellas: en FTS5 eso es un AND implícito, que es lo
        //  que espera quien escribe dos palabras.
        Some(partes.join(" "))
    }
}

//  Las que no dicen nada por sí solas. Solo se usan en el repesque; ver
//  `consulta_fts_floja` para el porqué.
//
//  Sin acentos porque así llegan: el tokenizador del índice los quita, y estas
//  se comparan contra lo que ya salió de él.
const DE_RELLENO: &[&str] = &[
    //  Español: preguntar por algo guardado suena casi siempre igual.
    "algo", "algun", "alguna", "alguno", "algunos", "algunas", "como", "cosa",
    "cosas", "cual", "cuales", "cuando", "donde", "esta", "este", "esto",
    "estos", "estas", "guarde", "guardado", "guardada", "hace", "hacer", "para",
    "pero", "porque", "pues", "sobre", "tenia", "tengo", "tiene", "todo",
    "todos", "todas", "unos", "unas", "acerca", "busca", "buscar", "encuentra",
    "quiero", "puedes", "podrias", "dime", "sabes", "recuerda", "recuerdas",
    //  Inglés: la mitad de lo guardado está en inglés y a veces se pregunta así.
    "about", "anything", "does", "have", "something", "that", "the", "there",
    "these", "this", "those", "what", "when", "where", "which", "with", "your",
    "find", "search", "show", "tell", "remember", "some", "from", "into",
];

//  La misma consulta, pero valiendo con CUALQUIERA de las palabras.
//
//  El AND implícito es lo correcto mientras encuentre algo, y es lo peor que
//  puede pasar cuando no: con cuatro palabras basta que una no esté para que la
//  respuesta sea «no hay nada guardado que se parezca a eso», que es mentira.
//  Y lo dice justo cuando más material hay, porque cuanto más grande es la
//  biblioteca más larga es la pregunta que le haces.
pub fn consulta_fts_floja(bruta: &str) -> Option<String> {
    let q = consulta_fts(bruta)?;
    //  Una sola palabra ya se buscó tal cual; repetirla no encuentra nada nuevo.
    if !q.contains(' ') {
        return None;
    }
    //  Y fuera las palabras que no dicen nada.
    //
    //  En un OR son veneno: cada una arrastra por su cuenta, así que «de» —que
    //  está en todo lo que has guardado— devolvía la biblioteca entera. Con AND
    //  daban igual, porque tenían que estar todas.
    //
    //  Se van por dos motivos. Las CORTAS, de tres letras o menos, porque a esa
    //  longitud casi nada arrastra significado; cuesta algo —«go» y «rs» son
    //  etiquetas de verdad—, y se pierden solo en el repesque.
    //
    //  Y las de RELLENO, que en español son largas: «tengo», «sobre», «algo»,
    //  «cuando». Sin ellas fuera, preguntar «¿tengo algo sobre fondos de
    //  pantalla?» devolvía media biblioteca por «tengo» y por «algo». La lista
    //  es corta y de los dos idiomas que habla la casa, y vive AQUÍ y no en el
    //  AND porque ahí no estorban: si alguien escribe «sobre» entre comillas,
    //  la busca de verdad.
    let largas: Vec<&str> = q
        .split(' ')
        .filter(|t| {
            //  Plegada —sin tildes y en minúsculas— antes de comparar: la
            //  lista está escrita sin ellas porque así sale del índice, y sin
            //  esto «tenía» no es «tenia» y se cuela igual.
            let limpia = crate::util::plegar(
                &t.chars().filter(|c| c.is_alphanumeric()).collect::<String>(),
            );
            limpia.chars().count() > 3 && !DE_RELLENO.contains(&limpia.as_str())
        })
        .collect();
    //  Si no queda ninguna, se repescan todas: más vale de más que nada.
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

//  La misma consulta con las palabras enteras: sin el `*` del prefijo.
pub fn consulta_fts_exacta(bruta: &str) -> Option<String> {
    Some(consulta_fts(bruta)?.replace("\"*", "\""))
}

pub fn buscar(db: &Connection, texto: &str, limite: usize) -> rusqlite::Result<Vec<Fila>> {
    let Some(q) = consulta_fts(texto) else {
        return Ok(Vec::new());
    };
    //  Las palabras enteras primero, y detrás lo que solo empieza igual.
    //
    //  El prefijo es para lo que todavía se está escribiendo, y con él «pan»
    //  valía lo mismo en «pan casero» que en «captura-pantalla.png», que salía
    //  antes por tener el título más corto. Mientras la última palabra está a
    //  medias la exacta no encuentra nada y todo viene del prefijo, así que
    //  escribir sigue encontrando letra a letra.
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
    //  Con todas las palabras primero. Si no sale nada, con cualquiera de
    //  ellas: más vale lo que se parece a la mitad de lo que preguntaste que la
    //  pantalla vacía, y `bm25` ya pone arriba lo que encaja en más sitios.
    if filas.is_empty() {
        if let Some(floja) = consulta_fts_floja(texto) {
            filas = buscar_con(db, &floja, limite)?;
        }
    }

    //  Y por último, lo que se PARECE.
    //
    //  Después y no mezclado: lo que contiene tus palabras es lo que pediste, y
    //  va primero siempre. Lo que se parece se añade detrás para llenar, y solo
    //  si hay modelo. Fundir los dos órdenes en una puntuación común suena
    //  mejor y en la práctica es una manera de que lo exacto acabe el tercero.
    if filas.len() < limite {
        let ya: std::collections::HashSet<String> =
            filas.iter().map(|f| f.id.clone()).collect();
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
    //  Uno por columna, contando `capture_id`, que no se indexa pero es la
    //  primera: sin su 0.0 todos iban corridos uno, el título pesaba 4 y el
    //  cuerpo 6, y una palabra de pasada en un artículo le ganaba al título.
    //  Y el sitio, detrás de todo, a 3.
    //
    //  Los pesos de `bm25` no son un adorno. Por defecto todas las columnas
    //  valen igual, y entonces una palabra perdida en medio de un artículo de
    //  diez mil palabras compite con la misma palabra en el título. El título
    //  manda, las etiquetas casi tanto —las pusiste tú a mano—, y el cuerpo es
    //  el que menos.
    //
    //  El resumen pesa casi como una etiqueta, y por lo mismo: son cuarenta
    //  palabras elegidas para decir de qué va esto, así que una coincidencia
    //  ahí vale mucho más que una en medio del artículo.
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
    //  Aquí entra el hash de la miniatura y sale su ruta. El campo es el mismo
    //  porque quien la dibuja no tiene nada que hacer con un hash.
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

//  Lo que está a medias.
//
//  Enriquecer se hace al guardar, y eso deja fuera dos cosas: lo que guardaste
//  antes de que esto existiera, y aquello para lo que no había red en ese
//  momento. Sin una lista de pendientes, una biblioteca de hace un mes se queda
//  para siempre con direcciones crudas por título, que es justo el problema que
//  el enriquecimiento venía a resolver.
//
//  Está a medias si le falta la miniatura, si su título sigue siendo el de
//  apaño, si no se sabe de qué sitio viene, o si no tiene texto dentro. Las
//  cuatro, porque son cuatro maneras distintas de estar a medias: una imagen
//  puede tener miniatura y llamarse «ejemplo.com».
//
//  Devuelve la dirección a la que ir y/o el fichero que mirar, y nada más:
//  quien sale a la calle no necesita la captura entera.
#[derive(Debug, Serialize)]
pub struct PorEnriquecer {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    //  El fichero, ya montado. Igual que `blob_path`: en la base vive el hash y
    //  quien lo va a abrir no tiene nada que hacer con un hash.
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
        //  La canónica primero: es la que hay que visitar.
        let url = canonica
            .or(origen)
            .filter(|u| u.starts_with("http"));
        //  Y el fichero solo si le falta el texto: volver a abrir un PDF que ya
        //  se leyó es trabajo tirado, y son segundos por cada uno.
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

// ── el parecido por significado ──────────────────────────────────
//
//  FTS encuentra lo que contiene tus palabras. Esto encuentra lo que quiere
//  decir lo mismo, que es otra cosa y hace falta cuando la biblioteca crece:
//  con veinte capturas te acuerdas de cómo se llamaban; con dos mil, no.
//
//  Todo esto no existe si no hay modelo, y entonces la biblioteca busca
//  exactamente como antes. Es opcional a propósito: son 506 MB en el disco de
//  alguien.

//  Con qué texto se representa una captura, leído de la base.
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

//  Rehacer el vector de una captura. Se llama después de guardarla y de cada
//  vez que cambia lo que la describe: el título, el resumen, las etiquetas.
//  Con cuánto material merece la pena hacer un vector.
//
//  Es el número más importante de todo esto y salió de un fallo. Una captura de
//  Reddit —que no da texto a quien no ejecuta JavaScript— se quedó con seis
//  caracteres: su vector era el de la palabra «Reddit» y aterrizaba en una zona
//  genérica, así que se parecía un 0,26 a «repostería» y un 0,24 a «editar
//  vídeo». Salía en TODAS las búsquedas.
//
//  Un vector hecho de una palabra no resume nada; es ruido con forma de
//  resultado. Sin material no hay vector, y esa captura se sigue encontrando
//  por su nombre, que es lo único que de verdad se sabe de ella.
pub const MATERIAL_MINIMO: usize = 24;

pub fn revectorizar(db: &Connection, id: &str) -> rusqlite::Result<bool> {
    if !crate::vectores::listo() {
        return Ok(false);
    }
    let texto = texto_para_vector(db, id)?;
    if texto.trim().chars().count() < MATERIAL_MINIMO
        || texto.split_whitespace().count() < 4
    {
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

//  Las que aún no tienen vector, o lo tienen de otra medida —o sea de otro
//  modelo—. Se rehacen de N en N para que instalar el modelo con dos mil
//  capturas dentro no bloquee la biblioteca cinco minutos.
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
        //  Y con material suficiente, aquí también: si no, las que no dan para
        //  un vector se eligen en cada barrido, para siempre, sin que ninguna
        //  llegue a tenerlo.
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

//  Lo que más se parece a lo que has escrito.
//
//  Se comparan todos los vectores a mano y sin índice, y es a propósito: una
//  biblioteca personal son miles de capturas, no millones, y multiplicar dos
//  mil vectores de 256 números tarda menos que abrir un índice. El día que esto
//  no baste, la tabla ya está separada para meterle uno.
//  Por debajo de esto no se parece: se parece un poco a todo, que es como los
//  vectores dicen «no sé». Sin umbral, buscar cualquier cosa devuelve la
//  biblioteca entera ordenada por casualidad.
//
//  El número sale de mirar los de verdad, no de un artículo: con este modelo
//  —embeddings estáticos, sin transformador— los cosenos son más bajos que con
//  los grandes, y el 0,35 que traía copiado dejaba fuera aciertos claros.
pub const PARECIDO_MINIMO: f32 = 0.22;

pub fn parecidas(db: &Connection, texto: &str, limite: usize) -> rusqlite::Result<Vec<(String, f32)>> {
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

//  Lo que todavía no ha resumido nadie.
//
//  Devuelve lo justo para que un modelo pueda decir de qué va: el título, de
//  dónde viene y un trozo del principio. **No** el contenido entero, y esa es
//  una decisión de privacidad, no de tamaño: lo que sale del equipo tiene que
//  ser lo mínimo para hacer el trabajo, y para resumir un enlace bastan su
//  título y su descripción.
#[derive(Debug, Serialize)]
pub struct PorClasificar {
    pub id: String,
    pub title: String,
    pub source: String,
    #[serde(rename = "type")]
    pub tipo: String,
    pub excerpt: String,
    //  Y el fichero, si es una imagen y hay que MIRARLA.
    //
    //  De una imagen no hay nada que resumir a partir de su ficha: el título es
    //  el nombre del fichero y el extracto está vacío. O se mira o no se sabe
    //  qué es, y por eso viaja la ruta.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_path: Option<String>,
}

pub fn por_clasificar(db: &Connection, limite: i64) -> rusqlite::Result<Vec<PorClasificar>> {
    let mut s = db.prepare(
        //  Lo que no se ha mirado todavía, no lo que no tiene resumen: son
        //  cosas distintas. De una página que no da texto no se puede decir
        //  nada, y sin apuntar el intento se pregunta otra vez cada veinte
        //  segundos, para siempre y pagando.
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
                //  Recortado aquí y no en quien llama: el tope es de lo que
                //  sale del equipo, y eso no lo decide el que hace la llamada.
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

//  La lista de siempre: lo último guardado, sin lo tirado. Es lo que enseña la
//  página cuando no has escrito nada en la caja.
//
//  El filtro es de la navegación: un espacio, una etiqueta o «hoy». Se pasa
//  entero y se arma aquí en vez de dejar que quien llama componga SQL: es la
//  diferencia entre un filtro y una inyección.
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
        donde.push_str(
            " AND id IN (SELECT capture_id FROM capture_spaces WHERE space_id = :space)",
        );
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

//  Medianoche de hoy, en la zona del equipo. `date(...,'localtime')` la calcula
//  SQLite; hacerlo en Rust exigiría una caja de fechas entera para esto.
fn comienzo_del_dia() -> i64 {
    (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
        / 86400)
        * 86400
        * 1000
}

//  Lo que ha vuelto a la superficie: lo que estaba aplazado y ya toca.
//
//  Nada más, y es a propósito. Las otras razones para reaparecer —hace mucho
//  que no lo abres, esta aplicación va con ese proyecto— son de D6, y
//  rellenar esta banda con «lo último guardado» mientras tanto sería mentir con
//  el título puesto: «han vuelto a la superficie» no significa «lo de ayer».
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

//  La navegación de la izquierda, con sus números. Sale de la base y no de una
//  lista escrita en QML: los espacios los crea el usuario, y una barra lateral
//  fija dejaría de contar el día que cree el cuarto.
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
            count: uno("SELECT COUNT(*) FROM captures WHERE trashed_at IS NULL", &[])?,
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
                    if c.is_empty() { None } else { Some(c) }
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
    //  Cuántas siguen sin resumen. Es lo único de las cuentas que no describe
    //  la biblioteca sino el TRABAJO que queda, y está aquí porque la página ya
    //  pide las cuentas en cada refresco: un número más no cuesta una llamada
    //  más, y sin él la única manera de saber si esto va era mirar la base de
    //  datos a mano.
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
        pending_summary: uno(
            "SELECT COUNT(*) FROM captures
              WHERE trashed_at IS NULL AND summary = '' AND summary_at IS NULL",
        )?,
        tags: uno("SELECT COUNT(*) FROM tags")?,
        spaces: uno("SELECT COUNT(*) FROM spaces")?,
        blobs: uno("SELECT COUNT(DISTINCT blob_hash) FROM captures WHERE blob_hash IS NOT NULL")?,
        blob_bytes: uno(
            "SELECT COALESCE(SUM(blob_bytes), 0) FROM
               (SELECT DISTINCT blob_hash, blob_bytes FROM captures WHERE blob_hash IS NOT NULL)",
        )?,
    })
}
