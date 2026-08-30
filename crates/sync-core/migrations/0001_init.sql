CREATE TABLE IF NOT EXISTS files (
    id UUID PRIMARY KEY,
    bucket_key UUID NOT NULL,
    local_path TEXT NOT NULL,
    modified_at TIMESTAMPTZ NOT NULL,
    namespace_id UUID NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS namespaces (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT NOW()
);
