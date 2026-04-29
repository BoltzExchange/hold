build:
	cargo build

build-release:
	cargo build --release

clippy:
	cargo clippy --all-targets --all-features -- -D warnings

python-install:
	cd tests-regtest && uv sync

python-lint:
	cd tests-regtest && uv run ruff check . ../tests

python-lint-fix:
	cd tests-regtest && uv run ruff check --fix . ../tests

python-format:
	cd tests-regtest && uv run ruff format . ../tests

python-protos:
	cd tests-regtest && uv run python -m grpc_tools.protoc -I ../protos \
		--python_out=hold/protos \
		--pyi_out=hold/protos \
		--grpc_python_out=hold/protos \
		../protos/hold.proto

regtest-start:
	cd regtest && COMPOSE_PROFILES=ci ./start.sh

regtest-setup:
	mkdir -p regtest/data/cln2/plugins
	cp target/debug/hold regtest/data/cln2/plugins/
	chmod 777 regtest/data/cln2/plugins/hold
	docker exec boltz-cln-2 lightning-cli --regtest --lightning-dir /app/lightning plugin stop /usr/local/bin/hold
	rm -rf regtest/data/cln2/regtest/hold/
	docker exec boltz-cln-2 lightning-cli --regtest --lightning-dir /app/lightning plugin start /app/lightning/plugins/hold

	make python-protos

regtest-stop:
	cd regtest && COMPOSE_PROFILES=ci ./stop.sh

db-start:
	docker run --name hold-db --rm -e POSTGRES_DB=hold -e POSTGRES_USER=hold -e POSTGRES_PASSWORD=hold \
		-d -p 5433:5432 postgres:17-alpine

db-stop:
	docker stop hold-db

integration-tests:
	cd tests-regtest && uv run pytest hold/

changelog:
	git-cliff -o CHANGELOG.md

binaries:
	docker buildx build . -o=build --target=binaries
	mv build/hold build/hold-linux-amd64
	docker buildx build . -o=build --target=binaries --platform linux/arm64
	mv build/hold build/hold-linux-arm64
	tar -czcf build/hold-linux-amd64.tar.gz build/hold-linux-amd64
	tar -czcf build/hold-linux-arm64.tar.gz build/hold-linux-arm64

.PHONY: build
