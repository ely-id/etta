//  Search by what it means, not just by the words it contains.
//
//  FTS5 finds what contains your words. It is exact and fast, but does not know
//  that “wallpapers” and “wallpaper backgrounds” are the same, or that a video
//  editor has anything to do with “putting a clip together.” This is the other
//  approach: each capture becomes a vector, as does the query, and they are
//  compared by angle.
//
//  Four decisions, each with consequences that show up later:
//
//  THE MODEL IS LOCAL AND THE WORKER STILL HAS NO NETWORK. The crate is
//  deliberately compiled with `local-only`: without that flag it gains the
//  ability to download models, making the statement in the README untrue. The
//  host downloads the model with `./marea deriva modelo`.
//
//  AND IF IT ISN'T THERE, NOTHING BREAKS. Without a model, search works exactly
//  as before. Making it optional is half the point: it takes 506 MB on someone's
//  disk, and a library that stops working because a half-gigabyte file is
//  missing is not a library, just a promise.
//
//  IT LOADS ONCE AND STAYS LOADED. Reading half a gigabyte of safetensors on
//  every search would be slower than having no search at all.
//
//  AND IT IS STORED RAW, as little-endian `f32`. Not to save space: a vector is
//  a list of numbers, and putting it in JSON makes it four times larger, only to
//  parse the whole thing on every comparison.

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use model2vec_rs::model::StaticModel;

//  Where the model lives. A directory with the three files required by the
//  format: the weights, tokenizer, and its configuration.
pub fn modelo_dir() -> PathBuf {
    crate::paths::base().join("modelo")
}

pub fn hay_modelo() -> bool {
    let d = modelo_dir();
    ["model.safetensors", "tokenizer.json", "config.json"]
        .iter()
        .all(|f| d.join(f).is_file())
}

//  Loaded once, with the option that it may be absent. `OnceLock` also stores
//  “not found”: if it is missing, we do not retry on every search.
static MODELO: OnceLock<Option<Mutex<StaticModel>>> = OnceLock::new();

fn modelo() -> Option<&'static Mutex<StaticModel>> {
    MODELO
        .get_or_init(|| {
            if !hay_modelo() {
                return None;
            }
            //  Normalized: comparing two vectors is now just multiplying them,
            //  with no need to divide by their lengths on every comparison.
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

//  How many numbers each vector contains. Stored with each one because a
//  different model gives a different dimension, and comparing vectors with
//  different dimensions does not produce an error; it produces a made-up
//  similarity.
pub fn medida() -> usize {
    vector("hola").map(|v| v.len()).unwrap_or(0)
}

pub fn vector(texto: &str) -> Option<Vec<f32>> {
    varios(&[texto.to_string()]).and_then(|mut v| v.pop())
}

pub fn varios(textos: &[String]) -> Option<Vec<Vec<f32>>> {
    let m = modelo()?;
    let g = m.lock().ok()?;
    //  Token limit per text: a whole article dilutes its meaning until the
    //  vector stops resembling anything. A vector summarizes WHAT IT IS ABOUT,
    //  and the beginning matters more than the end for that.
    Some(g.encode_with_args(textos, Some(512), 32))
}

//  What represents a capture.
//
//  The title, summary, tags, and beginning of the text, in that order and
//  truncated. NOT the entire content: a vector is an average, and the average
//  of forty pages resembles a little bit of everything and a lot of nothing.
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
    //  From the body, only what is needed to fill the remaining space: if there
    //  is already a title and summary, the body adds little and gets in the way.
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

//  The cosine of two ALREADY NORMALIZED vectors, which is their dot product.
//  Check the dimension first: vectors of different sizes come from different
//  models, and comparing them up to the shorter length would produce a number
//  that looks like a similarity.
pub fn parecido(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return -1.0;
    }
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}
