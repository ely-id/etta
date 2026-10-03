//  Buscar por lo que quiere decir, y no solo por las palabras que trae.
//
//  FTS5 encuentra lo que contiene tus palabras. Es exacto, es rápido y no sabe
//  que «fondos de pantalla» y «wallpapers» son lo mismo, ni que un editor de
//  vídeo tiene que ver con «montar un clip». Esto es lo otro: cada captura se
//  convierte en un vector, la pregunta también, y se comparan por ángulo.
//
//  Cuatro decisiones, y las cuatro son de las que se pagan más tarde:
//
//  EL MODELO ES LOCAL Y EL WORKER SIGUE SIN RED. El crate se compila con
//  `local-only` a propósito: sin esa bandera se trae la capacidad de descargar
//  modelos, y eso convertiría en mentira la frase que hay escrita en el README.
//  Quien baja el modelo es la casa, con `./marea deriva modelo`.
//
//  Y SI NO ESTÁ, NO PASA NADA. Sin modelo se busca exactamente como antes.
//  Media gracia de esto es que sea opcional: son 506 MB en el disco de alguien,
//  y una biblioteca que deja de funcionar porque falta un fichero de medio giga
//  no es una biblioteca, es una promesa.
//
//  SE CARGA UNA VEZ Y SE QUEDA. Leer medio giga de safetensors en cada búsqueda
//  sería más lento que no tener búsqueda.
//
//  Y SE GUARDA CRUDO, en `f32` little-endian. No es por ahorrar: es que un
//  vector es una lista de números y meterlo en JSON lo hace cuatro veces más
//  grande para que luego haya que parsearlo entero en cada comparación.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use model2vec_rs::model::StaticModel;

//  Dónde vive el modelo. Un directorio con los tres ficheros que pide el
//  formato: los pesos, el tokenizador y su configuración.
pub fn modelo_dir() -> PathBuf {
    crate::paths::base().join("modelo")
}

pub fn hay_modelo() -> bool {
    let d = modelo_dir();
    ["model.safetensors", "tokenizer.json", "config.json"]
        .iter()
        .all(|f| d.join(f).is_file())
}

//  Cargado una vez, y con la posibilidad de no haberlo. `OnceLock` guarda
//  también el «no hay»: si falta, no se vuelve a intentar en cada búsqueda.
static MODELO: OnceLock<Option<Mutex<StaticModel>>> = OnceLock::new();

fn modelo() -> Option<&'static Mutex<StaticModel>> {
    MODELO
        .get_or_init(|| {
            if !hay_modelo() {
                return None;
            }
            //  Normalizado: así comparar dos vectores es multiplicarlos, y no
            //  hay que dividir por sus longitudes en cada comparación.
            match StaticModel::from_pretrained(modelo_dir(), None, Some(true), None) {
                Ok(m) => Some(Mutex::new(m)),
                Err(e) => {
                    eprintln!("deriva-worker: el modelo no se puede leer: {}", e);
                    None
                }
            }
        })
        .as_ref()
}

pub fn listo() -> bool {
    modelo().is_some()
}

//  Cuántos números tiene cada vector. Se guarda con cada uno porque un modelo
//  distinto da otra medida, y comparar vectores de medidas distintas no da un
//  error: da un parecido inventado.
pub fn medida() -> usize {
    vector("hola").map(|v| v.len()).unwrap_or(0)
}

pub fn vector(texto: &str) -> Option<Vec<f32>> {
    varios(&[texto.to_string()]).and_then(|mut v| v.pop())
}

pub fn varios(textos: &[String]) -> Option<Vec<Vec<f32>>> {
    let m = modelo()?;
    let g = m.lock().ok()?;
    //  Tope de tokens por texto: un artículo entero diluye lo que dice hasta
    //  que el vector deja de parecerse a nada. Lo que se resume en un vector es
    //  DE QUÉ VA, y para eso el principio vale más que el final.
    Some(g.encode_with_args(textos, Some(512), 32))
}

//  Con qué se representa una captura.
//
//  El título, el resumen, las etiquetas y el principio del texto, en ese orden
//  y recortado. NO el contenido entero: un vector es una media, y la media de
//  cuarenta páginas se parece un poco a todo y mucho a nada.
pub fn texto_de(
    titulo: &str,
    resumen: &str,
    etiquetas: &str,
    extracto: &str,
    cuerpo: &str,
) -> String {
    let mut t = String::new();
    for trozo in [titulo, resumen, etiquetas, extracto] {
        let limpio = trozo.trim();
        if limpio.is_empty() {
            continue;
        }
        if !t.is_empty() {
            t.push('\n');
        }
        t.push_str(limpio);
    }
    //  Y del cuerpo, solo lo que falte para llenar el hueco: si ya hay título y
    //  resumen, el cuerpo aporta poco y estorba.
    if t.chars().count() < 400 {
        let falta = 1200 - t.chars().count().min(1200);
        let principio: String = cuerpo.trim().chars().take(falta).collect();
        if !principio.is_empty() {
            if !t.is_empty() {
                t.push('\n');
            }
            t.push_str(&principio);
        }
    }
    t.chars().take(1200).collect()
}

// ── guardarlos y compararlos ─────────────────────────────────────

pub fn a_bytes(v: &[f32]) -> Vec<u8> {
    let mut b = Vec::with_capacity(v.len() * 4);
    for x in v {
        b.extend_from_slice(&x.to_le_bytes());
    }
    b
}

pub fn de_bytes(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

//  El coseno de dos vectores YA NORMALIZADOS, que es su producto. Se comprueba
//  la medida antes: dos vectores de tamaños distintos son de modelos distintos,
//  y compararlos hasta donde alcancen daría un número que parece un parecido.
pub fn parecido(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return -1.0;
    }
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}
