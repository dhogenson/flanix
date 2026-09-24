# TODO

- [ ] Don't create the bucket as a side effect of `flanix config` (`sync.rs:27-37`, `s3.rs:104-110`).
- [ ] Surface symlinked files that are silently skipped by the scanner so users don't believe they are backed up.
- [ ] Handle equal-mtime/diverged-content files explicitly instead of silently skipping one direction (`diff.rs:38-40`, `98`).
