//  The small stuff: identifiers, time, hashes, and base64.

use sha2::{Digest, Sha256};

pub fn ahora_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

//  An identifier with the time first.
//
//  It could be entirely random, but then two consecutive captures would land
//  far apart in the index and fragment the primary key. With the time first,
//  things saved together are written together, and IDs can also be sorted by
//  age without looking up their rows.
pub fn nuevo_id() -> String {
    let mut b = [0u8; 8];
    azar(&mut b);
    format!("{:011x}{}", ahora_ms(), hex(&b))
}

pub fn azar(dest: &mut [u8]) {
    use std::io::Read;
    //  `/dev/urandom`, not a custom generator seeded with the time: two
    //  processes started in the same millisecond would produce the same
    //  sequence, which here would mean two captures with the same ID.
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        if f.read_exact(dest).is_ok() {
            return;
        }
    }
    //  If even that fails, something is very wrong; at least avoid zeros.
    let t = ahora_ms() as u64;
    for (i, d) in dest.iter_mut().enumerate() {
        *d = ((t >> (i % 8 * 8)) as u8) ^ (i as u8).wrapping_mul(31);
    }
}

pub fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

pub fn sha256(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

//  Hash a file without loading it all into memory: nobody has enough RAM for a
//  two-gigabyte video just to check whether it is already present.
pub fn sha256_de_fichero(p: &std::path::Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(p)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex(&h.finalize()))
}

//  Hash the CONTENT, not the bytes.
//
//  The same paragraph copied twice can have different whitespace —an extra
//  newline, a tab where there were spaces—, and if that counts as different
//  text, duplicate detection will never work. Normalize it first: collapse
//  whitespace and trim the ends.
pub fn hash_de_texto(t: &str) -> String {
    let mut limpio = String::with_capacity(t.len());
    let mut espacio = false;
    for c in t.chars() {
        if c.is_whitespace() {
            espacio = true;
        } else {
            if espacio && !limpio.is_empty() {
                limpio.push(' ');
            }
            espacio = false;
            limpio.push(c);
        }
    }
    sha256(limpio.as_bytes())
}

// ── base64 ───────────────────────────────────────────────────────
//
//  We only need to decode, and only the standard alphabet. Bringing in a whole
//  crate for thirty lines in a binary meant to stay small is more than it is
//  worth; there is a round-trip test alongside it.

pub fn base64(entrada: &str) -> Option<Vec<u8>> {
    let mut fuera = Vec::with_capacity(entrada.len() / 4 * 3);
    let mut acc: u32 = 0;
    let mut bits: u32 = 0;
    for c in entrada.bytes() {
        //  Ignore spaces and newlines: multi-megabyte base64 is often split
        //  across lines.
        if c == b'\n' || c == b'\r' || c == b' ' || c == b'\t' {
            continue;
        }
        if c == b'=' {
            break;
        }
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            fuera.push(((acc >> bits) & 0xff) as u8);
        }
    }
    Some(fuera)
}

//  Encoding is only needed in tests —the worker receives base64, it does not
//  produce it—, but an encoder without a decoder cannot be round-trip tested.
#[allow(dead_code)]
pub fn a_base64(bytes: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for trozo in bytes.chunks(3) {
        let b = [
            trozo[0],
            *trozo.get(1).unwrap_or(&0),
            *trozo.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        s.push(A[(n >> 18) as usize & 63] as char);
        s.push(A[(n >> 12) as usize & 63] as char);
        s.push(if trozo.len() > 1 {
            A[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        s.push(if trozo.len() > 2 {
            A[n as usize & 63] as char
        } else {
            '='
        });
    }
    s
}

//  Where a link comes from, expressed as searchable words: the hostname without
//  `www.` or `m.` —the tokenizer already splits it into “youtube” and “com”—
//  and the familiar shortcuts everyone uses, because “youtu.be” is YouTube
//  even if the name does not say so. A file, or something that is not a web
//  address, does not come from a site.
pub fn sitio_buscable(direccion: &str) -> String {
    let t = direccion.trim();
    let resto = match t
        .strip_prefix("https://")
        .or_else(|| t.strip_prefix("http://"))
    {
        Some(r) => r,
        None => return String::new(),
    };
    let autoridad = resto.split(['/', '?', '#']).next().unwrap_or("");
    let maquina = autoridad.rsplit('@').next().unwrap_or("");
    let maquina = maquina.split(':').next().unwrap_or("").to_lowercase();
    let maquina = maquina
        .strip_prefix("www.")
        .or_else(|| maquina.strip_prefix("m."))
        .unwrap_or(&maquina)
        .to_string();
    if maquina.is_empty() {
        return String::new();
    }
    let tambien = match maquina.as_str() {
        "youtu.be" => " youtube",
        "x.com" | "t.co" => " twitter",
        "redd.it" => " reddit",
        _ => "",
    };
    format!("{}{}", maquina, tambien)
}

// ── the URL, in comparable form ───────────────────────────────────
//
//  Links to the same page can be written in ten different ways: with `www`,
//  with a section `#`, or with the campaign suffix added by an email. Without
//  normalization, “you already have this” never works and the library fills up
//  with the same page six times.
//
//  What we do NOT change: the order of the remaining parameters or the
//  capitalization of the path. Video IDs are case-sensitive, and “normalizing”
//  that would turn two videos into one.
pub fn canonizar_url(bruta: &str) -> Option<String> {
    let t = bruta.trim();
    let (esquema, resto) = if let Some(r) = t.strip_prefix("https://") {
        ("https", r)
    } else if let Some(r) = t.strip_prefix("http://") {
        ("http", r)
    } else {
        return None;
    };
    //  Remove the fragment: `#section` says where you were looking, not
    //  which page it is.
    let resto = resto.split('#').next().unwrap_or("");
    let (autoridad, camino) = match resto.find('/') {
        Some(i) => (&resto[..i], &resto[i..]),
        None => (resto, "/"),
    };
    if autoridad.is_empty() {
        return None;
    }
    let mut host = autoridad.to_lowercase();
    //  The default port is unnecessary: `example.com:443` and
    //  `example.com` are the same host.
    for (e, p) in [("https", ":443"), ("http", ":80")] {
        if esquema == e {
            if let Some(h) = host.strip_suffix(p) {
                host = h.to_string();
            }
        }
    }
    if let Some(h) = host.strip_prefix("www.") {
        host = h.to_string();
    }
    let (ruta, consulta) = match camino.find('?') {
        Some(i) => (&camino[..i], &camino[i + 1..]),
        None => (camino, ""),
    };
    let ruta = if ruta.is_empty() { "/" } else { ruta };
    //  Also remove anything that only tracks where you came from.
    let mut params: Vec<&str> = Vec::new();
    for p in consulta.split('&') {
        if p.is_empty() {
            continue;
        }
        let clave = p.split('=').next().unwrap_or(p).to_lowercase();
        if clave.starts_with("utm_")
            || matches!(
                clave.as_str(),
                "fbclid"
                    | "gclid"
                    | "gbraid"
                    | "wbraid"
                    | "msclkid"
                    | "mc_eid"
                    | "mc_cid"
                    | "igshid"
                    | "ref_src"
                    | "ref_url"
                    | "s"
                    | "si"
                    | "spm"
                    | "yclid"
            )
        {
            continue;
        }
        params.push(p);
    }
    let mut fuera = format!("{}://{}{}", esquema, host, ruta);
    //  A trailing slash on an empty path does not matter, but `/blog/` can
    //  matter on some servers, so remove only the root slash.
    if fuera.ends_with("//") {
        fuera.pop();
    }
    if !params.is_empty() {
        fuera.push('?');
        fuera.push_str(&params.join("&"));
    }
    Some(fuera)
}

//  Without accents and in lowercase.
//
//  Used to compare words against manually written lists. The index already does
//  this itself —`remove_diacritics 2` in the tokenizer—, so a list that does
//  not will never match its output: “tenía” is not “tenia,” and the word still
//  slips through.
pub fn plegar(t: &str) -> String {
    t.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            otro => otro,
        })
        .collect()
}
