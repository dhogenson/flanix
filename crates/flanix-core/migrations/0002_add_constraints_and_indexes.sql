ALTER TABLE files ADD FOREIGN KEY (namespace_id) REFERENCES namespaces(id);
ALTER TABLE files ADD CONSTRAINT files_namespace_id_local_path_key UNIQUE (namespace_id, local_path);
ALTER TABLE namespaces ADD UNIQUE (name);