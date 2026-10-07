.PHONY: build run package drivers test vet selfcheck
build:
	python3 tools/build.py
run: build
	./bin/superlink
package:
	python3 tools/build.py --package
drivers:
	python3 tools/build.py --all-drivers --skip-app
test:
	go test ./...
vet:
	go vet ./...
selfcheck:
	python3 -m unittest discover -s tools -p 'test_release_*.py' -v
	python3 tools/check-architecture.py
	go run ./tools/check-go-size
	python3 tools/verify-upstream.py
	python3 tools/verify-fyne.py
	python3 tools/verify-ui-assets.py
	go test ./...
	go test -tags gonavi_full_drivers ./internal/upstream/db ./cmd/driver-agent
	go test -race ./internal/application ./internal/infra/... ./internal/domain ./internal/ui ./cmd/release-sign
	go vet ./...
	go -C third_party/fyne test -race -tags=test ./internal/painter ./internal/cache
	go -C third_party/fyne test -race -tags=test ./container -run 'Test(CachedChildTheme|CachedFeatureState|DestroyedOverride|ThemeOverride)'
	python3 tools/build.py --all-drivers --package
