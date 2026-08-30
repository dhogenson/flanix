# AGENTS.md

## Golden Rule

**Do NOT add, edit, or delete any code unless I explicitly ask you to.**

Before making any change to source code, configuration, or build files, confirm
that I have requested it. If in doubt, ask first. Read, explain, and advise
freely — just don't touch the code without being asked.

## Project Overview

Rust workspace for a file-sync tool. It scans a local directory and syncs files
to an S3-compatible bucket (using the local Floci emulator for dev), tracking
sync state in a Postgres database via sqlx.

## Code Formatting

- Use `rustfmt` (the default Rust formatter): `cargo fmt`.
- clippy is available for linting: `cargo clippy`.

## Code Structure

Cargo workspace with two crates:

- **`crates/sync-core/`** — the library crate (`sync_core`). Contains the core
  sync logic:
  - `src/config.rs` — configuration / env handling
  - `src/db.rs` — Postgres access (via sqlx)
  - `src/s3.rs` — S3 bucket operations (via aws-sdk-s3)
  - `src/scanner.rs` — local directory scanning
  - `src/sync.rs` — the `Sync` type orchestrating add/push/pull
  - `src/errors.rs` — error types (thiserror)
  - `src/lib.rs` — module wiring and public re-exports
  - `migrations/` — sqlx SQL migrations
  - `tests/` — integration tests (db, s3)
- **`crates/sync-cli/`** — the binary crate. CLI built with `clap`, uses
  `sync_core::Sync`.

## Tests

- Integration tests live in `crates/sync-core/tests/`.
- There is also a `tests.sh` script that spins up the test services via
  `docker-compose.test.yaml`, applies migrations, runs `cargo test`, and tears
  down the containers.

## How to Run / Build

- Build: `cargo build`
- Run tests: `cargo test`
- Dev services (Floci S3 emulator + Postgres): `docker compose up -d` (see
  `docker-compose.yaml`)
- Interactive shell with AWS env vars set: `nix-shell` (see `shell.nix`)

### Environment

Local dev relies on these env vars (also available in `.env` / `shell.nix`):

- `AWS_ENDPOINT_URL=http://localhost:4566`
- `AWS_DEFAULT_REGION=us-east-1`
- `AWS_ACCESS_KEY_ID=test`
- `AWS_SECRET_ACCESS_KEY=test`
- `DATABASE_URL=postgres://user:password@localhost/mydb`

Note: the sqlx compile-time macros require the live Postgres schema, so tests
need the DB up and migrations applied (that is what `tests.sh` does).

## Helpful Notes

- `.gitignore` excludes `/test_files`, `/target`, and `.env`.
- The CLI (`myapp`) subcommands: `add <name>`, `push <namespace> <path>`,
  `pull <namespace> <path>`.
- Remember the golden rule: **no code changes unless explicitly requested.**
