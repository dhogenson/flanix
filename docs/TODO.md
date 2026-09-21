# TODO

Safety work identified in the production-readiness review of the sync flow.

## Critical

- [ ] Gate or remove the S3 "orphan" sweep in `push` (`sync.rs:223-235`). Right now any object in the bucket not listed in the DB is hard-deleted on every push. One DB reset (e.g. `docker compose down -v`), a shared/reused bucket, or a second install pointing at the same bucket empties the whole backup.
- [ ] Make downloads atomic (`s3.rs:236-239`). `File::create` truncates the local file before the stream arrives, so an interrupted/failed `pull` corrupts the pre-existing local copy. Use temp file + `sync_all` + `rename`, with a backup of the old file before overwrite.
- [ ] Add conflict detection/handling (`diff.rs:94-100`). Currently "bigger mtime wins": clock skew, two devices editing the same file, or a device deleting a file another device changed locally all silently destroy one side's data with no warning and no versioning.

## Major

- [ ] Reorder S3 mutations vs the DB transaction (`sync.rs:152-160` and commit at `164-214`). Deletes happen before the DB commit, so a commit failure leaves the DB believing an object still exists that is gone from the bucket, and `push` will never re-upload it (mtime+hash look "up to date").
- [ ] Verify integrity after upload (compare S3 object hash to local hash) before treating a file as synced; detect corrupted/truncated objects in the bucket instead of trusting the DB.
- [ ] Trash retention/config: `.flanix-trash` is permanently wiped after 30 days with no confirmation, no configurable retention, and no check that other devices have seen the deletion. Confirm before trashing a file whose local copy is newer than the cloud copy (mtime/hash check at `sync.rs:322-332`).
- [ ] `--dry-run` flag for `pull` (preview what would change/delete before doing it).
- [ ] Handle namespace/path moves safely. Changing the sync folder maps every old path to "removed", which deletes all cloud objects for that namespace on the next `push` (`sync.rs:144`, `158-160`).
- [ ] Make a single failed `fs::rename` during the trash pass not abort the whole `pull`.
- [ ] Recoverability for uploads: crash between PUT and DB commit leaves orphans that the sweep deletes next push; commit the DB transaction around/with the S3 writes.

## Minor

- [ ] Don't create the bucket as a side effect of `flanix config` (`sync.rs:27-37`, `s3.rs:104-110`).
- [ ] Surface symlinked files that are silently skipped by the scanner so users don't believe they are backed up.
- [ ] Handle equal-mtime/diverged-content files explicitly instead of silently skipping one direction (`diff.rs:38-40`, `98`).