ALTER TABLE files ADD FOREIGN KEY (namespace_id) REFERENCES namespaces(id);
ALTER TABLE namespaces ADD UNIQUE (name);
CREATE INDEX idx_files_namespace_id ON files(namespace_id);
CREATE INDEX idx_files_local_path ON files(local_path);