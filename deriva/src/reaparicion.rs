//  Qué te devuelve, y por qué esa y no otra.
//
//  El motor es LOCAL Y DETERMINISTA, que es lo que pide el plan y no es un
//  detalle de implementación: significa que la razón por la que algo ha vuelto
//  se puede explicar con una frase, y que la misma biblioteca en el mismo
//  momento devuelve lo mismo. Un modelo eligiendo qué recordarte sería más
//  listo y completamente inauditable.
//
//  Y no infiere nada de ti. Se apunta lo que pasó con cada aparición —abierta,
//  aplazada, descartada— y sirve para NO INSISTIR. El plan lo dice con todas
//  las letras: «no se usa para inferir el estado emocional del usuario».

use rusqlite::{params, Connection};
use serde::Serialize;

//  Cuánto hace que la abriste para que deje de contar como reciente.
const RECIENTE_DIAS: i64 = 7;
//  Cuántas veces se le puede enseñar algo sin que haga nada antes de callarse.
const INSISTIR_MAX: i64 = 3;
//  Qué es «favorito olvidado» y «lectura pendiente corta».
const OLVIDADO_DIAS: i64 = 30;
const LECTURA_CORTA: i64 = 2000; //  caracteres

#[derive(Debug, Serialize)]
pub struct Devuelta {
    pub item: crate::search::Fila,
    //  Por qué esta. Es una enumeración cerrada y sale tal cual a la interfaz:
    //  si no se puede nombrar la razón, la captura no es elegible.
    pub reason: String,
    //  Y en palabras, para poder enseñarlo sin traducir enumeraciones en QML.
    pub why: String,
}

//  La regla de elegibilidad del plan, entera y en un sitio.
//
//  Va como texto SQL y no como filtro en Rust a propósito: son cinco
//  condiciones sobre la misma tabla, y hacerlas en memoria significaría leer la
//  biblioteca entera para descartar el 99 %.
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

//  Y la prioridad, también del plan:
//
//      fecha explícita > proyecto o aplicación asociada > favorito olvidado
//        > lectura pendiente corta > elemento antiguo al azar con diversidad
//
//  Se resuelve en orden y se para en la primera que dé algo. Un `ORDER BY` con
//  todas mezcladas daría el mismo resultado y no se podría explicar: así, la
//  razón que se enseña es literalmente la consulta que acertó.
pub fn siguiente(
    db: &Connection,
    espacio_de_la_app: Option<&str>,
) -> rusqlite::Result<Option<Devuelta>> {
    let ahora = crate::util::ahora_ms();
    let hace_poco = ahora - RECIENTE_DIAS * 86_400_000;
    let olvidado = ahora - OLVIDADO_DIAS * 86_400_000;

    //  1. Fecha explícita: la aplazaste para hoy. Es lo único que pediste tú.
    if let Some(id) = uno(
        db,
        &format!(
            "SELECT c.id FROM captures c
              WHERE {} AND c.snoozed_until IS NOT NULL
              ORDER BY c.snoozed_until DESC LIMIT 1",
            elegibles()
        ),
        &[(":ahora", &ahora), (":hace_poco", &hace_poco), (":insistir", &INSISTIR_MAX)],
    )? {
        return armar(db, &id, "scheduled", "la aplazaste para hoy");
    }

    //  2. El espacio asociado a lo que tienes delante. La asociación es opt-in y
    //  llega desde fuera: aquí no se mira ni una ventana.
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
                return armar(db, &id, "app_context", "va con lo que tienes abierto");
            }
        }
    }

    //  3. Favorito olvidado: lo marcaste y llevas un mes sin mirarlo.
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
        return armar(db, &id, "forgotten_favorite", "lo marcaste y lleva un mes ahí");
    }

    //  4. Lectura pendiente corta. Corta a propósito: devolverte un ensayo de
    //  diez mil palabras mientras trabajas no es una ayuda, es una interrupción
    //  con buenas intenciones.
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
        return armar(db, &id, "short_read", "es corto y lo dejaste para luego");
    }

    //  5. Algo antiguo, con diversidad.
    //
    //  «Al azar» de verdad repetiría del mismo sitio: si tienes cuatrocientas
    //  capturas de arquitectura y doce de cocina, el azar puro te enseña
    //  arquitectura casi siempre. Se elige primero el espacio menos visto
    //  últimamente y de ahí lo más viejo, que es lo que hace que la biblioteca
    //  entera vaya rotando en vez de un rincón de ella.
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
        &[(":ahora", &ahora), (":hace_poco", &hace_poco), (":insistir", &INSISTIR_MAX)],
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

//  Se apunta que se enseñó, SIN resultado todavía. Es lo que permite dejar de
//  insistir con algo que enseñas y nadie toca.
pub fn apuntar_mostrada(db: &Connection, id: &str, razon: &str) -> rusqlite::Result<i64> {
    db.execute(
        "INSERT INTO revisit_events (capture_id, reason, shown_at) VALUES (?1, ?2, ?3)",
        params![id, razon, crate::util::ahora_ms()],
    )?;
    Ok(db.last_insert_rowid())
}

//  Y qué hiciste con ella. Cerrado: `opened`, `snoozed`, `dismissed`,
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

//  Cuántas seguidas no han servido de nada —ignoradas o descartadas—, para
//  callarse hasta mañana. La cuenta la
//  lleva el worker porque es quien tiene el historial; el silencio lo aplica
//  quien enseña, que es quien sabe qué hora es para el usuario.
pub fn descartes_seguidos(db: &Connection) -> rusqlite::Result<i64> {
    let mut s = db.prepare(
        //  Por `id` además de por la hora: dos apariciones del mismo
        //  milisegundo se ordenaban al azar, y entonces «seguidos» dejaba de
        //  significar nada. El `id` es un contador y siempre sube.
        //
        //  Y SIN `outcome IS NOT NULL`, que era lo que rompía el freno entero.
        //  Filtrando por resultado solo se veían las que el usuario había
        //  descartado A MANO, y descartar a mano es ir a la página, elegir la
        //  tarjeta y pulsar un botón de 26 píxeles. Quien no quiere algo no
        //  hace ese viaje: lo ignora. Así que la cuenta se quedaba en cero
        //  para siempre —quince apariciones seguidas, ni una con resultado— y
        //  el silencio prometido no llegó a activarse nunca.
        //
        //  Ignorar es la respuesta más común y ahora cuenta como lo que es.
        "SELECT outcome FROM revisit_events
          WHERE shown_at >= ?1 ORDER BY shown_at DESC, id DESC LIMIT 8",
    )?;
    let ultimos: Vec<Option<String>> = s
        .query_map([inicio_de_hoy()], |r| r.get::<_, Option<String>>(0))?
        .filter_map(|x| x.ok())
        .collect();
    let mut n = 0;
    for o in ultimos {
        //  Sin resultado es «se la enseñé y no hizo nada»; `dismissed` es
        //  «ya la he visto». Las dos dicen que no hace falta insistir. Abrir
        //  o aplazar corta la racha, porque las dos son que sí servía.
        match o.as_deref() {
            None | Some("dismissed") => n += 1,
            _ => break,
        }
    }
    Ok(n)
}

//  Y desde cuándo cuenta «hoy». En un sitio porque lo miran dos: el tope
//  diario y el silencio, que el plan promete «hasta el día siguiente» y sin
//  esta línea era hasta el fin de los tiempos.
fn inicio_de_hoy() -> i64 {
    (crate::util::ahora_ms() / 86_400_000) * 86_400_000
}

//  Cuántas se han enseñado hoy, para el tope diario.
pub fn mostradas_hoy(db: &Connection) -> rusqlite::Result<i64> {
    db.query_row(
        "SELECT COUNT(*) FROM revisit_events WHERE shown_at >= ?1",
        [inicio_de_hoy()],
        |r| r.get(0),
    )
}

//  Cuándo fue la última, para el hueco mínimo entre apariciones.
//
//  Lo pregunta quien enseña, que tenía el dato en una propiedad de QML y lo
//  perdía en cada recarga. Aquí está en disco desde el principio: la tabla es
//  la misma que lleva el historial, y no hacía falta más que devolverlo.
pub fn ultima_aparicion(db: &Connection) -> rusqlite::Result<i64> {
    db.query_row(
        "SELECT COALESCE(MAX(shown_at), 0) FROM revisit_events",
        [],
        |r| r.get(0),
    )
}
