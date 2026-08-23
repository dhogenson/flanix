CREATE TABLE IF NOT EXISTS files (
    id uuid PRIMARY KEY,
    bucket_key TEXT NOT NULL,
    local_path TEXT NOT NULL,
    group_id uuid NOT NULL,
    created_at timestamp NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS groups (
    id uuid PRIMARY KEY,
    name TEXT NOT NULL,
    created_at timestamp NOT NULL DEFAULT NOW()
);
