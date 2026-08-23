docker compose -f docker-compose.test.yaml up -d --wait

export DATABASE_URL=postgres://user:password@localhost/mydb
export AWS_ENDPOINT_URL="http://localhost:4566";
export AWS_DEFAULT_REGION="us-east-1";
export AWS_ACCESS_KEY_ID="test";
export AWS_SECRET_ACCESS_KEY="test";

# sqlx compile-time macros need the schema in the live database
for file in crates/*/migrations/*.sql; do
  docker compose -f docker-compose.test.yaml exec -T test-db \
    psql -v ON_ERROR_STOP=1 -U user -d mydb < "$file"
done

cargo test

docker compose -f docker-compose.test.yaml down -v
