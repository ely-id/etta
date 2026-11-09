//  Worker tests.
//
//  Each test runs against a fresh library in a temporary directory, with a
//  lock around it because the path is passed through an environment variable
//  shared by the whole process. Without the lock, parallel tests would
//  overwrite each other's library and the one that ran second would fail.

use super::*;
use std::sync::Mutex;

static CERROJO: Mutex<()> = Mutex::new(());

fn banco<T>(f: impl FnOnce(&rusqlite::Connection) -> T) -> T {
    let _g = CERROJO.lock().unwrap_or_else(|e| e.into_inner());
    let mut b = [0u8; 8];
    util::azar(&mut b);
    let dir = std::env::temp_dir().join(format!("deriva-prueba-{}", util::hex(&b)));
    std::env::set_var("ETTA_DERIVA_DIR", &dir);
    let db = db::abrir().expect("no abre");
    let r = f(&db);
    drop(db);
    let _ = std::fs::remove_dir_all(&dir);
    std::env::remove_var("ETTA_DERIVA_DIR");
    r
}

fn texto(t: &str) -> ingest::Peticion {
    ingest::Peticion {
        tipo: "text".into(),
        text: Some(t.into()),
        ..Default::default()
    }
}

// ── basics ────────────────────────────────────────────────────────

#[test]
fn base64_ida_y_vuelta() {
    for caso in [
        &b""[..],
        &b"a"[..],
        &b"ab"[..],
        &b"abc"[..],
        &b"\x00\xff\x10marea"[..],
    ] {
        let v = util::base64(&util::a_base64(caso)).expect("descodifica");
        assert_eq!(v, caso, "no vuelve igual");
    }
    //  Also split into lines, as large base64 payloads arrive.
    assert_eq!(util::base64("bWFy\nZWE=").unwrap(), b"marea");
    //  Report invalid base64 instead of guessing.
    assert!(util::base64("no válido!").is_none());
}

//  Links to the same page can be written in ten different ways. If these are
//  not normalized together, “you already have this” never works.
#[test]
fn la_misma_url_escrita_de_varias_maneras_es_una() {
    let esperado = Some("https://ejemplo.com/articulo".to_string());
    for v in [
        "https://ejemplo.com/articulo",
        "https://www.ejemplo.com/articulo",
        "https://EJEMPLO.com/articulo",
        "https://ejemplo.com:443/articulo",
        "https://ejemplo.com/articulo#seccion-3",
        "https://ejemplo.com/articulo?utm_source=boletin&utm_medium=email",
        "  https://ejemplo.com/articulo?fbclid=abc  ",
    ] {
        assert_eq!(util::canonizar_url(v), esperado, "con {}", v);
    }
    //  But real distinctions must be preserved: video IDs are case-sensitive,
    //  and “normalizing” that would turn two videos into one.
    assert_ne!(
        util::canonizar_url("https://v.com/w?v=aB3"),
        util::canonizar_url("https://v.com/w?v=ab3")
    );
    //  Non-web items are not canonicalized: `file://` duplicates are matched
    //  by content, not URL.
    assert_eq!(util::canonizar_url("file:///home/a/x.png"), None);
    assert_eq!(util::canonizar_url("javascript:alert(1)"), None);
}

#[test]
fn el_mismo_texto_con_otros_espacios_es_el_mismo_texto() {
    let a = util::hash_de_texto("Hola   mundo\n\n con espacios ");
    let b = util::hash_de_texto("Hola mundo con espacios");
    assert_eq!(a, b);
    assert_ne!(a, util::hash_de_texto("Hola mundo con espacio"));
}

//  Searching for `it's` must not fail. User input is not an FTS5 query, and
//  passing it through as-is produces a syntax error instead of results.
#[test]
fn lo_que_escribe_el_usuario_nunca_es_sintaxis() {
    for bruta in [
        "it's",
        "\"sin cerrar",
        "* OR 1=1",
        "a AND NOT b",
        "^inicio",
        "-- comentario",
        "(",
    ] {
        let q = search::consulta_fts(bruta);
        if let Some(q) = q {
            //  The real test: SQLite accepts it.
            banco(|db| {
                db.query_row(
                    "SELECT COUNT(*) FROM captures_fts WHERE captures_fts MATCH ?1",
                    [&q],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap_or_else(|e| panic!("«{}» → «{}» no compila: {}", bruta, q, e));
            });
        }
    }
    //  An empty query finds nothing instead of everything.
    assert_eq!(search::consulta_fts("   "), None);
    assert_eq!(search::consulta_fts("!!!"), None);
}

// ── saving ───────────────────────────────────────────────────────

#[test]
fn guardar_un_texto_y_encontrarlo() {
    banco(|db| {
        let mut p = texto("La marea sube dos veces al día y baja otras dos.");
        p.title = Some("Mareas".into());
        p.tags = vec!["mar".into()];
        let r = ingest::ingerir(db, &p);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].status, "saved");

        assert_eq!(search::buscar(db, "marea", 10).unwrap().len(), 1);
        //  By title, body, and tag.
        assert_eq!(search::buscar(db, "Mareas", 10).unwrap().len(), 1);
        assert_eq!(search::buscar(db, "mar", 10).unwrap().len(), 1);
        //  And with a snippet highlighted, as shown in the list.
        assert!(search::buscar(db, "marea", 10).unwrap()[0]
            .snippet
            .is_some());
    });
}

#[test]
fn lo_mismo_dos_veces_es_un_duplicado() {
    banco(|db| {
        let p = texto("Exactamente lo mismo.");
        let a = ingest::ingerir(db, &p);
        assert_eq!(a[0].status, "saved");
        //  With different whitespace: it is still the same.
        let b = ingest::ingerir(db, &texto("Exactamente    lo\n mismo. "));
        assert_eq!(b[0].status, "duplicate");
        assert_eq!(b[0].duplicate_of, a[0].id);
        assert_eq!(search::cuentas(db).unwrap().captures, 1);
    });
}

#[test]
fn la_misma_pagina_con_otra_coletilla_es_un_duplicado() {
    banco(|db| {
        let una = ingest::Peticion {
            tipo: "url".into(),
            source_url: Some("https://ejemplo.com/post".into()),
            ..Default::default()
        };
        assert_eq!(ingest::ingerir(db, &una)[0].status, "saved");
        let otra = ingest::Peticion {
            tipo: "url".into(),
            source_url: Some("https://www.ejemplo.com/post?utm_source=x".into()),
            ..Default::default()
        };
        assert_eq!(ingest::ingerir(db, &otra)[0].status, "duplicate");
    });
}

//  Saving something you trashed must work. Otherwise, deletion would be a
//  life sentence: you could never save that page again.
#[test]
fn lo_tirado_no_cuenta_como_duplicado() {
    banco(|db| {
        let a = ingest::ingerir(db, &texto("Una cosa."));
        let id = a[0].id.clone().unwrap();
        db.execute("UPDATE captures SET trashed_at = 1 WHERE id = ?1", [&id])
            .unwrap();
        db::reindexar(db, &id).unwrap();
        assert_eq!(ingest::ingerir(db, &texto("Una cosa."))[0].status, "saved");
        //  Trashed items stop appearing in search.
        assert_eq!(search::buscar(db, "Una cosa", 10).unwrap().len(), 1);
    });
}

#[test]
fn un_tipo_que_no_existe_se_rechaza() {
    banco(|db| {
        let p = ingest::Peticion {
            tipo: "cualquiera".into(),
            ..Default::default()
        };
        let r = ingest::ingerir(db, &p);
        assert_eq!(r[0].status, "rejected");
        assert_eq!(r[0].reason.as_deref(), Some("tipo_desconocido"));
    });
}

#[test]
fn sin_contenido_no_se_guarda_una_captura_vacia() {
    banco(|db| {
        let r = ingest::ingerir(
            db,
            &ingest::Peticion {
                tipo: "text".into(),
                ..Default::default()
            },
        );
        assert_eq!(r[0].status, "rejected");
        assert_eq!(r[0].reason.as_deref(), Some("sin_contenido"));
    });
}

// ── files ─────────────────────────────────────────────────────────

#[test]
fn un_fichero_se_copia_una_sola_vez_aunque_se_guarde_dos() {
    banco(|db| {
        let f = std::env::temp_dir().join(format!("deriva-f-{}.txt", util::ahora_ms()));
        std::fs::write(&f, b"unos bytes cualesquiera").unwrap();

        let p = ingest::Peticion {
            tipo: "document".into(),
            paths: vec![f.to_string_lossy().to_string()],
            ..Default::default()
        };
        let a = ingest::ingerir(db, &p);
        assert_eq!(a[0].status, "saved");

        //  The blob is where the plan says, and its name is its hash.
        let hash = util::sha256(b"unos bytes cualesquiera");
        let sitio = paths::blob_de(&hash);
        assert!(sitio.exists(), "no está en {}", sitio.display());
        assert_eq!(std::fs::read(&sitio).unwrap(), b"unos bytes cualesquiera");

        //  The same file again: a duplicate, with no extra copy.
        assert_eq!(ingest::ingerir(db, &p)[0].status, "duplicate");
        assert_eq!(blobs::todos().unwrap().len(), 1);

        let _ = std::fs::remove_file(&f);
    });
}

#[test]
fn un_lote_da_un_resultado_por_fichero() {
    banco(|db| {
        let mut rutas = Vec::new();
        for i in 0..3 {
            let f = std::env::temp_dir().join(format!("deriva-l-{}-{}.txt", util::ahora_ms(), i));
            std::fs::write(&f, format!("fichero {}", i)).unwrap();
            rutas.push(f.to_string_lossy().to_string());
        }
        //  And one duplicate: of four dragged items, three new and one already
        //  present is not a batch failure.
        rutas.push(rutas[0].clone());
        let p = ingest::Peticion {
            tipo: "file".into(),
            paths: rutas.clone(),
            ..Default::default()
        };
        let r = ingest::ingerir(db, &p);
        assert_eq!(r.len(), 4);
        assert_eq!(r.iter().filter(|x| x.status == "saved").count(), 3);
        assert_eq!(r.iter().filter(|x| x.status == "duplicate").count(), 1);
        for ruta in rutas.iter().take(3) {
            let _ = std::fs::remove_file(ruta);
        }
    });
}

//  Resolve symlinks before copying. Without this, a symlink named like a photo
//  and pointing to `/etc/passwd` would be copied into the library without a
//  second thought.
#[test]
fn lo_que_no_se_deja_copiar() {
    banco(|db| {
        let casos = [
            ("relativa.txt", "ruta_relativa"),
            ("/no/existe/de/verdad.txt", "no_existe"),
            ("/proc/self/status", "ruta_del_sistema"),
            ("/tmp", "no_es_un_fichero"),
        ];
        for (ruta, motivo) in casos {
            let p = ingest::Peticion {
                tipo: "file".into(),
                paths: vec![ruta.into()],
                ..Default::default()
            };
            let r = ingest::ingerir(db, &p);
            assert_eq!(r[0].status, "rejected", "con {}", ruta);
            assert_eq!(r[0].reason.as_deref(), Some(motivo), "con {}", ruta);
        }
        //  The library does not copy itself.
        let dentro = paths::base().join("library.sqlite3");
        let p = ingest::Peticion {
            tipo: "file".into(),
            paths: vec![dentro.to_string_lossy().to_string()],
            ..Default::default()
        };
        assert_eq!(
            ingest::ingerir(db, &p)[0].reason.as_deref(),
            Some("ya_es_de_la_biblioteca")
        );
    });
}

#[test]
fn una_imagen_en_linea_se_guarda_por_sus_bytes() {
    banco(|db| {
        let datos = b"\x89PNG\r\n\x1a\nfingido";
        let p = ingest::Peticion {
            tipo: "image".into(),
            source_url: Some("https://ejemplo.com/pagina".into()),
            inline: Some(ingest::EnLinea {
                mime: Some("image/png".into()),
                name: Some("foto.png".into()),
                base64: util::a_base64(datos),
            }),
            ..Default::default()
        };
        let r = ingest::ingerir(db, &p);
        assert_eq!(r[0].status, "saved");
        let f = search::una(db, r[0].id.as_ref().unwrap()).unwrap().unwrap();
        assert_eq!(f.blob_hash.as_deref(), Some(util::sha256(datos).as_str()));
        assert_eq!(f.blob_bytes, Some(datos.len() as i64));
        assert_eq!(f.blob_mime.as_deref(), Some("image/png"));
    });
}

#[test]
fn un_base64_roto_se_rechaza_en_vez_de_guardar_basura() {
    banco(|db| {
        let p = ingest::Peticion {
            tipo: "image".into(),
            inline: Some(ingest::EnLinea {
                mime: None,
                name: None,
                base64: "esto no es base64 ###".into(),
            }),
            ..Default::default()
        };
        assert_eq!(
            ingest::ingerir(db, &p)[0].reason.as_deref(),
            Some("base64_invalido")
        );
    });
}

// ── the index ─────────────────────────────────────────────────────
//  The index is not maintained by triggers. This test validates that decision:
//  tags live in another table, and a trigger on `captures` would not see them.
#[test]
fn cambiar_una_etiqueta_se_busca() {
    banco(|db| {
        let r = ingest::ingerir(db, &texto("Un texto sin nada especial."));
        let id = r[0].id.clone().unwrap();
        assert_eq!(search::buscar(db, "bicicleta", 10).unwrap().len(), 0);

        ingest::poner_etiqueta(db, &id, "Bicicleta").unwrap();
        db::reindexar(db, &id).unwrap();
        assert_eq!(search::buscar(db, "bicicleta", 10).unwrap().len(), 1);
    });
}

#[test]
fn tirar_y_recuperar_saca_y_devuelve_del_indice() {
    banco(|db| {
        let r = ingest::ingerir(
            db,
            &texto("Algo que se busca por su palabra rara: xilofono."),
        );
        let id = r[0].id.clone().unwrap();
        assert_eq!(search::buscar(db, "xilofono", 10).unwrap().len(), 1);

        let p = proto::Peticion {
            v: 1,
            id: "t".into(),
            method: "trash".into(),
            params: serde_json::json!({ "id": id }),
        };
        assert!(proto::despachar(db, &p).ok);
        assert_eq!(search::buscar(db, "xilofono", 10).unwrap().len(), 0);
        //  It is still there: trashing is not destruction.
        assert!(search::una(db, &id).unwrap().is_some());

        let p = proto::Peticion {
            method: "untrash".into(),
            ..p
        };
        assert!(proto::despachar(db, &p).ok);
        assert_eq!(search::buscar(db, "xilofono", 10).unwrap().len(), 1);
    });
}

//  The title weighs more than the body. Without weights, a stray word in a
//  ten-thousand-word article competes with the same word in the title.
#[test]
fn el_titulo_manda_sobre_el_cuerpo() {
    banco(|db| {
        let mut cuerpo = texto(&format!(
            "{} ballena {}",
            "relleno ".repeat(200),
            "relleno ".repeat(200)
        ));
        cuerpo.title = Some("Un artículo largo".into());
        ingest::ingerir(db, &cuerpo);

        let mut titulo = texto("Nada que ver aquí dentro.");
        titulo.title = Some("Ballena".into());
        ingest::ingerir(db, &titulo);

        let r = search::buscar(db, "ballena", 10).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].title, "Ballena", "el título tendría que ir primero");
    });
}

// ── integrity, backup, and export ────────────────────────────────

#[test]
fn una_biblioteca_recien_hecha_esta_entera() {
    banco(|db| {
        ingest::ingerir(db, &texto("algo"));
        let i = mantenimiento::integridad(db, true).unwrap();
        assert!(i.ok, "{:?}", i);
        assert_eq!(i.sqlite, "ok");
        assert!(i.missing_blobs.is_empty());
        assert_eq!(i.orphan_blobs, 0);
    });
}

//  The failure that really matters: the library shows something that can no
//  longer be opened. Report it by name instead of failing when it is opened.
#[test]
fn un_blob_que_desaparece_se_denuncia() {
    banco(|db| {
        let f = std::env::temp_dir().join(format!("deriva-i-{}.bin", util::ahora_ms()));
        std::fs::write(&f, b"contenido").unwrap();
        let p = ingest::Peticion {
            tipo: "file".into(),
            paths: vec![f.to_string_lossy().to_string()],
            ..Default::default()
        };
        ingest::ingerir(db, &p);
        let _ = std::fs::remove_file(&f);

        let hash = util::sha256(b"contenido");
        std::fs::remove_file(paths::blob_de(&hash)).unwrap();

        let i = mantenimiento::integridad(db, false).unwrap();
        assert!(!i.ok);
        assert_eq!(i.missing_blobs, vec![hash]);
    });
}

#[test]
fn un_blob_corrupto_solo_sale_en_la_revision_profunda() {
    banco(|db| {
        let f = std::env::temp_dir().join(format!("deriva-c-{}.bin", util::ahora_ms()));
        std::fs::write(&f, b"original").unwrap();
        let p = ingest::Peticion {
            tipo: "file".into(),
            paths: vec![f.to_string_lossy().to_string()],
            ..Default::default()
        };
        ingest::ingerir(db, &p);
        let _ = std::fs::remove_file(&f);

        //  Someone edits the blob. Its name is still the old hash, so this is
        //  found only by hashing it again.
        std::fs::write(paths::blob_de(&util::sha256(b"original")), b"otra cosa").unwrap();

        assert!(mantenimiento::integridad(db, false).unwrap().ok);
        let profunda = mantenimiento::integridad(db, true).unwrap();
        assert!(!profunda.ok);
        assert_eq!(profunda.corrupt_blobs.len(), 1);
    });
}

#[test]
fn un_indice_incompleto_se_repara() {
    banco(|db| {
        let r = ingest::ingerir(db, &texto("palabra rarisima: zurriburri"));
        let id = r[0].id.clone().unwrap();
        db.execute("DELETE FROM captures_fts WHERE capture_id = ?1", [&id])
            .unwrap();
        assert_eq!(search::buscar(db, "zurriburri", 10).unwrap().len(), 0);
        assert!(!mantenimiento::integridad(db, false).unwrap().ok);

        assert_eq!(mantenimiento::reparar_indice(db).unwrap(), 1);
        assert_eq!(search::buscar(db, "zurriburri", 10).unwrap().len(), 1);
        assert!(mantenimiento::integridad(db, false).unwrap().ok);
    });
}

#[test]
fn la_copia_de_seguridad_se_puede_abrir_y_tiene_lo_mismo() {
    banco(|db| {
        for i in 0..3 {
            ingest::ingerir(db, &texto(&format!("cosa numero {}", i)));
        }
        let c = mantenimiento::respaldar(db, 5).unwrap();
        assert!(c.bytes > 0);

        let copia = db::abrir_en(std::path::Path::new(&c.path)).unwrap();
        assert_eq!(search::cuentas(&copia).unwrap().captures, 3);
        //  It can be searched too: the index is included in the backup.
        assert_eq!(search::buscar(&copia, "numero", 10).unwrap().len(), 3);
    });
}

#[test]
fn las_copias_viejas_se_tiran() {
    banco(|db| {
        ingest::ingerir(db, &texto("una"));
        for _ in 0..4 {
            mantenimiento::respaldar(db, 2).unwrap();
            //  Names include milliseconds; without a delay, two backups in the
            //  same millisecond would use the same filename.
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let cuantas = std::fs::read_dir(paths::backups()).unwrap().count();
        assert_eq!(cuantas, 2);
    });
}

//  Leaving must be possible, with usable files: an attachment named `9f2a…`
//  is still yours, but you cannot open it.
#[test]
fn exportar_deja_todo_legible_sin_este_programa() {
    banco(|db| {
        let f = std::env::temp_dir().join(format!("deriva-e-{}.txt", util::ahora_ms()));
        std::fs::write(&f, b"el adjunto").unwrap();
        let mut p = ingest::Peticion {
            tipo: "document".into(),
            paths: vec![f.to_string_lossy().to_string()],
            ..Default::default()
        };
        p.tags = vec!["viaje".into()];
        ingest::ingerir(db, &p);
        ingest::ingerir(db, &texto("una nota suelta"));
        let _ = std::fs::remove_file(&f);

        let destino = paths::base().join("salida");
        let x = mantenimiento::exportar(db, &destino).unwrap();
        assert_eq!(x.captures, 2);
        assert_eq!(x.files, 1);

        let jsonl = std::fs::read_to_string(destino.join("captures.jsonl")).unwrap();
        assert_eq!(jsonl.lines().count(), 2);
        assert!(jsonl.contains("una nota suelta"), "falta el texto entero");
        assert!(jsonl.contains("viaje"), "faltan las etiquetas");
        assert!(destino.join("manifest.json").exists());

        //  The attachment, with a human-readable name and intact contents.
        let ficheros: Vec<_> = std::fs::read_dir(destino.join("files"))
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(ficheros.len(), 1);
        let nombre = ficheros[0].file_name().to_string_lossy().to_string();
        assert!(nombre.ends_with(".txt"), "se llama «{}»", nombre);
        assert_eq!(std::fs::read(ficheros[0].path()).unwrap(), b"el adjunto");
    });
}

// ── what the page needs ──────────────────────────────────────────

//  Navigation comes from the database, not a list in QML: users create
//  folders, and a fixed sidebar would stop counting them when a fourth is
//  created.
#[test]
fn la_navegacion_cuenta_lo_que_hay() {
    banco(|db| {
        let mut p = texto("una cosa");
        p.space = Some("Proyectos".into());
        p.tags = vec!["azul".into()];
        ingest::ingerir(db, &p);
        ingest::ingerir(db, &texto("otra cosa suelta"));

        let s = search::secciones(db).unwrap();
        let de = |id: &str| s.iter().find(|x| x.id == id).map(|x| x.count).unwrap_or(-1);
        assert_eq!(de("inbox"), 2);
        assert_eq!(de("today"), 2);
        //  The three from the plan are seeded, and only the one with content
        //  counts.
        assert_eq!(de("proyectos"), 1);
        assert_eq!(de("leer-luego"), 0);
        assert_eq!(de("inspiracion"), 0);
        assert_eq!(de("tags"), 1);
        //  In the order shown in the design: inbox, today, folders, tags.
        assert_eq!(s[0].id, "inbox");
        assert_eq!(s[1].id, "today");
        assert_eq!(s[s.len() - 1].id, "tags");
    });
}

#[test]
fn se_puede_listar_por_espacio_y_por_etiqueta() {
    banco(|db| {
        let mut a = texto("del proyecto");
        a.space = Some("Proyectos".into());
        a.tags = vec!["azul".into()];
        ingest::ingerir(db, &a);
        let mut b = texto("de inspiración");
        b.space = Some("Inspiración".into());
        ingest::ingerir(db, &b);

        let todo = search::Filtro::default();
        assert_eq!(search::listar(db, 50, None, &todo).unwrap().len(), 2);

        let solo = search::Filtro {
            space: Some("proyectos".into()),
            ..Default::default()
        };
        let r = search::listar(db, 50, None, &solo).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].title, "del proyecto");

        let etiq = search::Filtro {
            tag: Some("azul".into()),
            ..Default::default()
        };
        assert_eq!(search::listar(db, 50, None, &etiq).unwrap().len(), 1);

        //  A nonexistent folder returns nothing, not everything. A filter
        //  silently ignored is worse than a nonexistent filter.
        let nada = search::Filtro {
            space: Some("marte".into()),
            ..Default::default()
        };
        assert_eq!(search::listar(db, 50, None, &nada).unwrap().len(), 0);
    });
}

//  “Resurfaced” means just that, not “recently saved.” Filling that area with
//  recent items would be a lie under its heading.
#[test]
fn solo_vuelve_a_la_superficie_lo_que_ya_tocaba() {
    banco(|db| {
        for i in 0..4 {
            ingest::ingerir(db, &texto(&format!("cosa {}", i)));
        }
        assert_eq!(
            search::reaparecidas(db, 5).unwrap().len(),
            0,
            "no había nada aplazado"
        );

        let ids: Vec<String> = search::listar(db, 5, None, &search::Filtro::default())
            .unwrap()
            .iter()
            .map(|f| f.id.clone())
            .collect();
        //  One for yesterday and another for a week from now.
        let ahora = util::ahora_ms();
        db.execute(
            "UPDATE captures SET snoozed_until = ?2 WHERE id = ?1",
            rusqlite::params![ids[0], ahora - 86_400_000],
        )
        .unwrap();
        db.execute(
            "UPDATE captures SET snoozed_until = ?2 WHERE id = ?1",
            rusqlite::params![ids[1], ahora + 604_800_000],
        )
        .unwrap();

        let v = search::reaparecidas(db, 5).unwrap();
        assert_eq!(v.len(), 1, "la de la semana que viene todavía no toca");
        assert_eq!(v[0].id, ids[0]);
    });
}

//  The inspector is the only writer, and only writes what belongs to you: the
//  note, tags, and favorite. Content is not edited.
#[test]
fn la_nota_y_las_etiquetas_se_pueden_cambiar_y_se_buscan() {
    banco(|db| {
        let r = ingest::ingerir(db, &texto("un texto cualquiera"));
        let id = r[0].id.clone().unwrap();

        let p = proto::Peticion {
            v: 1,
            id: "a".into(),
            method: "annotate".into(),
            params: serde_json::json!({
                "id": id, "note": "lo guardé por el tercer párrafo",
                "tags": ["relectura", "AZUL"], "favorite": true,
            }),
        };
        let resp = proto::despachar(db, &p);
        assert!(resp.ok, "{:?}", resp.error);

        let f = search::una(db, &id).unwrap().unwrap();
        assert_eq!(f.note, "lo guardé por el tercer párrafo");
        assert!(f.favorite);
        //  Normalized: two tags with the same name are the same tag.
        assert_eq!(f.tags, vec!["azul".to_string(), "relectura".to_string()]);

        //  Search by note and tag, verifying that the index was rebuilt.
        assert_eq!(search::buscar(db, "tercer", 10).unwrap().len(), 1);
        assert_eq!(search::buscar(db, "relectura", 10).unwrap().len(), 1);

        //  Content, on the other hand, cannot be changed: there is no method.
        let malo = proto::Peticion {
            method: "edit_content".into(),
            ..p
        };
        assert!(!proto::despachar(db, &malo).ok);
    });
}

//  Yesterday's library opens today.
//
//  This is the bug that almost slipped through: the `source` column was added
//  by editing the schema, which only works for new databases. Someone who has
//  been using this since last week has `user_version = 1`, no such column, and
//  every capture read expects it.
//
//  So the test is not “migration succeeds,” but “after migration, yesterday's
//  library can be read in full and all its contents are still there.”
#[test]
fn una_biblioteca_de_la_version_anterior_se_migra() {
    let _g = CERROJO.lock().unwrap_or_else(|e| e.into_inner());
    let mut b = [0u8; 8];
    util::azar(&mut b);
    let dir = std::env::temp_dir().join(format!("deriva-vieja-{}", util::hex(&b)));
    std::env::set_var("ETTA_DERIVA_DIR", &dir);

    //  A real v1: the schema from that version, which is today's schema without
    //  the source column and with a thumbnail path instead of its hash.
    {
        let vieja = db::abrir().expect("no abre");
        //  Insert the data first, then restore the original schema. In the
        //  reverse order, today's INSERT —which names the column—would fail,
        //  leaving the test without the data that must survive.
        ingest::ingerir(&vieja, &texto("lo de ayer"));
        vieja
            .execute_batch(
                "ALTER TABLE captures DROP COLUMN source;
                 ALTER TABLE captures DROP COLUMN summary;
                 ALTER TABLE captures DROP COLUMN summary_at;
                 ALTER TABLE captures DROP COLUMN preview_hash;
                 ALTER TABLE captures ADD COLUMN preview_path TEXT;
                 DROP TABLE capture_vectors;
                 DROP TABLE captures_fts;
                 CREATE VIRTUAL TABLE captures_fts USING fts5(
                     capture_id UNINDEXED,
                     title, author, excerpt, note, content_text, tags,
                     tokenize = \"unicode61 remove_diacritics 2\"
                 );
                 PRAGMA user_version = 1;",
            )
            .expect("no la puedo dejar como estaba");
    }

    //  Reopen it with today's worker.
    let db = db::abrir().expect("no abre la vieja");
    let v: i64 = db
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, db::VERSION);
    let filas = search::listar(&db, 10, None, &search::Filtro::default()).expect("no lista");
    assert_eq!(filas.len(), 1, "lo de ayer tiene que seguir estando");
    assert_eq!(
        filas[0].source, "",
        "lo viejo no viene de ningún sitio y se dice así"
    );
    //  The index was rebuilt too: yesterday's content is still searchable,
    //  which would be lost if the FTS table were dropped and not refilled.
    assert_eq!(
        search::buscar(&db, "ayer", 10).unwrap().len(),
        1,
        "el índice no se rehizo tras cambiarle las columnas"
    );
    //  There is a backup too: the plan forbids migrating without one.
    let copias = std::fs::read_dir(paths::backups())
        .map(|d| d.count())
        .unwrap_or(0);
    assert!(copias > 0, "no se guardó copia antes de migrar");

    drop(db);
    let _ = std::fs::remove_dir_all(&dir);
    std::env::remove_var("ETTA_DERIVA_DIR");
}

// ── source ────────────────────────────────────────────────────────
//
//  A YouTube link and a PDF link are not the same, even though both are
//  `text/uri-list`. All of this is derived from the URL, without network access.

fn url(u: &str) -> ingest::Peticion {
    ingest::Peticion {
        tipo: "url".into(),
        source_url: Some(u.into()),
        ..Default::default()
    }
}

//  A video is a video even if the URL does not end in `.mp4`.
#[test]
fn un_enlace_de_youtube_es_un_video() {
    banco(|db| {
        let r = ingest::ingerir(db, &url("https://www.youtube.com/watch?v=dQw4w9WgXcQ"));
        let f = search::una(db, r[0].id.as_ref().unwrap()).unwrap().unwrap();
        assert_eq!(f.tipo, "video");
        assert_eq!(f.source, "youtube");
    });
}

//  The five ways of writing the same video URL are the same video. Without
//  this, dragging it from a phone and desktop saves two captures.
#[test]
fn el_mismo_video_escrito_de_cinco_maneras_es_uno() {
    banco(|db| {
        let iguales = [
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://youtu.be/dQw4w9WgXcQ",
            "https://m.youtube.com/watch?v=dQw4w9WgXcQ&t=90s",
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ&list=PL123",
            "https://www.youtube.com/shorts/dQw4w9WgXcQ",
        ];
        assert_eq!(ingest::ingerir(db, &url(iguales[0]))[0].status, "saved");
        for u in &iguales[1..] {
            assert_eq!(
                ingest::ingerir(db, &url(u))[0].status,
                "duplicate",
                "con {}",
                u
            );
        }
        assert_eq!(search::cuentas(db).unwrap().captures, 1);
    });
}

//  A channel is not a video: without checking the identifier format, a
//  nonexistent thumbnail would be saved.
#[test]
fn un_canal_de_youtube_no_finge_ser_un_video() {
    banco(|db| {
        let r = ingest::ingerir(db, &url("https://www.youtube.com/@alguien"));
        let f = search::una(db, r[0].id.as_ref().unwrap()).unwrap().unwrap();
        assert_eq!(f.source, "youtube");
        //  It is still from YouTube, but without an ID there is no canonical
        //  URL to derive.
        assert_eq!(
            f.canonical_url.as_deref(),
            Some("https://youtube.com/@alguien")
        );
    });
}

#[test]
fn reddit_x_y_github_se_reconocen_y_se_recortan() {
    banco(|db| {
        //  The trailing slug changes and would interfere with duplicate
        //  detection.
        let a = ingest::ingerir(
            db,
            &url("https://www.reddit.com/r/rust/comments/abc123/un_titulo_largo/"),
        );
        let f = search::una(db, a[0].id.as_ref().unwrap()).unwrap().unwrap();
        assert_eq!(f.source, "reddit");
        assert_eq!(
            f.canonical_url.as_deref(),
            Some("https://reddit.com/r/rust/comments/abc123")
        );
        //  The slug also makes a better title than the full address.
        assert_eq!(f.title, "Un titulo largo");
        //  Different slug, same thread: duplicate.
        assert_eq!(
            ingest::ingerir(
                db,
                &url("https://old.reddit.com/r/rust/comments/abc123/otro_slug/")
            )[0]
            .status,
            "duplicate"
        );

        let b = ingest::ingerir(db, &url("https://twitter.com/alguien/status/123456"));
        let f = search::una(db, b[0].id.as_ref().unwrap()).unwrap().unwrap();
        assert_eq!(f.source, "x");
        assert_eq!(
            f.canonical_url.as_deref(),
            Some("https://x.com/alguien/status/123456")
        );
        assert_eq!(f.title, "@alguien");

        let c = ingest::ingerir(db, &url("https://github.com/rust-lang/rust/issues/9"));
        let f = search::una(db, c[0].id.as_ref().unwrap()).unwrap().unwrap();
        assert_eq!(f.source, "github");
        assert_eq!(f.tipo, "code");
        assert_eq!(f.title, "rust-lang/rust");
    });
}

//  A direct link to a PDF is a document, not “a website.”
#[test]
fn la_extension_de_la_url_tambien_dice_que_es() {
    banco(|db| {
        let r = ingest::ingerir(db, &url("https://ejemplo.com/papers/informe-2026.pdf"));
        let f = search::una(db, r[0].id.as_ref().unwrap()).unwrap().unwrap();
        assert_eq!(f.tipo, "document");
        assert_eq!(f.source, "pdf");
        assert_eq!(f.title, "informe-2026.pdf");
    });
}

//  A generic page uses its domain as the title, which says more than
//  “Untitled” and much less than a two-hundred-character URL.
#[test]
fn una_pagina_cualquiera_se_titula_con_su_dominio() {
    banco(|db| {
        let r = ingest::ingerir(db, &url("https://ejemplo.com/una/ruta/larga?a=1&b=2"));
        let f = search::una(db, r[0].id.as_ref().unwrap()).unwrap().unwrap();
        assert_eq!(f.tipo, "url");
        assert_eq!(f.source, "web");
        assert_eq!(f.title, "ejemplo.com");
    });
}

//  What the ingest caller already knows takes precedence over the adapter: if
//  it said “document,” it knows more than we do. The adapter only overrides
//  the generic “url” type.
#[test]
fn el_adaptador_no_pisa_lo_que_ya_se_sabia() {
    banco(|db| {
        let mut p = url("https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        p.tipo = "document".into();
        p.title = Some("Lo llamo yo".into());
        let r = ingest::ingerir(db, &p);
        let f = search::una(db, r[0].id.as_ref().unwrap()).unwrap().unwrap();
        assert_eq!(f.tipo, "document");
        assert_eq!(f.title, "Lo llamo yo");
        //  But the source is still known.
        assert_eq!(f.source, "youtube");
    });
}

// ── your folders ─────────────────────────────────────────────────
//
//  The three built-in folders are not “the folders”; they are examples. What
//  makes this yours is being able to create as many as you want, name them as
//  you like, and remove them without losing what was inside.

fn carpeta(db: &rusqlite::Connection, params: serde_json::Value) -> proto::Respuesta {
    proto::despachar(
        db,
        &proto::Peticion {
            v: 1,
            id: "c".into(),
            method: "space_put".into(),
            params,
        },
    )
}

#[test]
fn una_carpeta_se_hace_antes_de_tener_que_meter() {
    banco(|db| {
        //  It exists while empty: that is the difference between a folder and
        //  a tag that appears on its own. One created when an item is added and
        //  removed when it is taken out is not a place to save things, but a
        //  side effect.
        let r = carpeta(db, serde_json::json!({ "name": "La mudanza" }));
        assert!(r.ok, "{:?}", r.error);
        let secciones = search::secciones(db).unwrap();
        let mia = secciones
            .iter()
            .find(|s| s.name == "La mudanza")
            .expect("no está");
        assert_eq!(mia.id, "la-mudanza", "el id sale del nombre y es un id");
        assert_eq!(mia.count, 0);
    });
}

//  Accents and spaces do not appear in the ID. It travels in page filters and
//  command-line arguments, and an accented ID would need escaping everywhere.
#[test]
fn el_id_de_una_carpeta_es_un_id() {
    for (nombre, id) in [
        ("Cosas de casa", "cosas-de-casa"),
        ("Investigación", "investigacion"),
        ("  Año  nuevo  ", "ano-nuevo"),
        ("¿Qué leo?", "que-leo"),
        ("C++", "c"),
    ] {
        assert_eq!(ingest::id_de_espacio(nombre), id, "con «{}»", nombre);
    }
}

//  Renaming is renaming, not creating another folder beside it.
#[test]
fn renombrar_una_carpeta_no_hace_otra() {
    banco(|db| {
        carpeta(db, serde_json::json!({ "name": "Cosas" }));
        let id = ingest::ingerir(db, &texto("algo dentro"))[0]
            .id
            .clone()
            .unwrap();
        proto::despachar(
            db,
            &proto::Peticion {
                v: 1,
                id: "a".into(),
                method: "annotate".into(),
                params: serde_json::json!({ "id": id, "space": "cosas" }),
            },
        );

        let r = carpeta(
            db,
            serde_json::json!({ "id": "cosas", "name": "Cosas de casa" }),
        );
        assert!(r.ok, "{:?}", r.error);

        let secciones = search::secciones(db).unwrap();
        let carpetas: Vec<_> = secciones.iter().filter(|s| s.kind == "space").collect();
        assert_eq!(carpetas.iter().filter(|s| s.id == "cosas").count(), 1);
        let mia = carpetas.iter().find(|s| s.id == "cosas").unwrap();
        assert_eq!(mia.name, "Cosas de casa");
        //  Its contents remain inside: the ID did not change.
        assert_eq!(mia.count, 1);
    });
}

//  Removing the folder does not burn the books.
#[test]
fn quitar_una_carpeta_no_borra_lo_que_habia() {
    banco(|db| {
        let id = ingest::ingerir(db, &texto("un apunte"))[0]
            .id
            .clone()
            .unwrap();
        proto::despachar(
            db,
            &proto::Peticion {
                v: 1,
                id: "a".into(),
                method: "annotate".into(),
                params: serde_json::json!({ "id": id, "space": "La mudanza" }),
            },
        );
        assert_eq!(
            search::listar(
                db,
                10,
                None,
                &search::Filtro {
                    space: Some("la-mudanza".into()),
                    ..Default::default()
                }
            )
            .unwrap()
            .len(),
            1
        );

        let r = proto::despachar(
            db,
            &proto::Peticion {
                v: 1,
                id: "d".into(),
                method: "space_delete".into(),
                params: serde_json::json!({ "id": "la-mudanza" }),
            },
        );
        assert!(r.ok, "{:?}", r.error);

        //  The folder is gone…
        assert!(!search::secciones(db)
            .unwrap()
            .iter()
            .any(|s| s.id == "la-mudanza"));
        //  …and its contents remain in the library.
        assert_eq!(search::cuentas(db).unwrap().captures, 1);
        assert!(search::una(db, &id).unwrap().is_some());
    });
}

//  Moving something into an existing folder does not change its name. That
//  would be a bug if the caller sent the ID as though it were the name.
#[test]
fn mover_por_id_respeta_el_nombre_de_la_carpeta() {
    banco(|db| {
        let id = ingest::ingerir(db, &texto("a leer"))[0].id.clone().unwrap();
        proto::despachar(
            db,
            &proto::Peticion {
                v: 1,
                id: "a".into(),
                method: "annotate".into(),
                params: serde_json::json!({ "id": id, "space": "leer-luego" }),
            },
        );
        let s = search::secciones(db).unwrap();
        let leer = s.iter().find(|s| s.id == "leer-luego").unwrap();
        assert_eq!(leer.name, "Read later", "the ID was used as the name");
        assert_eq!(leer.count, 1);
    });
}

// ── external metadata ─────────────────────────────────────────────
//
//  The worker does not access the network and never will. The host-provided
//  data comes through `enrich`; the only rule tested here is what that method
//  may and may not overwrite.

fn enriquecer(db: &rusqlite::Connection, id: &str, params: serde_json::Value) -> proto::Respuesta {
    let mut v = params;
    v["id"] = serde_json::json!(id);
    proto::despachar(
        db,
        &proto::Peticion {
            v: 1,
            id: "e".into(),
            method: "enrich".into(),
            params: v,
        },
    )
}

//  A saved link uses its domain as the title. That is a fallback, and the
//  page's title can replace it.
#[test]
fn la_portada_pisa_el_titulo_de_apano() {
    banco(|db| {
        let r = ingest::ingerir(db, &url("https://www.youtube.com/watch?v=dQw4w9WgXcQ"));
        let id = r[0].id.clone().unwrap();
        //  Its title is not known yet.
        assert_eq!(
            search::una(db, &id).unwrap().unwrap().title,
            "https://youtube.com/watch?v=dQw4w9WgXcQ"
        );

        let resp = enriquecer(
            db,
            &id,
            serde_json::json!({
                "title": "Rick Astley - Never Gonna Give You Up",
                "author": "Rick Astley",
                "excerpt": "El vídeo oficial.",
                "preview_b64": util::a_base64(b"unos bytes de imagen"),
            }),
        );
        assert!(resp.ok, "{:?}", resp.error);

        let f = search::una(db, &id).unwrap().unwrap();
        assert_eq!(f.title, "Rick Astley - Never Gonna Give You Up");
        assert_eq!(f.author, "Rick Astley");
        assert_eq!(f.excerpt, "El vídeo oficial.");
        //  The thumbnail path is already resolved: the renderer gets a path,
        //  not a hash.
        let foto = f.preview_path.expect("tendría que haber miniatura");
        assert!(
            std::path::Path::new(&foto).exists(),
            "la miniatura no está en el disco"
        );
        //  Search by the new title to verify that the index was rebuilt.
        assert_eq!(search::buscar(db, "astley", 10).unwrap().len(), 1);
    });
}

//  The saved item must provide the URL from which to fetch its thumbnail.
//  Otherwise, the caller that just saved it has the ID but does not know where
//  to go.
#[test]
fn lo_guardado_dice_de_que_direccion_era() {
    banco(|db| {
        let r = ingest::ingerir(db, &url("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=9"));
        //  Use the canonical URL, not the source URL: that is the one to visit.
        assert_eq!(
            r[0].url.as_deref(),
            Some("https://youtube.com/watch?v=dQw4w9WgXcQ")
        );
        //  Plain text has no URL, which must be representable too.
        assert_eq!(ingest::ingerir(db, &texto("sin dirección"))[0].url, None);
    });
}

//  The page body is saved AND SEARCHABLE.
//
//  This distinguishes a library that knows titles from one that knows what is
//  inside them: without it, neither you nor the AI can find a phrase from a
//  saved article.
#[test]
fn el_cuerpo_de_lo_guardado_se_indexa() {
    banco(|db| {
        let id = ingest::ingerir(db, &url("https://ejemplo.com/articulo"))[0]
            .id
            .clone()
            .unwrap();
        //  There is nothing to search inside yet.
        assert_eq!(search::buscar(db, "cerámica", 10).unwrap().len(), 0);

        assert!(
            enriquecer(
                db,
                &id,
                serde_json::json!({
                    "title": "Cómo cocer barro",
                    "text": "La cerámica de baja temperatura pide un esmalte distinto.",
                })
            )
            .ok
        );

        //  Match a word that appears only in the body.
        assert_eq!(search::buscar(db, "esmalte", 10).unwrap().len(), 1);
        //  The excerpt was filled with the beginning, which is what the card
        //  displays and what the model sees.
        let f = search::una(db, &id).unwrap().unwrap();
        assert!(f.excerpt.starts_with("La cerámica"), "{:?}", f.excerpt);
    });
}

//  Do not reread it: a capture whose text changes on its own stops being a
//  copy of what you saw. Reading the page again in a month would yield
//  something else.
#[test]
fn el_cuerpo_guardado_no_se_reescribe() {
    banco(|db| {
        let id = ingest::ingerir(db, &url("https://ejemplo.com/x"))[0]
            .id
            .clone()
            .unwrap();
        assert!(
            enriquecer(
                db,
                &id,
                serde_json::json!({ "text": "lo que decía entonces" })
            )
            .ok
        );
        assert!(enriquecer(db, &id, serde_json::json!({ "text": "lo que dice hoy" })).ok);
        assert_eq!(search::buscar(db, "entonces", 10).unwrap().len(), 1);
        assert_eq!(search::buscar(db, "hoy", 10).unwrap().len(), 0);
    });
}

//  Do not overwrite text provided with the drag either: that is what you
//  selected.
#[test]
fn el_texto_que_trajiste_manda() {
    banco(|db| {
        let id = ingest::ingerir(db, &texto("esto lo seleccioné yo"))[0]
            .id
            .clone()
            .unwrap();
        assert!(enriquecer(db, &id, serde_json::json!({ "text": "otra cosa distinta" })).ok);
        assert_eq!(search::buscar(db, "seleccioné", 10).unwrap().len(), 1);
        assert_eq!(search::buscar(db, "distinta", 10).unwrap().len(), 0);
    });
}

//  Do NOT overwrite the title provided with the drag. A browser sends the tab
//  title when dropped; the same site provided it, so it is equally valid.
#[test]
fn la_portada_no_pisa_el_titulo_que_traia_el_arrastre() {
    banco(|db| {
        let mut p = url("https://ejemplo.com/articulo");
        p.title = Some("Como lo llamé yo".into());
        let id = ingest::ingerir(db, &p)[0].id.clone().unwrap();

        let resp = enriquecer(
            db,
            &id,
            serde_json::json!({
                "title": "El título de la página", "author": "Alguien",
            }),
        );
        assert!(resp.ok, "{:?}", resp.error);
        let f = search::una(db, &id).unwrap().unwrap();
        assert_eq!(f.title, "Como lo llamé yo");
        //  But the author is set if it was absent; nothing is overwritten.
        assert_eq!(f.author, "Alguien");
    });
}

//  Your data is never touched, and enriching twice does not change the first
//  result.
#[test]
fn enriquecer_no_toca_lo_tuyo_ni_se_repite() {
    banco(|db| {
        let id = ingest::ingerir(db, &url("https://ejemplo.com/x"))[0]
            .id
            .clone()
            .unwrap();
        proto::despachar(
            db,
            &proto::Peticion {
                v: 1,
                id: "a".into(),
                method: "annotate".into(),
                params: serde_json::json!({ "id": id, "note": "mío", "tags": ["azul"] }),
            },
        );

        assert!(
            enriquecer(
                db,
                &id,
                serde_json::json!({
                    "title": "La primera", "author": "Uno",
                    "preview_b64": util::a_base64(b"la primera foto"),
                })
            )
            .ok
        );
        let primera = search::una(db, &id).unwrap().unwrap();

        assert!(
            enriquecer(
                db,
                &id,
                serde_json::json!({
                    "title": "La segunda", "author": "Otro",
                    "preview_b64": util::a_base64(b"otra foto distinta"),
                })
            )
            .ok
        );
        let f = search::una(db, &id).unwrap().unwrap();

        assert_eq!(f.note, "mío");
        assert_eq!(f.tags, vec!["azul".to_string()]);
        //  The second call overwrites nothing: the title is no longer a
        //  fallback, and the author and thumbnail are already present.
        assert_eq!(f.title, "La primera");
        assert_eq!(f.author, "Uno");
        assert_eq!(f.preview_path, primera.preview_path);
    });
}

//  A saved file appears in the pending list with ITS PATH, so it can be opened
//  and its text extracted.
//
//  Without this, a forty-page PDF remains a row called “report.pdf” forever:
//  it cannot be searched by its contents, and the AI sees the filename again.
#[test]
fn un_fichero_sin_texto_sale_con_su_ruta() {
    banco(|db| {
        let f = std::env::temp_dir().join(format!("deriva-{}.pdf", util::ahora_ms()));
        std::fs::write(&f, b"%PDF-1.4 lo que sea").unwrap();

        let p = ingest::Peticion {
            tipo: "document".into(),
            paths: vec![f.to_string_lossy().to_string()],
            ..Default::default()
        };
        let id = ingest::ingerir(db, &p)[0].id.clone().unwrap();

        let pendientes = search::por_enriquecer(db, 10).unwrap();
        let mio = pendientes
            .iter()
            .find(|x| x.id == id)
            .expect("no sale el fichero");
        let ruta = mio.path.as_deref().expect("sale sin ruta que abrir");
        assert!(
            std::path::Path::new(ruta).exists(),
            "la ruta no lleva a ningún sitio"
        );

        //  Once it has been read, it no longer appears for that reason.
        assert!(
            enriquecer(
                db,
                &id,
                serde_json::json!({
                    "text": "lo que ponía el pdf",
                })
            )
            .ok
        );
        let pendientes = search::por_enriquecer(db, 10).unwrap();
        assert!(
            pendientes.iter().all(|x| x.path.is_none() || x.id != id),
            "vuelve a pedir que se abra un fichero que ya se leyó"
        );
        //  Its contents are searchable.
        assert_eq!(search::buscar(db, "ponía", 10).unwrap().len(), 1);
    });
}

//  An older capture learns where it came from.
//
//  There is no need to ask anyone; the source is in its URL. What was missing
//  was a reason to inspect it, and that moment comes when someone revisits it.
#[test]
fn al_enriquecer_tambien_se_aprende_de_donde_venia() {
    banco(|db| {
        let id = ingest::ingerir(db, &url("https://www.youtube.com/watch?v=dQw4w9WgXcQ"))[0]
            .id
            .clone()
            .unwrap();
        //  Remove the source, as if it had been saved by last week's worker.
        db.execute(
            "UPDATE captures SET source = '', type = 'url' WHERE id = ?1",
            [&id],
        )
        .unwrap();

        assert!(enriquecer(db, &id, serde_json::json!({ "author": "Alguien" })).ok);
        let f = search::una(db, &id).unwrap().unwrap();
        assert_eq!(f.source, "youtube");
        assert_eq!(f.tipo, "video", "un enlace de YouTube es un vídeo");
    });
}

//  Do not change the type when it was already specified.
#[test]
fn aprender_de_donde_venia_no_pisa_el_tipo() {
    banco(|db| {
        let mut p = url("https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        p.tipo = "document".into();
        let id = ingest::ingerir(db, &p)[0].id.clone().unwrap();
        db.execute("UPDATE captures SET source = '' WHERE id = ?1", [&id])
            .unwrap();

        assert!(enriquecer(db, &id, serde_json::json!({ "author": "Alguien" })).ok);
        let f = search::una(db, &id).unwrap().unwrap();
        assert_eq!(f.source, "youtube");
        assert_eq!(f.tipo, "document");
    });
}

//  Items still missing a thumbnail can be retried.
//
//  Without this, items saved before enrichment existed —or when there was no
//  network—keep their raw URL as the title forever, exactly the problem
//  enrichment was meant to solve.
#[test]
fn se_sabe_lo_que_sigue_sin_portada() {
    banco(|db| {
        let a = ingest::ingerir(db, &url("https://ejemplo.com/uno"))[0]
            .id
            .clone()
            .unwrap();
        let b = ingest::ingerir(db, &url("https://ejemplo.com/dos"))[0]
            .id
            .clone()
            .unwrap();
        //  Plain text has no page to visit.
        ingest::ingerir(db, &texto("esto no sale a ningún sitio"));

        //  Both are still missing it.
        let pendientes = search::por_enriquecer(db, 10).unwrap();
        assert_eq!(pendientes.len(), 2);
        //  Include the URL needed to fetch it.
        assert!(pendientes
            .iter()
            .any(|p| p.id == a && p.url.as_deref().is_some_and(|u| u.starts_with("http"))));

        //  Enrich one with everything: actual title, thumbnail, and contents.
        //  All three are needed because a partially enriched item is still
        //  incomplete.
        assert!(
            enriquecer(
                db,
                &a,
                serde_json::json!({
                    "title": "Un título de verdad",
                    "preview_b64": util::a_base64(b"una foto"),
                    "text": "el cuerpo del artículo, entero",
                })
            )
            .ok
        );
        let pendientes = search::por_enriquecer(db, 10).unwrap();
        assert_eq!(
            pendientes.len(),
            1,
            "la enriquecida no puede seguir pendiente"
        );
        assert_eq!(pendientes[0].id, b);

        //  A thumbnail does not make up for a fallback title: these are
        //  separate ways for an item to be incomplete.
        assert!(
            enriquecer(
                db,
                &b,
                serde_json::json!({
                    "preview_b64": util::a_base64(b"otra foto"),
                    "text": "algo de texto",
                })
            )
            .ok
        );
        assert_eq!(search::por_enriquecer(db, 10).unwrap().len(), 1);

        //  Do not fetch metadata for trashed items.
        proto::despachar(
            db,
            &proto::Peticion {
                v: 1,
                id: "t".into(),
                method: "trash".into(),
                params: serde_json::json!({ "id": b }),
            },
        );
        assert_eq!(search::por_enriquecer(db, 10).unwrap().len(), 0);
    });
}

//  An unreadable thumbnail does not leave a capture partially updated: reject
//  the whole update and leave the existing data unchanged.
#[test]
fn una_miniatura_ilegible_no_escribe_nada() {
    banco(|db| {
        let id = ingest::ingerir(db, &url("https://ejemplo.com/y"))[0]
            .id
            .clone()
            .unwrap();
        let resp = enriquecer(
            db,
            &id,
            serde_json::json!({
                "title": "No debería entrar", "preview_b64": "esto no es base64 ~~~",
            }),
        );
        assert!(!resp.ok);
        let f = search::una(db, &id).unwrap().unwrap();
        assert_eq!(
            f.title, "ejemplo.com",
            "se escribió el título pese al fallo"
        );
        assert!(f.preview_path.is_none());
    });
}

//  Try all words first, and any word if that finds nothing.
//
//  The implicit AND is correct as long as it finds something, and the worst
//  possible behavior when it does not: with four words, just one missing word
//  makes the answer “nothing saved looks like that,” which is false. This
//  fails just when there is the most content, because the larger the library,
//  the longer the query.
#[test]
fn si_con_todas_las_palabras_no_sale_nada_vale_con_una() {
    banco(|db| {
        let mut p = texto("Fondos de pantalla animados para Wayland");
        p.title = Some("live-paper".into());
        ingest::ingerir(db, &p);
        ingest::ingerir(db, &texto("Una receta de bizcocho"));

        //  All words: a match.
        assert_eq!(search::buscar(db, "fondos pantalla", 10).unwrap().len(), 1);
        //  One missing word makes AND find nothing, even though there is
        //  something quite similar.
        let r = search::buscar(db, "fondos de pantalla para escritorio", 10).unwrap();
        assert_eq!(r.len(), 1, "la respuesta era «no hay nada» y sí lo había");
        assert_eq!(r[0].title, "live-paper");

        //  Unrelated items still do not appear: fallback search is here to
        //  avoid false negatives, not to return anything at all.
        assert_eq!(search::buscar(db, "termodinámica", 10).unwrap().len(), 0);
    });
}

//  A complete question is not treated as a collection of unrelated words.
//
//  This is what arrives from chat: the AI passes your words as-is, and in
//  Spanish that often sounds like “do I have anything saved about…?” Without
//  removing filler words, fallback search returns half the library because
//  of “have” and “anything”; a result containing everything is as useless as
//  an empty one.
#[test]
fn una_pregunta_entera_no_arrastra_media_biblioteca() {
    banco(|db| {
        let mut p = texto("Fondos de pantalla animados para Wayland");
        p.title = Some("live-paper".into());
        ingest::ingerir(db, &p);
        let mut p = texto("Tengo que acordarme de algo sobre el bizcocho");
        p.title = Some("receta".into());
        ingest::ingerir(db, &p);

        let r = search::buscar(db, "¿tengo algo guardado sobre fondos de pantalla?", 10).unwrap();
        assert_eq!(r.len(), 1, "el relleno arrastró lo que no era");
        assert_eq!(r[0].title, "live-paper");
    });
}

//  A single word needs no fallback; it would be the same query.
#[test]
fn con_una_palabra_no_hay_segunda_vuelta() {
    assert_eq!(search::consulta_fts_floja("wallpapers"), None);
    //  Short and filler words are excluded from fallback: “of” appears in
    //  everything you save, as do “have” and “about.”
    assert_eq!(
        search::consulta_fts_floja("fondos de pantalla").unwrap(),
        "\"fondos\"* OR \"pantalla\"*"
    );
    //  Preserve the words in the query as-is —the index already removes
    //  accents. Only the comparison against the list is normalized.
    assert_eq!(
        search::consulta_fts_floja("tengo algo sobre cerámica antigua").unwrap(),
        "\"cerámica\"* OR \"antigua\"*"
    );
    assert_eq!(
        search::consulta_fts_floja("tenía algún apunte de cerámica").unwrap(),
        "\"apunte\"* OR \"cerámica\"*"
    );
    //  Unless nothing else remains: more is better than nothing.
    assert_eq!(
        search::consulta_fts_floja("de la").unwrap(),
        "\"de\"* OR \"la\"*"
    );
    assert_eq!(
        search::consulta_fts_floja("tengo algo").unwrap(),
        "\"tengo\"* OR \"algo\"*"
    );
}

// ── semantic similarity ──────────────────────────────────────────
//
//  The model is 506 MB and optional, so these tests run WITHOUT it. They verify
//  that the library works exactly as before without a model, and that pieces
//  that do not need one —storing a vector, comparing it, and choosing the text
//  used to represent a capture—do what they say.
//
//  Similarity quality cannot be tested here: it depends on the model, and a
//  hard-coded score would make the test fail the day the model is replaced by
//  a better one.

#[test]
fn sin_modelo_la_biblioteca_busca_igual() {
    banco(|db| {
        let mut p = texto("Fondos de pantalla animados para Wayland");
        p.title = Some("live-paper".into());
        ingest::ingerir(db, &p);

        //  There is no model in the test fixture: no vectors and no errors.
        assert!(
            !vectores::listo(),
            "el banco no puede depender de un modelo"
        );
        assert_eq!(search::parecidas(db, "fondos", 5).unwrap().len(), 0);
        let (hechas, faltan) = search::vectorizar_pendientes(db, 10).unwrap();
        assert_eq!((hechas, faltan), (0, 0));

        //  Regular search remains unchanged.
        assert_eq!(search::buscar(db, "wayland", 5).unwrap().len(), 1);
    });
}

//  Without enough material, there is no vector; this is not an optimization.
//
//  A Reddit capture had only six characters of content —that page returns no
//  text unless JavaScript runs—and its vector, made from the word “Reddit,”
//  landed in a generic area: 0.26 similar to “baking” and 0.24 similar to
//  “video editing.” It appeared in EVERY search.
//
//  A one-word vector summarizes nothing; it is noise shaped like a result.
#[test]
fn una_captura_sin_apenas_texto_no_lleva_vector() {
    banco(|db| {
        let mut flaca = ingest::Peticion {
            tipo: "url".into(),
            source_url: Some("https://ejemplo.com/vacia".into()),
            ..Default::default()
        };
        flaca.title = Some("Web".into());
        let flaco = ingest::ingerir(db, &flaca)[0].id.clone().unwrap();
        let gordo = ingest::ingerir(
            db,
            &texto("Fondos de pantalla animados para Wayland, escritos en Rust"),
        )[0]
        .id
        .clone()
        .unwrap();

        //  Without a model, there is nothing to test about vectors, but the
        //  filtering still works: the same query chooses which items are due.
        let (_, _) = search::vectorizar_pendientes(db, 10).unwrap();
        //  Apply the rule directly to the text that would represent each item.
        let de = |id: &str| {
            let f = search::una(db, id).unwrap().unwrap();
            vectores::texto_de(&f.title, &f.summary, "", &f.excerpt, "")
        };
        assert!(
            de(&flaco).chars().count() < search::MATERIAL_MINIMO,
            "«{}» daría para un vector",
            de(&flaco)
        );
        assert!(de(&gordo).chars().count() >= search::MATERIAL_MINIMO);
    });
}

//  A vector is written to and read from disk unchanged. It is stored raw —
//  little-endian `f32`—not as JSON: a vector is a list of numbers, and storing
//  it as text makes it four times larger and requires parsing on every comparison.
#[test]
fn un_vector_va_y_vuelve_del_disco() {
    let v = vec![0.5f32, -0.25, 0.0, 1.0];
    let b = vectores::a_bytes(&v);
    assert_eq!(b.len(), 16);
    assert_eq!(vectores::de_bytes(&b), v);
}

//  Two vectors of different dimensions are NOT compared up to the shorter
//  length: that would not produce an error, but a made-up similarity, which is
//  worse.
#[test]
fn dos_vectores_de_modelos_distintos_no_se_parecen() {
    let a = vec![1.0f32, 0.0];
    assert_eq!(vectores::parecido(&a, &[1.0, 0.0]), 1.0);
    assert!(vectores::parecido(&a, &[0.0, 1.0]).abs() < 0.001);
    assert_eq!(
        vectores::parecido(&a, &[1.0, 0.0, 0.0]),
        -1.0,
        "medidas distintas"
    );
    assert_eq!(vectores::parecido(&[], &[]), -1.0);
}

//  What represents a capture: what describes it, not everything it says. A
//  vector is an average, and the average of forty pages resembles a little bit
//  of everything and a lot of nothing.
#[test]
fn una_captura_se_representa_por_lo_que_la_describe() {
    let t = vectores::texto_de(
        "Cómo cocer barro",
        "Sobre cerámica de baja temperatura.",
        "ceramica, manualidades",
        "El horno...",
        "",
    );
    assert!(t.starts_with("Cómo cocer barro"));
    assert!(t.contains("cerámica"));
    assert!(t.contains("ceramica, manualidades"));

    //  The body is used only when the rest does not fill the limit: if there
    //  is already a title and summary, the whole article gets in the way.
    let corto = vectores::texto_de("Título", "", "", "", &"palabra ".repeat(500));
    assert!(corto.chars().count() <= 1200);
    assert!(corto.contains("palabra"));
    let lleno = vectores::texto_de(
        "Título",
        &"resumen largo ".repeat(40),
        "",
        "",
        "ESTE CUERPO NO DEBERIA ENTRAR",
    );
    assert!(!lleno.contains("NO DEBERIA"));
}

// ── what the AI understood ───────────────────────────────────────
//
//  There is no model here: the summary arrives already written, just like
//  page metadata arrives already fetched. The only rule tested is the one
//  that matters in `classify`, which is stricter than `enrich` because a page
//  title is a fact and this is an opinion.

fn clasificar(db: &rusqlite::Connection, id: &str, params: serde_json::Value) -> proto::Respuesta {
    let mut v = params;
    v["id"] = serde_json::json!(id);
    proto::despachar(
        db,
        &proto::Peticion {
            v: 1,
            id: "k".into(),
            method: "classify".into(),
            params: v,
        },
    )
}

//  What it understands is saved and SEARCHABLE: a summary missing from the
//  index cannot be found by anyone, you or the AI, and is therefore useless.
#[test]
fn el_resumen_se_guarda_y_se_busca() {
    banco(|db| {
        let id = ingest::ingerir(db, &texto("un texto larguísimo sobre cerámica"))[0]
            .id
            .clone()
            .unwrap();
        let r = clasificar(
            db,
            &id,
            serde_json::json!({
                "summary": "Explica cómo se cuece el barro a baja temperatura.",
                "tags": ["ceramica", "manualidades"],
            }),
        );
        assert!(r.ok, "{:?}", r.error);

        let f = search::una(db, &id).unwrap().unwrap();
        assert_eq!(
            f.summary,
            "Explica cómo se cuece el barro a baja temperatura."
        );
        assert_eq!(
            f.tags,
            vec!["ceramica".to_string(), "manualidades".to_string()]
        );
        //  Match a word that appears ONLY in the summary.
        assert_eq!(search::buscar(db, "cuece", 10).unwrap().len(), 1);
        assert_eq!(search::buscar(db, "barro", 10).unwrap().len(), 1);
    });
}

//  Tags are ADDED. `annotate` replaces them because you are the one writing
//  there; deleting yours to add the model's would let the AI organize your
//  closet to its liking.
#[test]
fn la_ia_suma_etiquetas_y_no_borra_las_tuyas() {
    banco(|db| {
        let id = ingest::ingerir(db, &texto("algo"))[0].id.clone().unwrap();
        proto::despachar(
            db,
            &proto::Peticion {
                v: 1,
                id: "a".into(),
                method: "annotate".into(),
                params: serde_json::json!({ "id": id, "tags": ["mías"], "note": "para el jueves" }),
            },
        );

        assert!(
            clasificar(
                db,
                &id,
                serde_json::json!({
                    "summary": "Un resumen.", "tags": ["suya"],
                })
            )
            .ok
        );

        let f = search::una(db, &id).unwrap().unwrap();
        assert_eq!(f.tags, vec!["mías".to_string(), "suya".to_string()]);
        assert_eq!(
            f.note, "para el jueves",
            "la nota es tuya y no la toca nadie"
        );
    });
}

//  Do not rewrite an existing summary: what you read yesterday should not say
//  something else today unless you changed it.
#[test]
fn el_resumen_no_se_reescribe() {
    banco(|db| {
        let id = ingest::ingerir(db, &texto("algo"))[0].id.clone().unwrap();
        assert!(clasificar(db, &id, serde_json::json!({ "summary": "El primero." })).ok);
        assert!(clasificar(db, &id, serde_json::json!({ "summary": "Otro distinto." })).ok);
        assert_eq!(
            search::una(db, &id).unwrap().unwrap().summary,
            "El primero."
        );
    });
}

//  Set the folder only if the capture was unfiled. Moving something for you is
//  the kind of help that makes you lose track of your things.
#[test]
fn la_ia_no_te_mueve_lo_que_ya_habias_colocado() {
    banco(|db| {
        let suelta = ingest::ingerir(db, &texto("sin sitio"))[0]
            .id
            .clone()
            .unwrap();
        let colocada = ingest::ingerir(db, &texto("con sitio"))[0]
            .id
            .clone()
            .unwrap();
        proto::despachar(
            db,
            &proto::Peticion {
                v: 1,
                id: "a".into(),
                method: "annotate".into(),
                params: serde_json::json!({ "id": colocada, "space": "proyectos" }),
            },
        );

        let r = clasificar(db, &suelta, serde_json::json!({ "space": "inspiracion" }));
        assert_eq!(r.result.as_ref().unwrap()["moved"], serde_json::json!(true));
        let r = clasificar(db, &colocada, serde_json::json!({ "space": "inspiracion" }));
        assert_eq!(
            r.result.as_ref().unwrap()["moved"],
            serde_json::json!(false)
        );

        assert_eq!(
            search::listar(
                db,
                10,
                None,
                &search::Filtro {
                    space: Some("proyectos".into()),
                    ..Default::default()
                }
            )
            .unwrap()
            .len(),
            1,
            "le cambiaron la carpeta"
        );
    });
}

//  Do NOT ask again about items that cannot be summarized.
//
//  This bug was observed in production: a Reddit page that returns no text
//  unless JavaScript runs stayed pending forever, and the model was asked
//  about it every twenty seconds —rightly saying nothing—and charging each
//  time. “Nothing could be said” is a response and must be remembered.
#[test]
fn lo_que_no_se_puede_resumir_no_se_pregunta_para_siempre() {
    banco(|db| {
        let id = ingest::ingerir(db, &url("https://ejemplo.com/vacia"))[0]
            .id
            .clone()
            .unwrap();
        assert_eq!(search::por_clasificar(db, 10).unwrap().len(), 1);

        //  Ask the model and it has nothing to say: `classify` with no fields
        //  is exactly that.
        assert!(clasificar(db, &id, serde_json::json!({})).ok);

        assert_eq!(
            search::por_clasificar(db, 10).unwrap().len(),
            0,
            "se volvería a preguntar lo mismo cada vez"
        );
        //  The count shown to the user decreases too; otherwise, the page would
        //  say “1 left to summarize” forever.
        assert_eq!(search::cuentas(db).unwrap().pending_summary, 0);
    });
}

//  But if the material changes, process it again. It was processed with only a
//  title, and now there is a whole article: that is different material.
#[test]
fn si_llega_el_texto_se_vuelve_a_mirar() {
    banco(|db| {
        let id = ingest::ingerir(db, &url("https://ejemplo.com/tarde"))[0]
            .id
            .clone()
            .unwrap();
        assert!(clasificar(db, &id, serde_json::json!({})).ok);
        assert_eq!(search::por_clasificar(db, 10).unwrap().len(), 0);

        //  The page body has arrived.
        assert!(
            enriquecer(
                db,
                &id,
                serde_json::json!({
                    "text": "un artículo entero sobre cerámica",
                })
            )
            .ok
        );
        assert_eq!(
            search::por_clasificar(db, 10).unwrap().len(),
            1,
            "con material nuevo hay que volver a mirarlo"
        );
    });
}

//  The pending list includes only what is needed to summarize each item: its
//  title, source, and a snippet. NOT the entire content.
#[test]
fn lo_que_falta_por_resumir_sale_recortado() {
    banco(|db| {
        let largo = "palabra ".repeat(400);
        let id = ingest::ingerir(db, &texto(&largo))[0].id.clone().unwrap();
        let faltan = search::por_clasificar(db, 10).unwrap();
        assert_eq!(faltan.len(), 1);
        assert_eq!(faltan[0].id, id);
        assert!(
            faltan[0].excerpt.chars().count() <= 400,
            "sale más texto del equipo del necesario"
        );

        assert!(clasificar(db, &id, serde_json::json!({ "summary": "Ya está." })).ok);
        assert_eq!(search::por_clasificar(db, 10).unwrap().len(), 0);
    });
}

//  The tags you already use, so they can be displayed and the AI can reuse
//  them instead of inventing new ones.
#[test]
fn se_pueden_listar_las_etiquetas_que_usas() {
    banco(|db| {
        let a = ingest::ingerir(db, &texto("uno"))[0].id.clone().unwrap();
        let b = ingest::ingerir(db, &texto("dos"))[0].id.clone().unwrap();
        for (id, ts) in [(&a, vec!["cocina", "recetas"]), (&b, vec!["cocina"])] {
            proto::despachar(
                db,
                &proto::Peticion {
                    v: 1,
                    id: "a".into(),
                    method: "annotate".into(),
                    params: serde_json::json!({ "id": id, "tags": ts }),
                },
            );
        }
        let r = proto::despachar(
            db,
            &proto::Peticion {
                v: 1,
                id: "t".into(),
                method: "tags".into(),
                params: serde_json::json!({}),
            },
        );
        assert!(r.ok, "{:?}", r.error);
        let v = r.result.unwrap();
        let lista = v["tags"].as_array().unwrap();
        //  Most-used first: it is most likely to be useful again, and the list
        //  sent to a model is truncated.
        assert_eq!(lista[0]["name"], serde_json::json!("cocina"));
        assert_eq!(lista[0]["count"], serde_json::json!(2));
        assert_eq!(lista.len(), 2);

        //  Do not include an unused tag: that would offer the model an empty
        //  drawer as though it were an active category.
        proto::despachar(
            db,
            &proto::Peticion {
                v: 1,
                id: "a".into(),
                method: "annotate".into(),
                params: serde_json::json!({ "id": a, "tags": ["cocina"] }),
            },
        );
        let r = proto::despachar(
            db,
            &proto::Peticion {
                v: 1,
                id: "t".into(),
                method: "tags".into(),
                params: serde_json::json!({}),
            },
        );
        assert_eq!(r.result.unwrap()["tags"].as_array().unwrap().len(), 1);
    });
}

// ── resurfacing ───────────────────────────────────────────────────
//
//  The engine is deterministic, which is what this tests: the same library
//  returns the same result, and the displayed reason is literally the rule
//  that matched.

fn aplazar(db: &rusqlite::Connection, id: &str, cuando: i64) {
    db.execute(
        "UPDATE captures SET snoozed_until = ?2 WHERE id = ?1",
        rusqlite::params![id, cuando],
    )
    .unwrap();
}

//  Check the plan's priority order from top to bottom: what you asked for
//  takes precedence over everything else.
#[test]
fn lo_que_aplazaste_vuelve_antes_que_nada() {
    banco(|db| {
        //  A forgotten favorite, the third priority.
        let viejo = ingest::ingerir(db, &texto("un favorito de hace mucho"))[0]
            .id
            .clone()
            .unwrap();
        db.execute(
            "UPDATE captures SET favorite = 1, captured_at = ?2 WHERE id = ?1",
            rusqlite::params![viejo, util::ahora_ms() - 60 * 86_400_000i64],
        )
        .unwrap();

        //  An item snoozed until yesterday, the first priority.
        let pedido = ingest::ingerir(db, &texto("esto lo aplacé yo"))[0]
            .id
            .clone()
            .unwrap();
        aplazar(db, &pedido, util::ahora_ms() - 86_400_000);

        let v = reaparicion::siguiente(db, None).unwrap().unwrap();
        assert_eq!(v.item.id, pedido);
        assert_eq!(v.reason, "scheduled");
        //  The reason can be read: if it cannot be named, the item is not
        //  eligible.
        assert!(!v.why.is_empty());
    });
}

//  An item snoozed until next month is not eligible: snoozing means “not now,”
//  and returning it anyway would mean not listening.
#[test]
fn lo_aplazado_para_luego_no_vuelve_todavia() {
    banco(|db| {
        let id = ingest::ingerir(db, &texto("para el mes que viene"))[0]
            .id
            .clone()
            .unwrap();
        aplazar(db, &id, util::ahora_ms() + 30 * 86_400_000);
        assert!(reaparicion::siguiente(db, None).unwrap().is_none());
    });
}

//  Neither is something you just opened: you have just seen it.
#[test]
fn lo_recien_abierto_no_vuelve() {
    banco(|db| {
        let id = ingest::ingerir(db, &texto("lo miré ayer"))[0]
            .id
            .clone()
            .unwrap();
        db.execute(
            "UPDATE captures SET last_opened_at = ?2 WHERE id = ?1",
            rusqlite::params![id, util::ahora_ms() - 86_400_000i64],
        )
        .unwrap();
        assert!(reaparicion::siguiente(db, None).unwrap().is_none());

        //  But after a week, it is eligible.
        db.execute(
            "UPDATE captures SET last_opened_at = ?2 WHERE id = ?1",
            rusqlite::params![id, util::ahora_ms() - 30 * 86_400_000i64],
        )
        .unwrap();
        assert!(reaparicion::siguiente(db, None).unwrap().is_some());
    });
}

//  After being shown three times with no action, it goes quiet. This keeps
//  the same card from following you around all week.
#[test]
fn lo_que_se_ensena_y_nadie_toca_deja_de_salir() {
    banco(|db| {
        let id = ingest::ingerir(db, &texto("nadie me hace caso"))[0]
            .id
            .clone()
            .unwrap();
        for _ in 0..2 {
            reaparicion::apuntar_mostrada(db, &id, "old_random").unwrap();
        }
        assert!(
            reaparicion::siguiente(db, None).unwrap().is_some(),
            "dos todavía"
        );
        reaparicion::apuntar_mostrada(db, &id, "old_random").unwrap();
        assert!(
            reaparicion::siguiente(db, None).unwrap().is_none(),
            "a la tercera, silencio"
        );
    });
}

//  Application context is opt-in and comes from outside: the worker does not
//  inspect any windows. It simply prefers the specified folder.
#[test]
fn el_espacio_de_la_aplicacion_manda_sobre_lo_antiguo() {
    banco(|db| {
        ingest::ingerir(db, &texto("algo viejo cualquiera"));
        let mut p = texto("los planos del proyecto");
        p.space = Some("Proyectos".into());
        let del_proyecto = ingest::ingerir(db, &p)[0].id.clone().unwrap();

        let v = reaparicion::siguiente(db, Some("proyectos"))
            .unwrap()
            .unwrap();
        assert_eq!(v.item.id, del_proyecto);
        assert_eq!(v.reason, "app_context");

        //  Without a specified folder, the usual rule wins.
        let otra = reaparicion::siguiente(db, None).unwrap().unwrap();
        assert_ne!(otra.reason, "app_context");
    });
}

//  A muted folder returns nothing. Muting means “do not remind me about this,”
//  and that must be possible without deleting anything.
#[test]
fn un_espacio_silenciado_no_devuelve_nada() {
    banco(|db| {
        let mut p = texto("cosas del trabajo");
        p.space = Some("Proyectos".into());
        ingest::ingerir(db, &p);
        assert!(reaparicion::siguiente(db, None).unwrap().is_some());

        db.execute(
            "UPDATE spaces SET color = 'silenciado' WHERE id = 'proyectos'",
            [],
        )
        .unwrap();
        assert!(reaparicion::siguiente(db, None).unwrap().is_none());
    });
}

#[test]
fn lo_que_hiciste_con_ella_se_apunta_una_vez() {
    banco(|db| {
        let id = ingest::ingerir(db, &texto("una cualquiera"))[0]
            .id
            .clone()
            .unwrap();
        let evento = reaparicion::apuntar_mostrada(db, &id, "old_random").unwrap();

        //  An unknown outcome is not saved: the list is closed.
        assert!(!reaparicion::apuntar_resultado(db, evento, "me_dio_igual").unwrap());
        assert!(reaparicion::apuntar_resultado(db, evento, "dismissed").unwrap());
        //  It also cannot be answered twice: an existing outcome is not
        //  overwritten.
        assert!(!reaparicion::apuntar_resultado(db, evento, "opened").unwrap());

        assert_eq!(reaparicion::descartes_seguidos(db).unwrap(), 1);
        assert_eq!(reaparicion::mostradas_hoy(db).unwrap(), 1);
    });
}

//  After two consecutive dismissals, go quiet until tomorrow. The worker
//  keeps the count; the caller applies the quiet period because it knows the
//  user's local time.
#[test]
fn los_descartes_seguidos_se_cuentan_y_se_cortan() {
    banco(|db| {
        let id = ingest::ingerir(db, &texto("una"))[0].id.clone().unwrap();
        for _ in 0..2 {
            let e = reaparicion::apuntar_mostrada(db, &id, "old_random").unwrap();
            reaparicion::apuntar_resultado(db, e, "dismissed").unwrap();
        }
        assert_eq!(reaparicion::descartes_seguidos(db).unwrap(), 2);

        //  An item you opened breaks the streak: the count is CONSECUTIVE.
        let e = reaparicion::apuntar_mostrada(db, &id, "old_random").unwrap();
        reaparicion::apuntar_resultado(db, e, "opened").unwrap();
        assert_eq!(reaparicion::descartes_seguidos(db).unwrap(), 0);
    });
}

//  Ignoring it counts the same as dismissing it. This tests the bug that made
//  it appear three times a day for five consecutive days.
//
//  Manually dismissing means opening the page, choosing the card, and pressing
//  a small button. Someone who does not want the feature will not make that
//  trip; they simply ignore it. When the count included only explicit
//  outcomes, fifteen consecutive appearances produced a streak of zero and
//  the quiet period never started.
#[test]
fn ignorar_una_aparicion_cuenta_como_no_haber_servido() {
    banco(|db| {
        let id = ingest::ingerir(db, &texto("una"))[0].id.clone().unwrap();
        //  Shown and untouched: no outcome, as expected.
        reaparicion::apuntar_mostrada(db, &id, "old_random").unwrap();
        assert_eq!(reaparicion::descartes_seguidos(db).unwrap(), 1);
        reaparicion::apuntar_mostrada(db, &id, "old_random").unwrap();
        assert_eq!(
            reaparicion::descartes_seguidos(db).unwrap(),
            2,
            "a la segunda, a callar"
        );

        //  Opening one breaks the streak even if earlier items still have no
        //  outcome: one useful appearance is the opposite signal.
        let e = reaparicion::apuntar_mostrada(db, &id, "old_random").unwrap();
        reaparicion::apuntar_resultado(db, e, "opened").unwrap();
        assert_eq!(reaparicion::descartes_seguidos(db).unwrap(), 0);
    });
}

//  The quiet period lasts one day, not forever. The plan promised that, but
//  the query did not check the date: once quiet, it stayed quiet for the rest
//  of the library's lifetime.
#[test]
fn el_silencio_se_levanta_al_dia_siguiente() {
    banco(|db| {
        let id = ingest::ingerir(db, &texto("una"))[0].id.clone().unwrap();
        let anteayer = util::ahora_ms() - 2 * 86_400_000;
        for _ in 0..3 {
            db.execute(
                "INSERT INTO revisit_events (capture_id, reason, shown_at) \
                        VALUES (?1, 'old_random', ?2)",
                rusqlite::params![id, anteayer],
            )
            .unwrap();
        }
        assert_eq!(
            reaparicion::descartes_seguidos(db).unwrap(),
            0,
            "lo de anteayer no puede seguir tapándole la boca hoy"
        );
    });
}

//  When the last one appeared. This replaces the QML property that was cleared
//  on every reload, making the half-hour gap ineffective.
#[test]
fn la_ultima_aparicion_sale_del_historial() {
    banco(|db| {
        assert_eq!(
            reaparicion::ultima_aparicion(db).unwrap(),
            0,
            "sin historial, cero: y cero deja pasar la primera"
        );
        let id = ingest::ingerir(db, &texto("una"))[0].id.clone().unwrap();
        let antes = util::ahora_ms();
        reaparicion::apuntar_mostrada(db, &id, "old_random").unwrap();
        assert!(reaparicion::ultima_aparicion(db).unwrap() >= antes);
    });
}

//  Deterministic: same result twice in a row. This is what separates an
//  explainable engine from one that must be trusted.
#[test]
fn dos_veces_seguidas_devuelve_lo_mismo() {
    banco(|db| {
        for i in 0..12 {
            ingest::ingerir(db, &texto(&format!("cosa {}", i)));
        }
        let a = reaparicion::siguiente(db, None).unwrap().unwrap();
        let b = reaparicion::siguiente(db, None).unwrap().unwrap();
        assert_eq!(a.item.id, b.item.id);
        assert_eq!(a.reason, b.reason);
    });
}

// ── phase output ──────────────────────────────────────────────────

//  “Saving, restarting, and restoring preserves every item.”
//
//  Truly restart: close the connection and open another on the same disk, as
//  happens when Marea reloads or the machine shuts down. Anything held only in
//  memory would be lost here.
#[test]
fn guardar_reiniciar_y_recuperar() {
    let _g = CERROJO.lock().unwrap_or_else(|e| e.into_inner());
    let mut b = [0u8; 8];
    util::azar(&mut b);
    let dir = std::env::temp_dir().join(format!("deriva-reinicio-{}", util::hex(&b)));
    std::env::set_var("ETTA_DERIVA_DIR", &dir);

    let f = std::env::temp_dir().join(format!("deriva-r-{}.bin", util::ahora_ms()));
    std::fs::write(&f, b"un adjunto que tiene que sobrevivir").unwrap();

    let mut ids = Vec::new();
    {
        let db = db::abrir().unwrap();
        for i in 0..25 {
            let mut p = texto(&format!("captura numero {} con la palabra pulpo", i));
            p.title = Some(format!("Nota {}", i));
            p.tags = vec!["prueba".into()];
            let r = ingest::ingerir(&db, &p);
            ids.push(r[0].id.clone().unwrap());
        }
        let p = ingest::Peticion {
            tipo: "document".into(),
            paths: vec![f.to_string_lossy().to_string()],
            ..Default::default()
        };
        ids.push(ingest::ingerir(&db, &p)[0].id.clone().unwrap());
        assert_eq!(search::cuentas(&db).unwrap().captures, 26);
    } //  aquí se cierra: se acabó el proceso, digamos.

    {
        let db = db::abrir().unwrap();
        assert_eq!(
            search::cuentas(&db).unwrap().captures,
            26,
            "faltan capturas"
        );
        assert_eq!(search::buscar(&db, "pulpo", 100).unwrap().len(), 25);
        assert_eq!(search::buscar(&db, "prueba", 100).unwrap().len(), 25);
        for id in &ids {
            assert!(search::una(&db, id).unwrap().is_some(), "se perdió {}", id);
        }
        //  The attachment is still on disk and remains unchanged.
        let i = mantenimiento::integridad(&db, true).unwrap();
        assert!(i.ok, "{:?}", i);
    }

    let _ = std::fs::remove_file(&f);
    let _ = std::fs::remove_dir_all(&dir);
    std::env::remove_var("ETTA_DERIVA_DIR");
}

// ── search well, not just find ───────────────────────────────────

//  The first result is the one that gets opened. A word in a capture's title
//  must outrank the same word buried in another capture's body.
#[test]
fn el_titulo_pesa_mas_que_el_cuerpo() {
    banco(|db| {
        let mut en_cuerpo =
            texto("Un artículo larguísimo que en algún párrafo menciona las mareas de pasada.");
        en_cuerpo.title = Some("Notas sueltas".into());
        ingest::ingerir(db, &en_cuerpo);
        let mut en_titulo = texto("Apuntes de la playa.");
        en_titulo.title = Some("Mareas".into());
        ingest::ingerir(db, &en_titulo);
        let r = search::buscar(db, "mareas", 10).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(
            r[0].title,
            "Mareas",
            "{:?}",
            r.iter().map(|f| f.title.clone()).collect::<Vec<_>>()
        );
    });
}

//  “pan” used to find “screenshot.png” before “homemade bread”: a prefix,
//  intended for a word still being typed, weighed as much as the complete word.
#[test]
fn la_palabra_entera_va_antes_que_lo_que_empieza_igual() {
    banco(|db| {
        let mut a = texto("una imagen");
        a.title = Some("captura-pantalla.png".into());
        ingest::ingerir(db, &a);
        let mut b = texto("una receta");
        b.title = Some("Cómo hacer pan casero sin amasar".into());
        ingest::ingerir(db, &b);
        let r = search::buscar(db, "pan", 10).unwrap();
        assert_eq!(r.len(), 2, "el prefijo sigue encontrando las dos");
        assert!(
            r[0].title.contains("pan casero"),
            "{:?}",
            r.iter().map(|f| f.title.clone()).collect::<Vec<_>>()
        );
        //  A partial word still matches while typing.
        assert_eq!(search::buscar(db, "amas", 10).unwrap().len(), 1);
    });
}

//  “the youtube video” could not find any YouTube videos: the link's source
//  was not in the index.
#[test]
fn un_enlace_se_encuentra_por_su_sitio() {
    banco(|db| {
        let mut a = url("https://www.youtube.com/watch?v=abc123");
        a.title = Some("Cómo hacer pan casero".into());
        ingest::ingerir(db, &a);
        let mut b = url("https://youtu.be/xyz789");
        b.title = Some("Otro vídeo".into());
        ingest::ingerir(db, &b);
        let mut c = url("https://es.wikipedia.org/wiki/Marea");
        c.title = Some("Marea".into());
        ingest::ingerir(db, &c);
        assert_eq!(search::buscar(db, "youtube", 10).unwrap().len(), 2);
        assert_eq!(search::buscar(db, "pan youtube", 10).unwrap().len(), 1);
        assert_eq!(search::buscar(db, "wikipedia", 10).unwrap().len(), 1);
        assert_eq!(search::buscar(db, "wiki", 10).unwrap().len(), 1);
    });
}

//  An existing library is at version 5, with an index that lacks the source.
//  Opening it with this worker rebuilds the index, and old links can be found
//  by source without saving them again.
#[test]
fn una_biblioteca_de_la_5_aprende_a_buscar_por_sitio() {
    let _g = CERROJO.lock().unwrap_or_else(|e| e.into_inner());
    let mut b = [0u8; 8];
    util::azar(&mut b);
    let dir = std::env::temp_dir().join(format!("deriva-v5-{}", util::hex(&b)));
    std::env::set_var("ETTA_DERIVA_DIR", &dir);
    {
        let vieja = db::abrir().expect("no abre");
        let mut p = url("https://www.youtube.com/watch?v=abc123");
        p.title = Some("Pan casero".into());
        ingest::ingerir(&vieja, &p);
        vieja
            .execute_batch(
                "DROP TABLE captures_fts;
                 CREATE VIRTUAL TABLE captures_fts USING fts5(
                     capture_id UNINDEXED,
                     title, author, excerpt, summary, note, content_text, tags,
                     tokenize = \"unicode61 remove_diacritics 2\"
                 );
                 INSERT INTO captures_fts (capture_id, title, author, excerpt, summary, note, content_text, tags)
                   SELECT id, title, author, excerpt, summary, note, content_text, '' FROM captures;
                 PRAGMA user_version = 5;",
            )
            .expect("no la puedo dejar como estaba");
    }
    let db = db::abrir().expect("no abre la de la 5");
    let v: i64 = db
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, db::VERSION);
    assert_eq!(
        search::buscar(&db, "pan", 10).unwrap().len(),
        1,
        "lo de antes se sigue encontrando"
    );
    assert_eq!(
        search::buscar(&db, "youtube", 10).unwrap().len(),
        1,
        "y ahora también por su sitio"
    );
    drop(db);
    let _ = std::fs::remove_dir_all(&dir);
    std::env::remove_var("ETTA_DERIVA_DIR");
}

// ── clearing the page: bulk trash and permanent deletion ────────────

fn pedir(db: &rusqlite::Connection, metodo: &str, params: serde_json::Value) -> proto::Respuesta {
    proto::despachar(
        db,
        &proto::Peticion {
            v: 1,
            id: "t".into(),
            method: metodo.into(),
            params,
        },
    )
}

//  Several at once go to the trash, stop being listed and found, and come
//  back out together.
#[test]
fn varias_a_la_papelera_y_de_vuelta() {
    banco(|db| {
        let a = ingest::ingerir(db, &texto("primera nota para borrar"))[0]
            .id
            .clone()
            .unwrap();
        let b = ingest::ingerir(db, &texto("segunda nota para borrar"))[0]
            .id
            .clone()
            .unwrap();
        let c = ingest::ingerir(db, &texto("tercera, que se queda"))[0]
            .id
            .clone()
            .unwrap();
        let r = pedir(
            db,
            "trash",
            serde_json::json!({ "ids": [a, b, "no-existe"] }),
        );
        assert!(r.ok);
        assert_eq!(
            r.result.as_ref().unwrap()["count"],
            2,
            "solo las que existen"
        );
        let quedan = search::listar(db, 50, None, &Default::default()).unwrap();
        assert_eq!(quedan.len(), 1);
        assert_eq!(quedan[0].id, c);
        assert!(
            search::buscar(db, "borrar", 10).unwrap().is_empty(),
            "lo tirado aún se encuentra"
        );

        let r = pedir(db, "untrash", serde_json::json!({ "ids": [a, b] }));
        assert_eq!(r.result.as_ref().unwrap()["count"], 2);
        assert_eq!(
            search::listar(db, 50, None, &Default::default())
                .unwrap()
                .len(),
            3
        );
        assert_eq!(
            search::buscar(db, "borrar", 10).unwrap().len(),
            2,
            "lo recuperado no se encuentra"
        );

        //  One by its id still answers as it always did.
        let r = pedir(db, "trash", serde_json::json!({ "id": c }));
        assert_eq!(r.result.as_ref().unwrap()["id"], c);
        assert!(!pedir(db, "trash", serde_json::json!({ "id": "no-existe" })).ok);
        assert!(!pedir(db, "trash", serde_json::json!({})).ok);
    });
}

//  Deleting for real takes the row, its index and its file; a file another
//  capture still points at stays until that one goes too.
#[test]
fn borrar_de_verdad_se_lleva_el_fichero_cuando_nadie_lo_usa() {
    banco(|db| {
        let f = std::env::temp_dir().join(format!("deriva-purga-{}.txt", util::ahora_ms()));
        std::fs::write(&f, b"bytes que se van a ir").unwrap();
        let p = ingest::Peticion {
            tipo: "document".into(),
            paths: vec![f.to_string_lossy().to_string()],
            ..Default::default()
        };
        let a = ingest::ingerir(db, &p)[0].id.clone().unwrap();
        let hash = util::sha256(b"bytes que se van a ir");
        let sitio = paths::blob_de(&hash);
        assert!(sitio.exists());

        //  A second capture sharing that file (as two of the same picture do).
        let b = ingest::ingerir(db, &texto("otra que comparte el fichero"))[0]
            .id
            .clone()
            .unwrap();
        db.execute(
            "UPDATE captures SET blob_hash = ?1 WHERE id = ?2",
            rusqlite::params![hash, b],
        )
        .unwrap();
        //  And something hanging from the first, which must go with it.
        db.execute(
            "INSERT OR IGNORE INTO capture_spaces (capture_id, space_id) VALUES (?1, 'leer-luego')",
            [&a],
        )
        .unwrap();

        let r = pedir(db, "purge", serde_json::json!({ "ids": [a] }));
        assert!(r.ok);
        assert_eq!(r.result.as_ref().unwrap()["count"], 1);
        assert_eq!(
            r.result.as_ref().unwrap()["files"],
            0,
            "borró un fichero que otra usa"
        );
        assert!(sitio.exists());
        assert!(search::una(db, &a).unwrap().is_none(), "la fila sigue ahí");
        let en_indice: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM captures_fts WHERE capture_id = ?1",
                [&a],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(en_indice, 0, "sigue en el índice");
        let colgando: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM capture_spaces WHERE capture_id = ?1",
                [&a],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(colgando, 0, "su carpeta no se fue con ella");

        //  The last one that used it: now the file goes.
        let r = pedir(db, "purge", serde_json::json!({ "id": b }));
        assert_eq!(r.result.as_ref().unwrap()["files"], 1);
        assert!(!sitio.exists(), "el fichero se quedó");
        assert_eq!(
            search::listar(db, 50, None, &Default::default())
                .unwrap()
                .len(),
            0
        );
        let _ = std::fs::remove_file(&f);
    });
}

//  On a backup opened with `--db`, deleting for real takes its rows and not
//  the files: those are the live library's, which still uses them.
#[test]
fn borrar_en_una_copia_no_toca_los_ficheros_de_la_viva() {
    banco(|db| {
        let f = std::env::temp_dir().join(format!("deriva-copia-{}.txt", util::ahora_ms()));
        std::fs::write(&f, b"bytes de la viva").unwrap();
        let p = ingest::Peticion {
            tipo: "document".into(),
            paths: vec![f.to_string_lossy().to_string()],
            ..Default::default()
        };
        let a = ingest::ingerir(db, &p)[0].id.clone().unwrap();
        let sitio = paths::blob_de(&util::sha256(b"bytes de la viva"));
        let copia = paths::base().join("copia.sqlite3");
        db.execute("VACUUM INTO ?1", [copia.to_string_lossy().to_string()])
            .unwrap();
        let otra = db::abrir_en(&copia).expect("abre la copia");
        let r = pedir(&otra, "purge", serde_json::json!({ "ids": [a] }));
        assert!(r.ok);
        assert_eq!(r.result.as_ref().unwrap()["count"], 1);
        assert_eq!(r.result.as_ref().unwrap()["files"], 0);
        assert_eq!(r.result.as_ref().unwrap()["files_kept"], true);
        assert!(sitio.exists(), "borró el fichero de la viva");
        assert!(
            search::una(db, &a).unwrap().is_some(),
            "la viva perdió su fila"
        );
        //  And one id that is not there says so, as trash does.
        assert!(!pedir(db, "purge", serde_json::json!({ "id": "no-existe" })).ok);
        let _ = std::fs::remove_file(&f);
    });
}

//  Continue the list with `before`, one page after another.
#[test]
fn la_lista_sigue_por_paginas() {
    banco(|db| {
        for i in 0..5 {
            ingest::ingerir(db, &texto(&format!("nota número {}", i)));
            std::thread::sleep(std::time::Duration::from_millis(3));
        }
        let primera = search::listar(db, 3, None, &Default::default()).unwrap();
        assert_eq!(primera.len(), 3);
        let r = pedir(
            db,
            "list",
            serde_json::json!({ "limit": 3, "before": primera[2].captured_at }),
        );
        let resto = r.result.as_ref().unwrap()["items"]
            .as_array()
            .unwrap()
            .len();
        assert_eq!(resto, 2);
    });
}
