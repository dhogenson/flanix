# TODO

A consolidated list of todos gathered from the daily logs in `docs/logs/` and codebase analysis.

---

## In Progress / New Ideas

- [x] **TOML config** — save/load configuration as a TOML file instead of the current format. *(Still open: `config_file.rs` still reads/writes `config.json` via serde_json.)*
- [ ] **`--dry-run` flag for pull** — preview changes before pulling, since pull deletes local files. *(Still open: no such flag exists in the CLI.)*
- [ ] Create different errors for each crate

---

## Critical: Fix Before Publishing to GitHub

- [x] **`.env` committed to git** — despite being listed in `.gitignore`, the `.env` file is already tracked (committed in `f9310b3` and earlier). The `.gitignore` rule is ineffective once a file is tracked. Fix: `git rm --cached .env` to untrack it while keeping the local file. *(Verified: `git ls-files` shows `.env` is tracked.)*
- [x] **First-run config crash** — `config_file.rs:98-100` created an empty config file with `File::create()`, then `Config::new()` (lines 51-59) failed to parse the empty content via `toml::from_str`. On a fresh system the app was unusable — the `add` subcommand was never reachable because `Sync::new()` fails first. *(Fixed: `config_file.rs` now writes default config content when creating the file.)*
- [x] **`unwrap()` panic on missing env var** — `s3.rs:52` does `env::var("AWS_DEFAULT_REGION").unwrap()` which panics in production if the variable isn't set. Should return `Result` or fall back to `GLOBAL_REGION`.
- [x] **Silently swallowed walkdir errors** — `scan_files.rs` previously used `.filter_map(|e| e.ok())` which silently discarded all `WalkDir` errors (permission denied, broken symlinks, I/O errors). Files in unreadable directories were silently skipped → data loss on push, stale files on pull. *(Fixed: `scan()` now propagates the first `WalkDir` error via `?`.)*
- [ ] **Add CI/CD pipeline** — no `.github/workflows/` exists. Add GitHub Actions to run `cargo fmt --check`, `cargo clippy`, and `tests.sh` on push/PR.
- [ ] **Add LICENSE file** — no LICENSE exists anywhere in the repo. Without one, GitHub flags the repo and others legally can't use the code. MIT recommended for personal projects.

---

## Optimization: Performance

- [x] **Double directory traversal** — `scan_files.rs:60-75` walks the entire tree once with `WalkDir::new(&self.scan_path)` to collect all dirs, then walks each dir *again* with `max_depth(1)` in `scan_folder()`. Every directory is visited twice. Fix: single `WalkDir` pass collecting files directly.
- [x] **Quadratic diff logic** — `diff.rs:52-56` and `diff.rs:110-113` use `.contains()`/`.any()` on `Vec`s inside loops → O(n*m). Fix: use `HashSet` (a `local_index` HashMap is already built at `diff.rs:74` — reuse it instead of rebuilding/looking up per-file).
- [x] **`download_object` buffers entire file in memory** — `s3.rs:154` does `response.body.collect().await?.into_bytes()` before writing to disk. For large files this is a memory concern, especially on mobile (the stated target). Fix: stream the body directly to a file via `AsyncWriteExt`.
- [x] **`list_buckets()` used for existence check** — `s3.rs:66-80` lists ALL buckets and matches by name. Fix: use `head_bucket()` (single request) for existence checks.
- [x] **`max_connections(5)` hardcoded** — `db.rs:30`. Should be configurable.

---

## Sync Core Improvements

- [ ] **Full rescan is expensive** — the current sync does a full rescan of the folder (glob-based), which is bad for large folders. *(Still open: no incremental/delta scanning exists.)*
- [ ] **DB keyed by path → rename/move problem** — because the DB map is keyed by path, renaming or moving a folder makes it look brand new (new UUID) and gets re-uploaded under a new ID. Fix with content hashing: use something cheap to store in the database so renamed files can be matched by identical hashes. *(Still open: `File` has no hash column; `files_to_upload` matches purely on path (`sync.rs` line 200).)*
- [ ] **Bucket state not consulted** — cloud state is really just "what the DB thinks is in the cloud." If an object is deleted from S3 but the DB row still exists, it's treated as up-to-date and skipped. The bucket should actually be consulted. *(Still open: `push`/`pull` only ever read from the DB; S3 contents are never queried during sync.)*
- [ ] **Partial sync on failure** — if push or pull fails mid-loop, you end up in a partially-synced state. Handle this gracefully (e.g., atomic commits / rollback). *(Still open: `push`/`pull` use `?` and don't roll back partial writes/deletes.)*
- [ ] Figure out how to store the database values that i keep calling over and over again

---

## Cleanup / Housekeeping

- [ ] **Unused dependencies** — remove dead deps:
  - `uuid.workspace = true` in `sync-cli/Cargo.toml:11` (never used)
  - `glob = "0.3.4"` in `sync-core/Cargo.toml:16` (never used in source)
  - `serde_json.workspace = true` in `sync-core/Cargo.toml:20` (never used directly)
  - `dotenvy = "0.15.7"` in `sync-config/Cargo.toml:11` (never called — `.env` file exists but is never loaded)
- [ ] **Dead code** — remove or wire up:
  - `Database::get_file_ids()` in `db.rs:108-117` — `pub` but never called anywhere
  - `file_hash` field + `hash_file()` in `scan_files.rs:19,34` — only used in tests, never in production sync
  - `test_hash_file_on_big_file` in `scan_files.rs:133-136` — empty test stub that asserts nothing
- [ ] **Commented-out code** — remove:
  - `diff.rs:67` — `// let local_files = scan_files(path)?;`
  - `diff.rs:100` — `// let local_files: Vec<PathBuf> = ...`
  - `sync-core/Cargo.toml:11` — `# aws-sdk-s3 = {version = "1.141.0" }`
- [ ] **Non-atomic config writes** — `config_file.rs:76-79` `create_namespace()` writes via `File::create()` + `BufWriter` without flush/sync, truncating the file in place. A crash mid-write corrupts the entire config. Fix: write to temp file, then atomically rename.
- [ ] **Non-atomic push** — `sync.rs:57-105` uploads file-by-file without a transaction. Crash between S3 upload and DB insert leaves orphaned S3 objects. *(Related to "Partial sync on failure" below.)*
- [ ] **`dotenvy` never called** — `.env` exists in repo root with AWS/DB values, but no code calls `dotenvy::dotenv()`. Either wire it up in `main.rs` or remove the dependency.
- [ ] **Cargo.toml metadata** — no `license`, `description`, or `repository` fields on any crate. Add them for GitHub/crates.io hygiene.
- [ ] **Clippy `result_large_err` (18 warnings)** — `SyncError` is 168 bytes. Consider boxing large variants (`Box<aws_sdk_s3::Error>`, `Box<ByteStreamError>`, `Box<anyhow::Error>`) to shrink the enum.
- [ ] **Clippy fixes available via `cargo clippy --fix`** — `needless_borrow`, `redundant_field_names`, `collapsible_if`, `useless_conversion`, `vec_init_then_push`, `new_without_default`, `empty_line_after_outer_attr`, `field_reassign_with_default`.
- [ ] **Hardcoded test defaults** — `config_file.rs:37-46` `Config::default()` hardcodes `http://localhost:4566`, `user:password`, etc. Acceptable for dev but should be externalized before release.
- [ ] **Add `.env.example`** — provide a template of expected env vars (the real `.env` should not be tracked; see Critical section).
- [ ] **Expand `.gitignore`** — currently only `/test_files`, `/target`, `.env`. Consider adding `**/*.rs.bk`, `.DS_Store`, `*.local`, `config.toml`, `.floci/`.
- [ ] **TUI title typo** — `sync-tui/src/lib.rs:155` says "This is a app" → should be "This is an app".

---

## Testing & Code Quality

- [ ] **Rewrite AI-written tests** — the existing tests were generated by AI and need rewriting. *(Unverified: current tests in `tests/db.rs`/`tests/s3.rs` look hand-written with explanatory comments, but completion can't be confirmed from code alone.)*
- [ ] **Local constant confusion** — figure out what "local constatin" is and fix it (was patched with AI for now). *(Unverified: the typo isn't present anywhere in source code, suggesting it may already be addressed.)*

---

## Build / Platform

- [x] **Windows build** — resolve linking errors when building on Windows. Reference command:
  - Install the tooling: `cargo install cargo-xwin`
  - Build: `cargo xwin build --release --target x86_64-pc-windows-msvc`
  - (Requires adding LLVM/Clang dependencies.)
