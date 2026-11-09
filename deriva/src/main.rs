//  Deriva · the local library worker.
//
//  One small binary. It saves what you drop into Etta, searches for it, and
//  returns it; none of this lives in QML, because QML reloads when you save a
//  repository file, and a half-written database would not survive that.
//
//  Two ways to talk to it, with the same dispatcher behind both: the socket —
//  used by Etta— and the command line, used to diagnose problems without
//  starting the entire desktop.
//
//  What it does NOT do, by design: access the network, open ports, execute
//  anything, or delete anything unless explicitly named.

mod adaptadores;
mod blobs;
mod db;
mod importar;
mod ingest;
mod mantenimiento;
mod paths;
mod proto;
mod reaparicion;
mod search;
mod serve;
mod util;
mod vectores;

#[cfg(test)]
mod pruebas;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let orden = args.first().map(|s| s.as_str()).unwrap_or("");

    let salida = match orden {
        "serve" => {
            if let Err(e) = serve::servir() {
                eprintln!("deriva-worker: {}", e);
                std::process::exit(1);
            }
            return;
        }
        "ingest" => por_metodo("ingest", opcion(&args, "--request")),
        "search" => {
            let q = opcion(&args, "--query").unwrap_or_default();
            let n = opcion(&args, "--limit").unwrap_or_else(|| "20".into());
            por_metodo(
                "search",
                Some(format!(r#"{{"query":{},"limit":{}}}"#, json_texto(&q), n)),
            )
        }
        "get" => {
            let id = opcion(&args, "--id").unwrap_or_default();
            por_metodo("get", Some(format!(r#"{{"id":{}}}"#, json_texto(&id))))
        }
        "list" => {
            let n = opcion(&args, "--limit").unwrap_or_else(|| "50".into());
            //  `--before <ms>`: the page after one that ended there.
            match opcion(&args, "--before") {
                Some(b) => por_metodo("list", Some(format!(r#"{{"limit":{},"before":{}}}"#, n, b))),
                None => por_metodo("list", Some(format!(r#"{{"limit":{}}}"#, n))),
            }
        }
        //  Throwing away, taking back out of the trash, and deleting for
        //  real: one by `--id`, or several with `--ids '["a","b"]'`.
        //  Deleting for real takes the files of the live library with it, so
        //  it is not done on another database: there it would delete files
        //  the live one still uses.
        "purge" if opcion(&args, "--db").is_some() => {
            eprintln!(
                "deriva-worker: purge cannot be used with --db: files belong to the live library"
            );
            std::process::exit(2);
        }
        "trash" | "untrash" | "purge" => por_metodo(orden, Some(ids_params(&args))),
        "stats" => por_metodo("stats", None),
        "doctor" => por_metodo("doctor", None),
        "import" => {
            //  `--folder <path>`, `--bookmarks <file>`, or `--deriva <directory>`.
            let (que, clave, valor) = if let Some(v) = opcion(&args, "--folder") {
                ("folder", "directory", v)
            } else if let Some(v) = opcion(&args, "--bookmarks") {
                ("bookmarks", "file", v)
            } else if let Some(v) = opcion(&args, "--deriva") {
                ("deriva", "directory", v)
            } else {
                eprintln!("deriva-worker: import requires --folder, --bookmarks, or --deriva");
                std::process::exit(2);
            };
            let espacio = opcion(&args, "--space").unwrap_or_default();
            por_metodo(
                "import",
                Some(format!(
                    r#"{{"kind":"{}","{}":{},"space":{}}}"#,
                    que,
                    clave,
                    json_texto(&valor),
                    json_texto(&espacio)
                )),
            )
        }
        //  Everything the page requests on startup, all at once. On the command
        //  line, this serves another purpose: save exactly what the interface
        //  sees to a file, so a visual test can reproduce it without a socket.
        "home" => {
            let n = opcion(&args, "--limit").unwrap_or_else(|| "60".into());
            por_metodo("home", Some(format!(r#"{{"limit":{}}}"#, n)))
        }
        "sections" => por_metodo("sections", None),
        //  What should be remembered and why. On the command line, this audits
        //  the engine: it is deterministic, so asking twice must give the same
        //  result, which can be checked by inspecting it.
        "revisit" => {
            let e = opcion(&args, "--space").unwrap_or_default();
            por_metodo(
                "revisit",
                Some(format!(r#"{{"space":{}}}"#, json_texto(&e))),
            )
        }
        "integrity-check" => {
            let profundo = args.iter().any(|a| a == "--deep");
            por_metodo(
                "integrity_check",
                Some(format!(r#"{{"deep":{}}}"#, profundo)),
            )
        }
        "repair-index" => por_metodo("repair_index", None),
        "backup" => {
            let k = opcion(&args, "--keep").unwrap_or_else(|| "5".into());
            por_metodo("backup", Some(format!(r#"{{"keep":{}}}"#, k)))
        }
        "export" => {
            let d = opcion(&args, "--directory").unwrap_or_default();
            por_metodo(
                "export",
                Some(format!(r#"{{"directory":{}}}"#, json_texto(&d))),
            )
        }
        "where" => {
            //  Where everything is. The first question when something is missing.
            println!(
                "{}",
                serde_json::json!({
                    "base": paths::base().to_string_lossy(),
                    "db": paths::db_path().to_string_lossy(),
                    "blobs": paths::blobs().to_string_lossy(),
                    "backups": paths::backups().to_string_lossy(),
                    "socket": paths::socket().to_string_lossy(),
                    "schema": db::VERSION,
                })
            );
            return;
        }
        "" | "-h" | "--help" | "help" => {
            ayuda();
            return;
        }
        otro => {
            eprintln!("deriva-worker: unknown command '{}'", otro);
            ayuda();
            std::process::exit(2);
        }
    };

    println!("{}", salida.0);
    if !salida.1 {
        std::process::exit(1);
    }
}

//  A command-line instruction is a protocol request. One path to the database:
//  two paths would mean two behaviors and a test covering only one of them.
fn por_metodo(metodo: &str, params: Option<String>) -> (String, bool) {
    //  `--db` opens ANOTHER database: use it to inspect a backup before
    //  restoring it, which is half the point of having backups. Without
    //  this, checking a backup requires replacing the working library.
    let args: Vec<String> = std::env::args().skip(1).collect();
    let abrir = match opcion(&args, "--db") {
        Some(p) => db::abrir_en(std::path::Path::new(&p)),
        None => db::abrir(),
    };
    let db = match abrir {
        Ok(d) => d,
        Err(e) => {
            return (
                serde_json::json!({ "ok": false, "error": e.to_string() }).to_string(),
                false,
            )
        }
    };
    let valor: serde_json::Value = match params {
        Some(s) if !s.trim().is_empty() => match serde_json::from_str(&s) {
            Ok(v) => v,
            Err(e) => {
                return (
                    serde_json::json!({ "ok": false, "error": e.to_string() }).to_string(),
                    false,
                )
            }
        },
        _ => serde_json::Value::Null,
    };
    let p = proto::Peticion {
        v: proto::VERSION,
        id: "cli".into(),
        method: metodo.into(),
        params: valor,
    };
    let r = proto::despachar(&db, &p);
    let ok = r.ok;
    (serde_json::to_string(&r).unwrap_or_default(), ok)
}

fn opcion(args: &[String], nombre: &str) -> Option<String> {
    args.iter()
        .position(|a| a == nombre)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

//  `--ids` is already a JSON list (it is checked as the request is read);
//  `--id`, one.
fn ids_params(args: &[String]) -> String {
    match opcion(args, "--ids") {
        Some(lista) => format!(r#"{{"ids":{}}}"#, lista),
        None => format!(
            r#"{{"id":{}}}"#,
            json_texto(&opcion(args, "--id").unwrap_or_default())
        ),
    }
}

fn json_texto(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

fn ayuda() {
    eprintln!(
        r#"deriva-worker · Deriva's local library

  serve                              serve the socket used by Etta
  ingest   --request <json>          store a capture
  search   --query <text> [--limit]  search the library
  get      --id <id>                 retrieve a complete capture
  list     [--limit N] [--before ms] list recent captures (up to 500)
  trash    --id <id> | --ids <json>  move captures to trash (reversible)
  untrash  --id <id> | --ids <json>  restore captures from trash
  purge    --id <id> | --ids <json>  permanently delete captures and files
  stats                              show library statistics
  doctor                             show versions, paths, and integrity
  import   --folder <path>           import a directory of files
           --bookmarks <file>        import browser bookmarks
           --deriva <directory>      import a Deriva export
  home     [--limit N]               show the page's startup data
  revisit  [--space <id>]            show what is due for revisiting, and why
  integrity-check [--deep]           verify library integrity
  repair-index                       rebuild missing search index entries
  backup   [--keep N]                create a backup, keeping N copies
  export   --directory <path>        export the library to a directory
  where                              show library paths

Use `--db <path>` to run a command against another database, for example to
inspect a backup without restoring it over the live library.

All output is a single line of JSON. `ETTA_DERIVA_DIR` relocates the library,
which lets tests run without touching the user's data."#
    );
}
