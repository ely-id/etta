//  De qué sitio viene lo que has guardado.
//
//  Un enlace de YouTube y un enlace a un PDF no son lo mismo aunque los dos
//  sean `text/uri-list`: uno es un vídeo con un identificador y una miniatura, y
//  el otro es un documento. Enseñar los dos como «un enlace» es tratar la
//  biblioteca como una lista de marcadores.
//
//  Esto NO sale a la red. Es todo lo que se puede saber mirando la URL, y es
//  bastante: de dónde viene, qué es, y cuál es su forma canónica. Lo que hace
//  falta traer de fuera —el título de verdad, la miniatura— lo trae el lado de
//  la casa, que es quien tiene la armadura anti-SSRF ya escrita y revisada. El
//  worker no tiene red y no la va a tener.
//
//  La lista de fuentes es cerrada, como las demás: sale a la interfaz eligiendo
//  un color y un nombre, y una lista abierta significa que cualquier URL puede
//  inventarse una fuente que nadie dibuja.

#[derive(Debug, Clone, PartialEq)]
pub struct Fuente {
    //  El identificador, de la lista cerrada.
    pub id: &'static str,
    //  Cómo se llama para una persona.
    pub nombre: &'static str,
    //  Qué clase de cosa es lo que hay ahí. Manda sobre la extensión: un enlace
    //  de YouTube es un vídeo aunque la URL no acabe en `.mp4`.
    pub tipo: &'static str,
}

pub const GENERICA: Fuente = Fuente { id: "web", nombre: "Web", tipo: "url" };

//  Lo que se saca de una URL sin pedirle nada a nadie.
#[derive(Debug)]
pub struct Leido {
    pub fuente: Fuente,
    //  La forma canónica de ESE sitio, que no es la genérica.
    //
    //  `youtu.be/ID`, `youtube.com/watch?v=ID` y `youtube.com/watch?v=ID&t=90`
    //  son el mismo vídeo. Sin esto, arrastrar el mismo vídeo desde el móvil y
    //  desde el escritorio guarda dos capturas, y el duplicado —que es media
    //  gracia de esto— no lo detecta.
    pub canonica: Option<String>,
    //  Un título de emergencia, hasta que llegue el de verdad —el de la
    //  portada de la página, que llega por otro camino y puede no llegar—.
    pub titulo: Option<String>,
}

pub fn leer(url: &str) -> Leido {
    let (host, ruta, consulta) = partes(url);
    let h = host.trim_start_matches("www.");

    //  ── YouTube ──────────────────────────────────────────────────
    if h == "youtube.com" || h == "m.youtube.com" || h == "music.youtube.com"
        || h == "youtu.be"
    {
        let id = if h == "youtu.be" {
            primer_segmento(&ruta)
        } else if ruta.starts_with("/shorts/") || ruta.starts_with("/embed/")
            || ruta.starts_with("/live/")
        {
            ruta.splitn(3, '/').nth(2).map(|s| s.to_string())
        } else {
            parametro(&consulta, "v")
        };
        //  Once caracteres, que es lo que mide un identificador de vídeo. Sin
        //  comprobarlo, una URL de canal daría una miniatura que no existe.
        let id = id.filter(|s| s.len() == 11 && s.chars().all(seguro_en_id));
        return Leido {
            fuente: Fuente { id: "youtube", nombre: "YouTube", tipo: "video" },
            canonica: id
                .as_ref()
                .map(|v| format!("https://youtube.com/watch?v={}", v)),
            titulo: None,
        };
    }

    //  ── Reddit ───────────────────────────────────────────────────
    if h == "reddit.com" || h == "old.reddit.com" || h == "np.reddit.com" {
        //  `/r/loquesea/comments/<id>/<slug>` → el hilo, sin el slug, que
        //  cambia y ensucia el duplicado.
        let trozos: Vec<&str> = ruta.split('/').filter(|s| !s.is_empty()).collect();
        let canonica = if trozos.len() >= 4 && trozos[0] == "r" && trozos[2] == "comments" {
            Some(format!(
                "https://reddit.com/r/{}/comments/{}",
                trozos[1], trozos[3]
            ))
        } else {
            None
        };
        let titulo = trozos
            .get(4)
            .map(|s| deslugar(s))
            .filter(|s| !s.is_empty());
        return Leido {
            fuente: Fuente { id: "reddit", nombre: "Reddit", tipo: "url" },
            canonica,
            titulo,
        };
    }

    //  ── X ────────────────────────────────────────────────────────
    if h == "x.com" || h == "twitter.com" || h == "mobile.twitter.com"
        || h == "nitter.net"
    {
        let trozos: Vec<&str> = ruta.split('/').filter(|s| !s.is_empty()).collect();
        let canonica = if trozos.len() >= 3 && trozos[1] == "status" {
            Some(format!("https://x.com/{}/status/{}", trozos[0], trozos[2]))
        } else {
            None
        };
        return Leido {
            fuente: Fuente { id: "x", nombre: "X", tipo: "url" },
            canonica,
            titulo: trozos.first().map(|u| format!("@{}", u)),
        };
    }

    //  ── GitHub ───────────────────────────────────────────────────
    if h == "github.com" {
        let trozos: Vec<&str> = ruta.split('/').filter(|s| !s.is_empty()).collect();
        if trozos.len() >= 2 {
            return Leido {
                fuente: Fuente { id: "github", nombre: "GitHub", tipo: "code" },
                canonica: Some(format!("https://github.com/{}/{}", trozos[0], trozos[1])),
                titulo: Some(format!("{}/{}", trozos[0], trozos[1])),
            };
        }
    }

    //  ── Vimeo y Twitch, que también son vídeo ────────────────────
    if h == "vimeo.com" {
        let id = primer_segmento(&ruta).filter(|s| s.chars().all(|c| c.is_ascii_digit()));
        return Leido {
            fuente: Fuente { id: "vimeo", nombre: "Vimeo", tipo: "video" },
            canonica: id.as_ref().map(|v| format!("https://vimeo.com/{}", v)),
            titulo: None,
        };
    }
    if h == "twitch.tv" || h == "clips.twitch.tv" {
        return Leido {
            fuente: Fuente { id: "twitch", nombre: "Twitch", tipo: "video" },
            canonica: None,
            titulo: None,
        };
    }

    //  ── Y lo que se sabe por la extensión ────────────────────────
    //
    //  Un enlace directo a un PDF o a una imagen es esa cosa, no «una web». Es
    //  la misma pista que usa el enrutador de arrastres.
    let ext = ruta
        .rsplit_once('.')
        .map(|(_, e)| e.to_lowercase())
        .unwrap_or_default();
    let por_ext = match ext.as_str() {
        "pdf" | "epub" => Some(("pdf", "PDF", "document")),
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "avif" => Some(("imagen", "Imagen", "image")),
        "mp4" | "webm" | "mkv" | "mov" => Some(("video", "Vídeo", "video")),
        "mp3" | "flac" | "opus" | "ogg" | "wav" => Some(("audio", "Audio", "audio")),
        _ => None,
    };
    if let Some((id, nombre, tipo)) = por_ext {
        return Leido {
            fuente: Fuente { id, nombre, tipo },
            canonica: None,
            titulo: ruta
                .rsplit('/')
                .next()
                .map(|s| s.to_string())
                .filter(|s| !s.is_empty()),
        };
    }

    //  Lo demás es una página, y su dominio es el mejor título de emergencia
    //  que hay: «ejemplo.com» dice más que «Sin título».
    Leido {
        fuente: GENERICA,
        canonica: None,
        titulo: if h.is_empty() { None } else { Some(h.to_string()) },
    }
}

fn seguro_en_id(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

fn partes(url: &str) -> (String, String, String) {
    let sin = url
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let sin = sin.split('#').next().unwrap_or("");
    let (autoridad, resto) = match sin.find('/') {
        Some(i) => (&sin[..i], &sin[i..]),
        None => (sin, ""),
    };
    let (ruta, consulta) = match resto.find('?') {
        Some(i) => (&resto[..i], &resto[i + 1..]),
        None => (resto, ""),
    };
    (
        autoridad.to_lowercase().split(':').next().unwrap_or("").to_string(),
        ruta.to_string(),
        consulta.to_string(),
    )
}

fn parametro(consulta: &str, nombre: &str) -> Option<String> {
    for par in consulta.split('&') {
        if let Some((k, v)) = par.split_once('=') {
            if k == nombre && !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

fn primer_segmento(ruta: &str) -> Option<String> {
    ruta.split('/')
        .find(|s| !s.is_empty())
        .map(|s| s.to_string())
}

//  `un-titulo-con-guiones` → `Un titulo con guiones`. Es lo que trae la URL de
//  Reddit y es mejor título que el identificador.
fn deslugar(s: &str) -> String {
    let limpio = s.replace(['-', '_'], " ");
    let mut c = limpio.chars();
    match c.next() {
        Some(p) => p.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}
