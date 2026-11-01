.PHONY: dev build test check docker clean

dev:
	./run.sh

build:
	bash site/build.sh

test:
	cargo test --workspace

check:
	cargo fmt --all -- --check
	cargo clippy --workspace --all-targets -- -D warnings

docker:
	docker compose up --build

clean:
	cargo clean
