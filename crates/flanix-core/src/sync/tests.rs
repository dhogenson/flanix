use super::*;
use crate::db::truncate_to_micros;
use anyhow::Result;
use chrono::{DateTime, TimeDelta, Utc};
use flanix_indexing::LocalFile;
use sqlx::PgPool;
use std::fs;
use std::fs::File;
use tempfile::TempDir;

// Scan a directory with the indexing crate, returning relative paths.
fn local_files(dir: &TempDir) -> Result<Vec<LocalFile>> {
    Indexer::new(dir.path().to_path_buf()).scan()
}

// Build a Sync wired to the isolated #[sqlx::test] database. The bucket is
// not exercised by files_to_upload, so a real client is only constructed to
// satisfy the struct's fields.
async fn test_sync(pool: PgPool) -> Sync {
    let database = Database::from_pool(pool).await;
    let mut bucket_config = Config::new().unwrap();
    bucket_config.bucket_name = "test-bucket".to_string();
    bucket_config.aws_default_region = "us-west-2".to_string();
    let bucket = Bucket::new(&bucket_config).await;
    Sync {
        database,
        bucket,
        config: Config::new().unwrap(),
    }
}

async fn create_namespace(db: &Database, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    db.create_namespace(id, name).await.unwrap();
    id
}

// Insert a record that simulates a file already stored in the "cloud" for
// the given namespace, with a specific modification time. The hash matches
// write_file's "test content", so equal-mtime files are considered up to date.
async fn insert_cloud_file(
    db: &Database,
    local_path: &str,
    modified_at: DateTime<Utc>,
    namespace_id: Uuid,
) {
    sqlx::query(
        "INSERT INTO files (id, bucket_key, local_path, modified_at, file_hash, namespace_id)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(Uuid::new_v4())
    .bind("test-bucket-key")
    .bind(local_path)
    .bind(modified_at)
    .bind(blake3::hash(b"test content").to_hex().to_string())
    .bind(namespace_id)
    .execute(&db.pool)
    .await
    .unwrap();
}

fn write_file(dir: &TempDir, name: &str) -> Result<PathBuf> {
    let path = dir.path().join(name);
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(&path, "test content")?;
    Ok(path)
}

fn file_modified(path: &PathBuf) -> Result<DateTime<Utc>> {
    let meta = fs::metadata(path)?;
    Ok(meta.modified()?.into())
}

fn sorted(mut paths: Vec<PathBuf>) -> Vec<PathBuf> {
    paths.sort();
    paths
}

fn ceil_to_db_precision(t: DateTime<Utc>) -> DateTime<Utc> {
    let sub_micro_ns = (t.timestamp_subsec_nanos() % 1000) as i64;
    if sub_micro_ns == 0 {
        t
    } else {
        t + TimeDelta::nanoseconds(1000 - sub_micro_ns)
    }
}

#[sqlx::test]
async fn skips_untouched_file_after_previous_upload(pool: PgPool) -> Result<()> {
    let sync = test_sync(pool.clone()).await;
    let dir = tempfile::tempdir()?;
    let a = write_file(&dir, "a.txt")?;
    let mtime = file_modified(&a)?;
    let ns_id = create_namespace(&sync.database, "ns-reupload").await;

    // Simulate a prior add_file: the DB stores the local mtime truncated down
    // to microsecond precision (not ceil'ed). This is what actually happens.
    insert_cloud_file(&sync.database, "a.txt", truncate_to_micros(mtime), ns_id).await;

    let local_files = local_files(&dir)?;
    let to_upload = sync.files_to_upload("ns-reupload", &local_files).await?;

    // Even though local ns mtime is strictly greater than the stored cloud
    // mtime, they are equal at DB precision, so it must not re-upload.
    assert!(to_upload.is_empty());
    Ok(())
}

#[sqlx::test]
async fn selects_files_not_in_cloud(pool: PgPool) -> Result<()> {
    let sync = test_sync(pool.clone()).await;
    let dir = tempfile::tempdir()?;
    write_file(&dir, "a.txt")?;
    write_file(&dir, "sub/b.txt")?;

    create_namespace(&sync.database, "ns-new").await;

    let local_files = local_files(&dir)?;
    let to_upload: Vec<PathBuf> = sync
        .files_to_upload("ns-new", &local_files)
        .await?
        .into_iter()
        .map(|f| f.file_path)
        .collect();

    // Neither file is in the cloud, so both must be selected. Paths are
    // stored relative to the scanned folder.
    assert_eq!(
        sorted(to_upload),
        sorted(vec![PathBuf::from("a.txt"), PathBuf::from("sub/b.txt")])
    );
    Ok(())
}

#[sqlx::test]
async fn skips_files_up_to_date(pool: PgPool) -> Result<()> {
    let sync = test_sync(pool.clone()).await;
    let dir = tempfile::tempdir()?;
    let a = write_file(&dir, "a.txt")?;
    let mtime = file_modified(&a)?;
    let ns_id = create_namespace(&sync.database, "ns-skip").await;

    // Cloud copy is not older than the local file -> up to date, skip.
    insert_cloud_file(&sync.database, "a.txt", ceil_to_db_precision(mtime), ns_id).await;

    let local_files = local_files(&dir)?;
    let to_upload = sync.files_to_upload("ns-skip", &local_files).await?;

    assert!(to_upload.is_empty());
    Ok(())
}

#[sqlx::test]
async fn skips_when_cloud_copy_is_newer(pool: PgPool) -> Result<()> {
    let sync = test_sync(pool.clone()).await;
    let dir = tempfile::tempdir()?;
    let a = write_file(&dir, "a.txt")?;
    let mtime = file_modified(&a)?;
    let ns_id = create_namespace(&sync.database, "ns-cloud-newer").await;

    // Cloud copy was modified after the local file -> no re-upload.
    insert_cloud_file(&sync.database, "a.txt", mtime + TimeDelta::hours(1), ns_id).await;

    let local_files = local_files(&dir)?;
    let to_upload = sync.files_to_upload("ns-cloud-newer", &local_files).await?;

    assert!(to_upload.is_empty());
    Ok(())
}

#[sqlx::test]
async fn selects_when_local_copy_is_newer(pool: PgPool) -> Result<()> {
    let sync = test_sync(pool.clone()).await;
    let dir = tempfile::tempdir()?;
    let a = write_file(&dir, "a.txt")?;
    let mtime = file_modified(&a)?;
    let ns_id = create_namespace(&sync.database, "ns-local-newer").await;

    // Cloud copy is older than the local file -> re-upload.
    insert_cloud_file(&sync.database, "a.txt", mtime - TimeDelta::hours(1), ns_id).await;

    let local_files = local_files(&dir)?;
    let to_upload: Vec<PathBuf> = sync
        .files_to_upload("ns-local-newer", &local_files)
        .await?
        .into_iter()
        .map(|f| f.file_path)
        .collect();

    assert_eq!(to_upload, vec![PathBuf::from("a.txt")]);
    Ok(())
}

#[sqlx::test]
async fn mixes_upload_and_skip_decisions(pool: PgPool) -> Result<()> {
    let sync = test_sync(pool.clone()).await;
    let dir = tempfile::tempdir()?;
    write_file(&dir, "new.txt")?; // no cloud copy -> upload
    let older = write_file(&dir, "older.txt")?; // local newer -> upload
    let same = write_file(&dir, "same.txt")?; // equal -> skip
    let cloud_newer = write_file(&dir, "cloud-newer.txt")?; // cloud newer -> skip
    let ns_id = create_namespace(&sync.database, "ns-mixed").await;

    insert_cloud_file(
        &sync.database,
        "older.txt",
        file_modified(&older)? - TimeDelta::hours(1),
        ns_id,
    )
    .await;
    insert_cloud_file(
        &sync.database,
        "same.txt",
        ceil_to_db_precision(file_modified(&same)?),
        ns_id,
    )
    .await;
    insert_cloud_file(
        &sync.database,
        "cloud-newer.txt",
        file_modified(&cloud_newer)? + TimeDelta::hours(1),
        ns_id,
    )
    .await;

    let local_files = local_files(&dir)?;
    let to_upload: Vec<PathBuf> = sync
        .files_to_upload("ns-mixed", &local_files)
        .await?
        .into_iter()
        .map(|f| f.file_path)
        .collect();

    assert_eq!(
        sorted(to_upload),
        sorted(vec![PathBuf::from("new.txt"), PathBuf::from("older.txt")])
    );
    Ok(())
}

#[sqlx::test]
async fn re_uploads_when_content_changes_but_mtime_is_preserved(pool: PgPool) -> Result<()> {
    let sync = test_sync(pool.clone()).await;
    let dir = tempfile::tempdir()?;
    let a = write_file(&dir, "a.txt")?;
    let original_mtime = file_modified(&a)?;
    let ns_id = create_namespace(&sync.database, "ns-same-mtime").await;

    // Cloud copy matches the current content at the same mtime -> up to date.
    insert_cloud_file(
        &sync.database,
        "a.txt",
        truncate_to_micros(original_mtime),
        ns_id,
    )
    .await;
    let initial_files = local_files(&dir)?;
    assert!(
        sync.files_to_upload("ns-same-mtime", &initial_files)
            .await?
            .is_empty()
    );

    // Content changes but the mtime is preserved (e.g. tools that stamp it).
    fs::write(&a, "different content")?;
    File::open(&a)?.set_modified(original_mtime.into())?;

    let local_files = local_files(&dir)?;
    let to_upload: Vec<PathBuf> = sync
        .files_to_upload("ns-same-mtime", &local_files)
        .await?
        .into_iter()
        .map(|f| f.file_path)
        .collect();

    assert_eq!(to_upload, vec![PathBuf::from("a.txt")]);
    Ok(())
}

#[sqlx::test]
async fn pulls_cloud_only_files(pool: PgPool) -> Result<()> {
    let sync = test_sync(pool.clone()).await;
    let dir = tempfile::tempdir()?;
    write_file(&dir, "a.txt")?;
    let ns_id = create_namespace(&sync.database, "ns-pull-new").await;

    // a.txt exists locally and in the cloud at the same mtime -> no download.
    let a_mtime = file_modified(&dir.path().join("a.txt"))?;
    insert_cloud_file(&sync.database, "a.txt", truncate_to_micros(a_mtime), ns_id).await;
    // b.txt exists only in the cloud -> must be downloaded.
    insert_cloud_file(&sync.database, "b.txt", Utc::now(), ns_id).await;

    let local_files = local_files(&dir)?;
    let to_download = sync.files_to_pull("ns-pull-new", &local_files).await?;

    assert_eq!(to_download, vec![PathBuf::from("b.txt")]);
    Ok(())
}

#[sqlx::test]
async fn skips_up_to_date_files_on_pull(pool: PgPool) -> Result<()> {
    let sync = test_sync(pool.clone()).await;
    let dir = tempfile::tempdir()?;
    let a = write_file(&dir, "a.txt")?;
    let mtime = file_modified(&a)?;
    let ns_id = create_namespace(&sync.database, "ns-pull-skip").await;

    // Cloud copy is not newer than the local file -> up to date, skip.
    insert_cloud_file(&sync.database, "a.txt", truncate_to_micros(mtime), ns_id).await;

    let local_files = local_files(&dir)?;
    let to_download = sync.files_to_pull("ns-pull-skip", &local_files).await?;

    assert!(to_download.is_empty());
    Ok(())
}

#[sqlx::test]
async fn pulls_when_cloud_copy_is_newer(pool: PgPool) -> Result<()> {
    let sync = test_sync(pool.clone()).await;
    let dir = tempfile::tempdir()?;
    let a = write_file(&dir, "a.txt")?;
    let mtime = file_modified(&a)?;
    let ns_id = create_namespace(&sync.database, "ns-pull-cloud-newer").await;

    // Cloud copy was modified after the local file -> download.
    insert_cloud_file(&sync.database, "a.txt", mtime + TimeDelta::hours(1), ns_id).await;

    let local_files = local_files(&dir)?;
    let to_download = sync
        .files_to_pull("ns-pull-cloud-newer", &local_files)
        .await?;

    assert_eq!(to_download, vec![PathBuf::from("a.txt")]);
    Ok(())
}

#[sqlx::test]
async fn skips_pull_when_local_copy_is_newer(pool: PgPool) -> Result<()> {
    let sync = test_sync(pool.clone()).await;
    let dir = tempfile::tempdir()?;
    let a = write_file(&dir, "a.txt")?;
    let mtime = file_modified(&a)?;
    let ns_id = create_namespace(&sync.database, "ns-pull-local-newer").await;

    // Cloud copy is older than the local file -> no download.
    insert_cloud_file(&sync.database, "a.txt", mtime - TimeDelta::hours(1), ns_id).await;

    let local_files = local_files(&dir)?;
    let to_download = sync
        .files_to_pull("ns-pull-local-newer", &local_files)
        .await?;

    assert!(to_download.is_empty());
    Ok(())
}

#[sqlx::test]
async fn pushes_local_conflicts(pool: PgPool) -> Result<()> {
    let sync = test_sync(pool.clone()).await;
    let dir = tempfile::tempdir()?;
    let a = write_file(&dir, "a.txt")?;
    let mtime = file_modified(&a)?;
    let ns_id = create_namespace(&sync.database, "ns-conflict-same").await;

    // "test content" hash is what insert_cloud_file writes.
    insert_cloud_file(&sync.database, "a.txt", mtime + TimeDelta::hours(1), ns_id).await;

    let local_files = local_files(&dir)?;
    let to_download = sync.files_to_pull("ns-conflict-same", &local_files).await?;
    assert_eq!(to_download, vec![PathBuf::from("a.txt")]);

    // Same content as the cloud copy -> refreshing the mtime is safe;
    // overwriting would not destroy anything, so it's not a conflict.
    let conflicts = sync
        .files_to_conflict("ns-conflict-same", &local_files, &to_download)
        .await?;
    assert!(conflicts.is_empty());
    Ok(())
}

#[sqlx::test]
async fn flags_conflict_when_local_content_differs(pool: PgPool) -> Result<()> {
    let sync = test_sync(pool.clone()).await;
    let dir = tempfile::tempdir()?;
    write_file(&dir, "a.txt")?; // local => "test content"
    let ns_id = create_namespace(&sync.database, "ns-conflict-differ").await;

    // Cloud copy is newer and holds different content.
    sqlx::query(
        "INSERT INTO files (id, bucket_key, local_path, modified_at, file_hash, namespace_id)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(Uuid::new_v4())
    .bind("test-bucket-key")
    .bind("a.txt")
    .bind(Utc::now())
    .bind(blake3::hash(b"cloud content").to_hex().to_string())
    .bind(ns_id)
    .execute(&sync.database.pool)
    .await
    .unwrap();

    let local_files = local_files(&dir)?;
    let to_download = sync
        .files_to_pull("ns-conflict-differ", &local_files)
        .await?;
    assert_eq!(to_download, vec![PathBuf::from("a.txt")]);

    // Downloading would overwrite the divergent local edition -> conflict.
    let conflicts = sync
        .files_to_conflict("ns-conflict-differ", &local_files, &to_download)
        .await?;
    assert_eq!(conflicts, vec![PathBuf::from("a.txt")]);
    Ok(())
}

#[sqlx::test]
async fn no_conflict_when_path_not_downloaded(pool: PgPool) -> Result<()> {
    let sync = test_sync(pool.clone()).await;
    let dir = tempfile::tempdir()?;
    write_file(&dir, "a.txt")?; // local => "test content"
    let ns_id = create_namespace(&sync.database, "ns-conflict-idle").await;

    // Cloud copy holds different content but is OLDER than the local copy, so
    // nothing gets downloaded. Even though the hashes differ, there's no
    // overwrite in the plan, so it is not a conflict.
    sqlx::query(
        "INSERT INTO files (id, bucket_key, local_path, modified_at, file_hash, namespace_id)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(Uuid::new_v4())
    .bind("test-bucket-key")
    .bind("a.txt")
    .bind(Utc::now() - TimeDelta::hours(1))
    .bind(blake3::hash(b"cloud content").to_hex().to_string())
    .bind(ns_id)
    .execute(&sync.database.pool)
    .await
    .unwrap();

    let local_files = local_files(&dir)?;
    let to_download = sync.files_to_pull("ns-conflict-idle", &local_files).await?;
    assert!(to_download.is_empty());

    let conflicts = sync
        .files_to_conflict("ns-conflict-idle", &local_files, &to_download)
        .await?;
    assert!(conflicts.is_empty());
    Ok(())
}

#[sqlx::test]
async fn lists_untracked_local_files_to_backup(pool: PgPool) -> Result<()> {
    let sync = test_sync(pool.clone()).await;
    let dir = tempfile::tempdir()?;
    let tracked = write_file(&dir, "tracked.txt")?;
    let mtime = file_modified(&tracked)?;
    let ns_id = create_namespace(&sync.database, "ns-pull-delete").await;

    // tracked.txt exists in the cloud -> not backed up.
    insert_cloud_file(
        &sync.database,
        "tracked.txt",
        truncate_to_micros(mtime),
        ns_id,
    )
    .await;
    // untracked.txt only exists locally -> must be backed up by pull.
    write_file(&dir, "untracked.txt")?;

    let local_files = local_files(&dir)?;
    let to_backup = sync.files_to_backup("ns-pull-delete", &local_files).await?;

    assert_eq!(to_backup, vec![PathBuf::from("untracked.txt")]);
    Ok(())
}

#[sqlx::test]
async fn skips_tombstoned_files_when_backing_up(pool: PgPool) -> Result<()> {
    let sync = test_sync(pool.clone()).await;
    let dir = tempfile::tempdir()?;
    write_file(&dir, "new.txt")?;
    write_file(&dir, "deleted.txt")?;

    let ns_id = create_namespace(&sync.database, "ns-tombstone").await;

    // new.txt is untracked and not tombstoned -> backed up.
    // deleted.txt exists locally but has a tombstone -> must NOT be backed up.
    sqlx::query(
        "INSERT INTO deleted_files (id, namespace_id, local_path, deleted_at)
         VALUES ($1, $2, $3, NOW())",
    )
    .bind(Uuid::new_v4())
    .bind(ns_id)
    .bind("deleted.txt")
    .execute(&sync.database.pool)
    .await
    .unwrap();

    let local_files = local_files(&dir)?;
    let to_backup = sync.files_to_backup("ns-tombstone", &local_files).await?;

    assert_eq!(to_backup, vec![PathBuf::from("new.txt")]);
    Ok(())
}

#[sqlx::test]
async fn re_upload_clears_tombstone(pool: PgPool) -> Result<()> {
    let sync = test_sync(pool.clone()).await;
    let ns_id = create_namespace(&sync.database, "ns-reupload-tombstone").await;

    sync.database
        .add_tombstone(&sync.database.pool, ns_id, "x.txt")
        .await?;

    let mut tx = sync.database.begin().await?;
    sync.database
        .remove_tombstone(&mut *tx, ns_id, "x.txt")
        .await?;
    tx.commit().await?;

    let tombstones = sync
        .database
        .get_tombstones("ns-reupload-tombstone")
        .await?;
    assert!(tombstones.is_empty());
    Ok(())
}

fn local_file(path: &str) -> LocalFile {
    LocalFile {
        file_path: PathBuf::from(path),
        modified_time: Utc::now(),
        file_size: 0,
        file_hash: None,
    }
}

#[test]
fn deletes_only_files_the_device_previously_had() {
    let previously_had = [
        PathBuf::from("mine.txt"),
        PathBuf::from("gone.txt"),
        PathBuf::from("also-deleted.txt"),
    ]
    .into_iter()
    .collect::<HashSet<_>>();

    // The disk retains mine.txt, so it is not a deletion. Another device's
    // file is in the cloud but was never on this device -> never deleted.
    let local_files = vec![local_file("mine.txt"), local_file("new.txt")];
    let cloud_paths = [
        PathBuf::from("mine.txt"),
        PathBuf::from("gone.txt"),
        PathBuf::from("also-deleted.txt"),
        PathBuf::from("someone-else.txt"),
    ]
    .into_iter()
    .collect::<HashSet<_>>();

    let deletes = Sync::files_to_delete(&previously_had, &local_files, &cloud_paths);

    assert_eq!(
        sorted(deletes.into_iter().collect()),
        sorted(vec![
            PathBuf::from("gone.txt"),
            PathBuf::from("also-deleted.txt")
        ])
    );
}

#[test]
fn skips_deletion_when_cloud_copy_already_gone() {
    let previously_had = [PathBuf::from("was-mine.txt")]
        .into_iter()
        .collect::<HashSet<_>>();

    // The cloud copy no longer exists (another device already deleted it), so
    // there is nothing left to delete or tombstone.
    let deletes = Sync::files_to_delete(
        &previously_had,
        &[local_file("other.txt")],
        &HashSet::from([PathBuf::from("other.txt")]),
    );

    assert!(deletes.is_empty());
}

fn paths_0_9() -> HashSet<PathBuf> {
    (0..10)
        .map(|i| PathBuf::from(format!("f{i}.txt")))
        .collect()
}

#[test]
fn mass_delete_guard_blocks_folder_repoint() {
    let previously_had = paths_0_9();
    let cloud_paths = previously_had.clone();

    // The disk is empty — the sync folder was moved/repointed, so every old
    // path looks like a deletion and every cloud object would be wiped.
    let deletes = Sync::files_to_delete(&previously_had, &[], &cloud_paths);
    assert_eq!(deletes.len(), 10);

    assert!(Sync::is_mass_delete(
        &deletes,
        &previously_had,
        &cloud_paths
    ));
}

#[test]
fn mass_delete_guard_allows_real_single_deletion() {
    let previously_had = paths_0_9();
    let cloud_paths = previously_had.clone();
    let local_files: Vec<LocalFile> = (0..9).map(|i| local_file(&format!("f{i}.txt"))).collect();

    // One file genuinely deleted: 1/10 of both sets is a normal deletion.
    let deletes = Sync::files_to_delete(&previously_had, &local_files, &cloud_paths);
    assert_eq!(deletes.len(), 1);

    assert!(!Sync::is_mass_delete(
        &deletes,
        &previously_had,
        &cloud_paths
    ));
}

#[test]
fn mass_delete_guard_not_triggered_by_small_shared_namespace() {
    // A device that only ever had 1 of 10 cloud files deleting it is 100% of
    // this device's files but only 10% of the namespace — deleting one's own
    // file must not be blocked just because the namespace is shared.
    let previously_had = HashSet::from([PathBuf::from("mine.txt")]);
    let cloud_paths: HashSet<PathBuf> = [PathBuf::from("mine.txt")]
        .into_iter()
        .chain((0..9).map(|i| PathBuf::from(format!("shared-{i}.txt"))))
        .collect();

    let deletes = Sync::files_to_delete(&previously_had, &[], &cloud_paths);
    assert_eq!(deletes, previously_had);

    assert!(!Sync::is_mass_delete(
        &deletes,
        &previously_had,
        &cloud_paths
    ));
}

#[test]
fn move_detection_spares_renamed_file() {
    // old.txt was renamed to new.txt: same content, different path.
    let deletes = HashSet::from([PathBuf::from("old.txt")]);
    let cloud_hashes = HashMap::from([(PathBuf::from("old.txt"), Some("hash-a".to_string()))]);
    let local_hashes = HashMap::from([("hash-a".to_string(), vec![PathBuf::from("new.txt")])]);

    let remaining = Sync::filter_moved_deletes(&deletes, &cloud_hashes, &local_hashes);
    assert!(remaining.is_empty());
}

#[test]
fn move_detection_keeps_real_deletion() {
    // old.txt is gone and no local file carries that content anymore.
    let deletes = HashSet::from([PathBuf::from("old.txt")]);
    let cloud_hashes = HashMap::from([(PathBuf::from("old.txt"), Some("hash-a".to_string()))]);
    let local_hashes = HashMap::from([("hash-b".to_string(), vec![PathBuf::from("new.txt")])]);

    let remaining = Sync::filter_moved_deletes(&deletes, &cloud_hashes, &local_hashes);
    assert_eq!(remaining, deletes);
}

#[test]
fn move_detection_ignores_unhashed_cloud_rows() {
    // A path recorded before content hashing existed has no hash to compare,
    // so we can't prove a move — treat it as a real deletion.
    let deletes = HashSet::from([PathBuf::from("old.txt")]);
    let cloud_hashes = HashMap::from([(PathBuf::from("old.txt"), None)]);
    let local_hashes = HashMap::from([("hash-a".to_string(), vec![PathBuf::from("new.txt")])]);

    let remaining = Sync::filter_moved_deletes(&deletes, &cloud_hashes, &local_hashes);
    assert_eq!(remaining, deletes);
}

#[test]
fn confirms_only_on_yes() {
    assert!(Sync::confirm_yes("y"));
    assert!(Sync::confirm_yes("Y"));
    assert!(Sync::confirm_yes("yes"));
    assert!(Sync::confirm_yes("  YES  "));

    assert!(!Sync::confirm_yes("n"));
    assert!(!Sync::confirm_yes(""));
    assert!(!Sync::confirm_yes("maybe"));
    assert!(!Sync::confirm_yes("yep"));
}

#[sqlx::test]
async fn devices_seen_since_requires_every_device(pool: PgPool) -> Result<()> {
    let database = Database::from_pool(pool).await;

    // No registered devices -> vacuously all seen.
    assert!(database.devices_seen_since(Utc::now()).await?);

    let device = Uuid::new_v4();
    database.register_device(device, "device-1").await?;
    database.register_device(Uuid::new_v4(), "device-2").await?;

    // Both just connected, so a timestamp in the past is covered.
    assert!(
        database
            .devices_seen_since(Utc::now() - TimeDelta::hours(1))
            .await?
    );
    assert!(
        !database
            .devices_seen_since(Utc::now() + TimeDelta::hours(1))
            .await?
    );

    // Age one device out to simulate a machine that hasn't pulled recently.
    sqlx::query("UPDATE devices SET last_seen_at = $1 WHERE id = $2")
        .bind(Utc::now() - TimeDelta::days(40))
        .bind(device)
        .execute(&database.pool)
        .await?;

    assert!(
        !database
            .devices_seen_since(Utc::now() - TimeDelta::days(10))
            .await?
    );
    Ok(())
}

#[sqlx::test]
async fn prunes_tombstone_only_after_every_device_synced(pool: PgPool) -> Result<()> {
    let database = Database::from_pool(pool).await;
    let ns_id = create_namespace(&database, "ns-device-prune").await;

    let device = Uuid::new_v4();
    database.register_device(device, "device-1").await?;

    // A file deleted 20 days ago; the device last connected 1 day before that
    // deletion, so it never saw it.
    let deleted_at = Utc::now() - TimeDelta::days(20);
    database
        .add_tombstone(&database.pool, ns_id, "x.txt")
        .await?;
    sqlx::query(
        "UPDATE deleted_files SET deleted_at = $1 WHERE namespace_id = $2 AND local_path = 'x.txt'",
    )
    .bind(deleted_at)
    .bind(ns_id)
    .execute(&database.pool)
    .await?;
    sqlx::query("UPDATE devices SET last_seen_at = $1 WHERE id = $2")
        .bind(deleted_at - TimeDelta::days(1))
        .bind(device)
        .execute(&database.pool)
        .await?;

    // Past the retention window but the device hasn't synced since -> kept.
    let pruned = database
        .prune_tombstones("ns-device-prune", Utc::now() - TimeDelta::days(15))
        .await?;
    assert_eq!(pruned, 0);
    assert_eq!(database.get_tombstones("ns-device-prune").await?.len(), 1);

    // The device pulls today -> every device has now seen the deletion.
    database.register_device(device, "device-1").await?;
    let pruned = database
        .prune_tombstones("ns-device-prune", Utc::now() - TimeDelta::days(15))
        .await?;
    assert_eq!(pruned, 1);
    assert!(database.get_tombstones("ns-device-prune").await?.is_empty());

    Ok(())
}
