build:
	cargo build

build-release:
	cargo build --release

python-install:
	cd tests && poetry install

python-lint:
	cd tests && poetry run ruff check

python-format:
	cd tests && poetry run ruff format

python-protos:
	cd tests && poetry run python -m grpc_tools.protoc -I ../protos \
		--python_out=hold/protos \
		--pyi_out=hold/protos \
		--grpc_python_out=hold/protos \
		../protos/hold.proto

regtest-start:
	git submodule init
	git submodule update
	chmod -R 777 regtest 2> /dev/null || true
	cd regtest && COMPOSE_PROFILES=ci ./start.sh
	mkdir regtest/data/cln2/plugins
	cp target/debug/hold regtest/data/cln2/plugins/
	docker exec boltz-cln-2 lightning-cli --regtest plugin stop /root/hold.sh
	rm -rf regtest/data/cln2/regtest/hold/
	docker exec boltz-cln-2 lightning-cli --regtest plugin start /root/.lightning/plugins/hold

	sleep 1
	docker exec boltz-cln-2 chmod 777 -R /root/.lightning/regtest/hold

	make python-protos

regtest-stop:
	cd regtest && ./stop.sh

db-start:
	docker run --name hold-db --rm -e POSTGRES_DB=hold -e POSTGRES_USER=hold -e POSTGRES_PASSWORD=hold \
		-d -p 5433:5432 postgres:14-alpine

db-stop:
	docker stop hold-db

integration-tests:
	cd tests && poetry run pytest
