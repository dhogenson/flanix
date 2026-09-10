# TODO

A consolidated list of todos gathered from the daily logs in `docs/logs/` and codebase analysis.

- [ ] **`--dry-run` flag for pull** — preview changes before pulling, since pull deletes local files. *(Still open: no such flag exists in the CLI.)*
- [ ] Create different errors for each crate
- [ ] **Add CI/CD pipeline** — no `.github/workflows/` exists. Add GitHub Actions to run `cargo fmt --check`, `cargo clippy`, and `tests.sh` on push/PR.
- [ ] **Full rescan is expensive** — the current sync does a full rescan of the folder (glob-based), which is bad for large folders. *(Still open: no incremental/delta scanning exists.)*
- [ ] Figure out how to store the database values that i keep calling over and over again, or do a batch pull
- [ ] **Non-atomic push** — `sync.rs:57-105` uploads file-by-file without a transaction. Crash between S3 upload and DB insert leaves orphaned S3 objects. *(Related to "Partial sync on failure" below.)*
- [ ] **Hardcoded test defaults** — `config_file.rs:37-46` `Config::default()` hardcodes `http://localhost:4566`, `user:password`, etc. Acceptable for dev but should be externalized before release.
