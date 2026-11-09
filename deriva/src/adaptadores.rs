//  Where the thing you saved came from.
//
//  A YouTube link and a PDF link are not the same, even though both are
//  `text/uri-list`: one is a video with an identifier and thumbnail, the other
//  is a document. Showing both as “a link” treats the library like a bookmark
//  list.
//
//  This does NOT access the network. It is everything that can be learned by
//  looking at the URL, which is quite a lot: where it came from, what it is, and
//  its canonical form. Anything that must be fetched externally —the actual
//  title and thumbnail— is fetched by the host, which already has its
//  anti-SSRF protections written and reviewed. The worker has no network access
//  and will not get it.
//
//  The source list is closed, like the others: it appears in the interface with
//  a color and a name, and an open list would let any URL invent a source that
//  the interface cannot render.

#[derive(Debug, Clone, PartialEq)]
pub struct Fuente {
    //  The identifier, from the closed list.
    pub id: &'static str,
    //  Its human-readable name.
    pub nombre: &'static str,
    //  What kind of thing it is. This takes precedence over the extension: a
    //  YouTube link is a video even if the URL does not end in `.mp4`.
    pub tipo: &'static str,
}

pub const GENERICA: Fuente = Fuente {
    id: "web",
    nombre: "Web",
    tipo: "url",
};

//  What can be derived from a URL without asking anyone for anything.
#[derive(Debug)]
pub struct Leido {
    pub fuente: Fuente,
    //  The canonical form for THAT site, not the generic one.
    //
    //  `youtu.be/ID`, `youtube.com/watch?v=ID`, and
    //  `youtube.com/watch?v=ID&t=90` are the same video. Without this,
    //  dragging the same video from a phone and desktop saves two captures,
    //  and duplicate detection —half the point of this— misses it.
    pub canonica: Option<String>,
    //  A fallback title until the actual one arrives —from the page's
    //  metadata, fetched separately and possibly unavailable.
    pub titulo: Option<String>,
}

pub fn leer(url: &str) -> Leido {
    let (host, ruta, consulta) = partes(url);
    let h = host.trim_start_matches("www.");

    //  ── YouTube ──────────────────────────────────────────────────
    if h == "youtube.com" || h == "m.youtube.com" || h == "music.youtube.com" || h == "youtu.be" {
        let id = if h == "youtu.be" {
            primer_segmento(&ruta)
        } else if ruta.starts_with("/shorts/")
            || ruta.starts_with("/embed/")
            || ruta.starts_with("/live/")
        {
            ruta.splitn(3, '/').nth(2).map(|s| s.to_string())
        } else {
            parametro(&consulta, "v")
        };
        //  Eleven characters, the length of a video identifier. Without
        //  checking this, a channel URL would produce a nonexistent
        //  thumbnail.
        let id = id.filter(|s| s.len() == 11 && s.chars().all(seguro_en_id));
        return Leido {
            fuente: Fuente {
                id: "youtube",
                nombre: "YouTube",
                tipo: "video",
            },
            canonica: id
                .as_ref()
                .map(|v| format!("https://youtube.com/watch?v={}", v)),
            titulo: None,
        };
    }

    //  ── Reddit ───────────────────────────────────────────────────
    if h == "reddit.com" || h == "old.reddit.com" || h == "np.reddit.com" {
        //  `/r/whatever/comments/<id>/<slug>` → the thread without the slug,
        //  which changes and gets in the way of duplicate detection.
        let trozos: Vec<&str> = ruta.split('/').filter(|s| !s.is_empty()).collect();
        let canonica = if trozos.len() >= 4 && trozos[0] == "r" && trozos[2] == "comments" {
            Some(format!(
                "https://reddit.com/r/{}/comments/{}",
                trozos[1], trozos[3]
            ))
        } else {
            None
        };
        let titulo = trozos.get(4).map(|s| deslugar(s)).filter(|s| !s.is_empty());
        return Leido {
            fuente: Fuente {
                id: "reddit",
                nombre: "Reddit",
                tipo: "url",
            },
            canonica,
            titulo,
        };
    }

    //  ── X ────────────────────────────────────────────────────────
    if h == "x.com" || h == "twitter.com" || h == "mobile.twitter.com" || h == "nitter.net" {
        let trozos: Vec<&str> = ruta.split('/').filter(|s| !s.is_empty()).collect();
        let canonica = if trozos.len() >= 3 && trozos[1] == "status" {
            Some(format!("https://x.com/{}/status/{}", trozos[0], trozos[2]))
        } else {
            None
        };
        return Leido {
            fuente: Fuente {
                id: "x",
                nombre: "X",
                tipo: "url",
            },
            canonica,
            titulo: trozos.first().map(|u| format!("@{}", u)),
        };
    }

    //  ── GitHub ───────────────────────────────────────────────────
    if h == "github.com" {
        let trozos: Vec<&str> = ruta.split('/').filter(|s| !s.is_empty()).collect();
        if trozos.len() >= 2 {
            return Leido {
                fuente: Fuente {
                    id: "github",
                    nombre: "GitHub",
                    tipo: "code",
                },
                canonica: Some(format!("https://github.com/{}/{}", trozos[0], trozos[1])),
                titulo: Some(format!("{}/{}", trozos[0], trozos[1])),
            };
        }
    }

    //  ── Vimeo and Twitch, which are also video ──────────────────
    if h == "vimeo.com" {
        let id = primer_segmento(&ruta).filter(|s| s.chars().all(|c| c.is_ascii_digit()));
        return Leido {
            fuente: Fuente {
                id: "vimeo",
                nombre: "Vimeo",
                tipo: "video",
            },
            canonica: id.as_ref().map(|v| format!("https://vimeo.com/{}", v)),
            titulo: None,
        };
    }
    if h == "twitch.tv" || h == "clips.twitch.tv" {
        return Leido {
            fuente: Fuente {
                id: "twitch",
                nombre: "Twitch",
                tipo: "video",
            },
            canonica: None,
            titulo: None,
        };
    }

    //  ── And what can be inferred from the extension ─────────────
    //
    //  A direct link to a PDF or image is that type of thing, not “a
    //  website.” This is the same clue used by the drag router.
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

    //  Everything else is a page, and its domain is the best fallback title
    //  available: “example.com” says more than “Untitled.”
    Leido {
        fuente: GENERICA,
        canonica: None,
        titulo: if h.is_empty() {
            None
        } else {
            Some(h.to_string())
        },
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
        autoridad
            .to_lowercase()
            .split(':')
            .next()
            .unwrap_or("")
            .to_string(),
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

//  `a-title-with-hyphens` → `A title with hyphens`. This is what Reddit URLs
//  provide, and it makes a better title than the identifier.
fn deslugar(s: &str) -> String {
    let limpio = s.replace(['-', '_'], " ");
    let mut c = limpio.chars();
    match c.next() {
        Some(p) => p.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}
