TODOS:
1. this is a full rescan of folder which for big folders is bad
2. the db map is keyed by path so if a folder gets renamed/moved it looks like ts new and gets reuploaded under a new uuid the fix:
content hasing, using something really cheap to store in a database and if a file is renamed you can tell because they are the same hashes
3. the bucket is not consulted at all, the cloud state is really just "what the db things is in the cloud"
so that means that if an object is deleted from s3 but the db row is still there its treated as up to date
and skipped
4. rewrite the tests what were made by AI
5. figure out what local constatin is and fix it (used ai to fix it for now)
6. sometimes a function in the sync-core crate just takes in a path to return something else, but more than one folder can have a file name so make it also require a namespace to

<!--This one is made by ai-->
7. Extract a "diffing" module before a full crate split. The real logic in sync.rs —
 files_to_upload and files_to_delete — is pure-ish logic that compares local scan
 results against cloud records. That's a clean seam. A diff.rs (or plan.rs) module
 that takes Vec<File> + Vec<File> and returns {upload: [...], delete: [...]} would:
 - make the core decision logic unit-testable without DB/S3 at all,
 - slim Sync down to just calling the scanner/db/diff, and
 - be the natural thing to extract into its own crate later if you want.
