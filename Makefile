.PHONY: agents build-agents run test vet selfcheck rust-test live-pg live-mysql

# 驱动 agent：SQLite 随包构建，其余按需
build-agents:
	python3 tools/build.py --skip-app
agents:
	python3 tools/build.py --all-drivers --skip-app

# Rust 应用（rewrite 分支的正式程序）
run:
	cargo run --manifest-path rust/Cargo.toml -p superlink

test:
	go test ./...
	go test -tags superlink_full_drivers ./internal/upstream/db ./cmd/driver-agent
	go vet ./...
	cargo fmt --manifest-path rust/Cargo.toml --all -- --check
	cargo clippy --manifest-path rust/Cargo.toml --all-targets
	cargo test --manifest-path rust/Cargo.toml

vet:
	go vet ./...
	cargo clippy --manifest-path rust/Cargo.toml --all-targets

# 真实 PG/Kingbase 冒烟：SUPERLINK_TEST_PG_KIND=kingbase SUPERLINK_TEST_PG_PASSWORD=... make live-pg
live-pg:
	cd rust && SUPERLINK_TEST_PG=1 cargo test -p superlink-drivers passes_the_battery -- --nocapture

# 真实 MySQL 冒烟：SUPERLINK_TEST_MYSQL_PASSWORD=... make live-mysql
live-mysql:
	cd rust && SUPERLINK_TEST_MYSQL=1 cargo test -p superlink-drivers passes_the_battery_on_a_live_mysql_server -- --nocapture

selfcheck:
	python3 tools/check-architecture.py
	go run ./tools/check-go-size
	python3 tools/verify-upstream.py
	python3 tools/verify-ui-assets.py
	python3 tools/verify-shell-assets.py
	go test ./...
	go test -tags superlink_full_drivers ./internal/upstream/db ./cmd/driver-agent
	go vet ./...
	cargo fmt --manifest-path rust/Cargo.toml --all -- --check
	cargo test --manifest-path rust/Cargo.toml
