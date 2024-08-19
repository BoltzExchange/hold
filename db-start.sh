docker run --name hold-db --rm -e POSTGRES_DB=hold -e POSTGRES_USER=hold -e POSTGRES_PASSWORD=hold -d -p 5433:5432 postgres:14-alpine
