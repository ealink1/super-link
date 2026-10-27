.PHONY: agents build-agents run test vet selfcheck rust-test

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

selfcheck:
	python3 tools/check-architecture.py
	go run ./tools/check-go-size
	python3 tools/verify-upstream.py
	python3 tools/verify-fyne.py
	python3 tools/verify-ui-assets.py
	python3 tools/verify-shell-assets.py
	go test ./...
	go test -tags superlink_full_drivers ./internal/upstream/db ./cmd/driver-agent
	go vet ./...
	cargo fmt --manifest-path rust/Cargo.toml --all -- --check
	cargo test --manifest-path rust/Cargo.toml
