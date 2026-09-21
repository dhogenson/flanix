-- Tombstones for files deleted on the cloud so `pull` can propagate
-- deletions to other machines without re-uploading deleted files as
-- "new" untracked files.
CREATE TABLE IF NOT EXISTS deleted_files (
    id UUID PRIMARY KEY,
    namespace_id UUID NOT NULL,
    local_path TEXT NOT NULL,
    deleted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT deleted_files_namespace_id_local_path_key UNIQUE (namespace_id, local_path)
);

ALTER TABLE deleted_files ADD FOREIGN KEY (namespace_id) REFERENCES namespaces(id);