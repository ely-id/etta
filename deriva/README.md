# deriva-worker

Deriva's local library: it saves what you hand over to Marea, finds it, and
returns it.

It is a separate binary, not QML code, for a specific reason: Quickshell reloads
the entire shell when a file in the repo is saved, and a database that is
mid-write does not survive that. Here, reloading the interface does not touch
anything.

## Using it

```sh
./marea deriva where            # where everything is
./marea deriva stats            # how many items there are
./marea deriva search --query "pulpo"
./marea deriva integrity-check --deep
./marea deriva backup --keep 5
./marea deriva export --directory /ruta/absoluta
```

It is compiled the first time it is called. Manually:

```sh
cd deriva && cargo build --release
```

`ETTA_DERIVA_DIR` moves the entire library and `ETTA_DERIVA_SOCKET` moves the
socket. Tests use them to avoid going near the user's library.

## How it communicates

Marea communicates with it over a private Unix socket in
`$XDG_RUNTIME_DIR/proyecto-marea/`, with permissions `0600`. Versioned JSON
Lines, one line per message:

```json
{"v":1,"id":"req-42","method":"search","params":{"query":"liquid UI","limit":20}}
{"v":1,"id":"req-42","ok":true,"result":{"items":[]}}
```

No ports, not even on `localhost`: any process on the machine can reach a port,
including any tab with a `fetch`.

Methods: `ping`, `ingest`, `search`, `list`, `get`, `stats`, `integrity_check`,
`repair_index`, `backup`, `export`, `trash`, `untrash`, `shutdown`. The command
line uses **the same dispatcher**: two paths to the same database are how you
end up with two behaviors and a test that covers only one.

## What it does not do

- **It does not access the network.** Not a single connection, and it will not
  open one. URL enrichment does exist—the title, author, and cover image of a
  link—but the part that accesses the network is the host: `tools/web`, with
  its anti-SSRF rules, and what it retrieves comes in through the `enrich`
  method. There is nothing to protect here because nothing leaves, which is
  easy to verify and easy to lose.
- **It does not execute anything.** It does not launch processes or interpret
  saved content.
- **It does not delete anything unless asked for it by name.** Throwing
  something away and destroying it are different things; `trash` marks it, and
  the row remains.
- **It does not accept arbitrary paths** except in `export`, where the user
  chooses the path and the controller validates it before it gets here.

## Where each item comes from

From the URL alone, without asking anyone for anything:
`adaptadores.rs` recognizes YouTube, Reddit, X, GitHub, Vimeo, Twitch, and, by
extension, PDFs, images, videos, and audio. That gives us three things:

- **The site** (`source`), which the interface displays with its brand and
  color.
- **The actual type**: a YouTube link is a `video` even if it does not end in
  `.mp4`.
- **A site-specific canonical URL**, which makes `youtu.be/ID`, `watch?v=ID`,
  `watch?v=ID&t=90`, `&list=…`, and `shorts/ID` **the same video**. Otherwise,
  dragging the same video from two sites saves two captures, and “you already
  had this” is never true.

And unknown details are not invented: a YouTube channel does not pretend to be
a video—without an identifier in the right format, there is no canonical URL.
An arbitrary page keeps its domain as its title, which says more than “Untitled.”

What requires the network—the actual title, channel, and thumbnail—comes
through `enrich`, brought in by the component with network access. See
[PRIVACIDAD.md](../design/deriva/PRIVACIDAD.md).

## Folders and summaries

The three default folders—Projects, Read later, Inspiration—are examples, not
the whole list. `space_put` and `space_delete` let you make your own: they are
created empty, can be renamed without changing their ID, and when removed,
**their contents stay**. The ID comes from the name, and is an ID:
`Investigación` → `investigacion`.

And `classify` saves what the AI understands from a capture: a summary, some
tags, and, if appropriate, a folder. There is no model here—just as there is no
network access: the host requests this from the user's engine and it arrives
already written. The rules for what can be overwritten are stricter than those
for `enrich`, because a title provided by a page is a fact, while this is a
model's opinion:

- The **note** is never touched: it is yours.
- The **summary** is set only if there was none.
- **Tags are added**; yours stay where they were.
- The **folder** is set only if the capture was not already in one.

The summary lives in its own column and **is included in the index**, with
almost the weight of a tag: these are forty words chosen to say what this is
about, so a match there is worth much more than one in the middle of an article.
A summary that cannot be found by searching is no use to anyone.

## It knows what's inside

`enrich` also saves the **body**: the text of the page you linked and the text
extracted from a file. It goes into `content_text`, which was already in the
index and was empty for everything other than a text selection. Without this,
searching for a phrase from a saved article found nothing, and the library only
knew about titles.

It is written **once and stays**. A capture is a copy of what you saw; one that
rereads itself a month later is no longer that.

And the host extracts it, not this worker: reading a page means accessing the
network, and extracting text from a PDF means launching a process on a file
someone gave you. It arrives here already read.

## And it searches by meaning, if you give it the model

`search` first returns what CONTAINS your words—FTS5 with BM25, all terms and
then any term—and fills in the rest with what is SIMILAR. The latter happens
only if there is a model in `<base>/modelo`; without one, everything works as
before.

The crate is compiled with `local-only`: this worker cannot download a model,
just as it cannot open a connection. The host provides it.

The vectors live in `capture_vectors`, raw and with their distance metric
alongside them. A separate table means that when there is a better model, it can
be emptied and rebuilt without touching a single capture; the metric is there
because comparing vectors from different models does not produce an error—it
produces an invented similarity.

And `similar` displays the similarity with its score, the only way to tune the
threshold without guessing.

## Where saved data lives

```text
${XDG_DATA_HOME:-~/.local/share}/proyecto-marea/deriva/
├── library.sqlite3
├── blobs/ab/cd/<sha256>
├── previews/
├── exports/
└── backups/
```

A blob's name is its hash. That gives us three things for free: saving the same
file twice takes up one copy, checking integrity means hashing it again, and
there is no need to escape names provided by the user—which is where half the
holes in file stores come from.

## Two rules that hold everything else together

**The blob before the row.** A crash between the two leaves, at most, ownerless
bytes on disk: they can be seen, counted, and swept away. The reverse would
leave a visible capture pointing to a file that does not exist, which cannot be
repaired by reading the database.

**Nothing is considered saved until SQLite confirms it.** Marea's success
animation waits for this confirmation, not the other way around.

## Testing it

```sh
./test-prototype --deriva
```

Internally, with `cargo test`—the database and blobs directly—and externally
with Node, which verifies that the binary and socket do what they say. The test
that gives this phase its name opens and closes different processes against the
same disk: if anything lived in memory, it would fail there.

## Installing and removing it

```sh
./marea deriva instalar      # worker, plugin, and accessories; STARTS DISABLED
./marea deriva desinstalar   # removes everything except your library
```

It starts disabled on purpose: the plugin appears in Settings → Plugins with
the implications of enabling it visible, and until you flip the switch it does
not run, save, or receive anything.

When removed, nothing remains except the library, which is yours. The
uninstaller tells you where it is and how to take it with you or delete it;
exporting and deleting are explicit, separate operations, and neither happens
by accident.

## Bringing in what you already had

```sh
./marea deriva import --folder ~/Documentos/recortes
./marea deriva import --bookmarks ~/Descargas/bookmarks.html --space leer-luego
./marea deriva import --deriva /donde/exportaste
```

The third is the reverse of `export`, and it exists because an export that
cannot be reimported is a dead file in a pretty format.

None of these runs on its own, watches a folder, or accesses the network. You
choose the path.

## When something goes wrong

```sh
./marea deriva doctor
```

It answers the first question in any diagnosis: what version of what is talking
to what. The binary version, expected and actual schema, where everything is,
how many copies there are, and whether the library is intact.

Permissions, privacy, and how to take your data with you are in
[`design/deriva/PRIVACIDAD.md`](../design/deriva/PRIVACIDAD.md).
