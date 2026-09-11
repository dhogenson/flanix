# Quick Start

## Prerequisites

- Rust toolchain (rustup)
- PostgreSQL 17+
- An S3-compatible storage backend (see [Storage Backend](#storage-backend) below)

## Build & Install

```sh
git clone https://github.com/dhogenson/flanix.git
cd flanix
cargo build --release
```

The binary will be at `target/release/flanix`. You can copy it somewhere on your `$PATH`:

```sh
cp target/release/flanix ~/.local/bin/
```

## Storage Backend

Flanix needs an S3-compatible object store and a PostgreSQL database. You have a few options for the S3 side.

### Option A: Docker Compose (floci mock — easiest for local dev)

The included `docker-compose.yaml` runs a local S3 mock (floci) on port 4566 and PostgreSQL on port 5432:

```sh
docker compose up -d
```

This gives you everything you need with zero configuration. The default credentials are `test`/`test`, bucket `test`, and the database is `postgres://user:password@localhost/mydb`.

### Option B: Garage (self-hosted, production-ready)

[Garage](https://garagehq.deuxfleurs.fr/) is an open-source distributed storage system with S3 API support. Flanix is explicitly compatible with Garage (path-style requests, buffered uploads).

1. Follow the [Garage installation guide](https://garagehq.deuxfleurs.fr/doc/quick-start/) to set up a Garage node.

2. Create a bucket and access key:

```sh
# Create a bucket
garage bucket create my-bucket

# Create an access key
garage key create my-key
# Note the Access Key ID and Secret Key from the output
```

3. Grant the key access to the bucket:

```sh
garage key allow --bucket my-bucket --read --write my-key
```

4. Point Flanix at your Garage endpoint (see [Configuration](#configuration) below). The endpoint will be something like `http://your-garage-node:3900`.

### Option C: Regular AWS S3 (or any S3-compatible service)

Use any S3 bucket you have access to. You can rely on the standard AWS credential chain (env vars, `~/.aws/credentials`, IAM roles) — just make sure the env vars `AWS_ENDPOINT_URL`, `AWS_DEFAULT_REGION`, `AWS_ACCESS_KEY_ID`, and `AWS_SECRET_ACCESS_KEY` are set, or configure them in the Flanix config file.

## Database

Flanix requires a running PostgreSQL instance. If you used Docker Compose above, this is already handled. Otherwise, create a database:

```sh
createdb flanix
```

Migrations are run automatically on startup.

## Configuration

Flanix stores its config at `~/.config/hogenson/flanix/config.toml` (Linux). Create it if it doesn't exist:

```toml
bucket_name = "my-bucket"
aws_endpoint = "http://localhost:3900"
aws_default_region = "us-east-1"
aws_access_key_id = "your-access-key"
aws_secret_access_key = "your-secret-key"
database_url = "postgres://user:password@localhost/flanix"
max_database_connections = 5
```

If the file doesn't exist, Flanix creates it with local dev defaults (floci on `localhost:4566`).

## Usage

### Add a namespace

Register a local directory to be synced. The namespace name must not contain `/`, `..`, or null bytes.

```sh
flanix add my-documents /home/user/documents
```

### Push (upload)

Sync your local changes to the cloud:

```sh
flanix push my-documents
```

### Pull (download)

Sync cloud changes down to your local machine:

```sh
flanix pull my-documents
```

## Full Example (Docker Compose)

```sh
# Start the backend
docker compose up -d

# Build flanix
cargo build --release

# Add a folder
./target/release/flanix add notes ~/notes

# Push it
./target/release/flanix push notes

# On another machine (with the same config pointing at the same backend)
./target/release/flanix pull notes
```
