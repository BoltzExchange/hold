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

regtest-start:
	git submodule init
	git submodule update
	chmod -R 777 regtest
	cd regtest && COMPOSE_PROFILES=ci ./start.sh
	cd ..
	mkdir regtest/data/cln1/plugins
	cp target/debug/hold regtest/data/cln1/plugins/
	docker exec boltz-cln-1 lightning-cli --regtest plugin stop /root/hold.sh
	rm -rf regtest/data/cln1/regtest/hold/
	docker exec boltz-cln-1 lightning-cli --regtest plugin start /root/.lightning/plugins/hold

regtest-stop:
	cd regtest && ./stop.sh

db-start:
	docker run --name hold-db --rm -e POSTGRES_DB=hold -e POSTGRES_USER=hold -e POSTGRES_PASSWORD=hold \
		-d -p 5433:5432 postgres:14-alpine

db-stop:
	docker stop hold-db

integration-tests:
	cd tests && poetry run pytest
