package update

import (
	"archive/zip"
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"runtime"
	"testing"

	"github.com/ealink1/super-link/internal/infra/instance"
	"github.com/ealink1/super-link/internal/infra/state"
)

func writeZip(t *testing.T, names []string, modes []os.FileMode) string {
	t.Helper()
	file, err := os.Create(filepath.Join(t.TempDir(), "update.zip"))
	if err != nil {
		t.Fatal(err)
	}
	archive := zip.NewWriter(file)
	for i, name := range names {
		header := &zip.FileHeader{Name: name, Method: zip.Store}
		header.SetMode(modes[i])
		entry, err := archive.CreateHeader(header)
		if err != nil {
			t.Fatal(err)
		}
		if _, err = entry.Write([]byte("fixture")); err != nil {
			t.Fatal(err)
		}
	}
	if err = errors.Join(archive.Close(), file.Close()); err != nil {
		t.Fatal(err)
	}
	return file.Name()
}
func TestExtractRejectsTraversalLinksAndCollisions(t *testing.T) {
	cases := []struct {
		name  string
		names []string
		modes []os.FileMode
	}{
		{"parent", []string{"../outside"}, []os.FileMode{0600}},
		{"absolute", []string{"/outside"}, []os.FileMode{0600}},
		{"link", []string{"App/link"}, []os.FileMode{os.ModeSymlink | 0777}},
		{"case", []string{"App/file", "App/FILE"}, []os.FileMode{0600, 0600}},
		{"duplicate", []string{"App/file", "App/file"}, []os.FileMode{0600, 0600}},
		{"device", []string{"App/CON.txt"}, []os.FileMode{0600}},
		{"normalize", []string{"App/a/../file"}, []os.FileMode{0600}},
	}
	for _, test := range cases {
		t.Run(test.name, func(t *testing.T) {
			archive := writeZip(t, test.names, test.modes)
			destination := filepath.Join(t.TempDir(), "extracted")
			if err := Extract(context.Background(), archive, destination); err == nil {
				t.Fatal("accepted unsafe archive")
			}
		})
	}
	archive := writeZip(t, []string{"App/program"}, []os.FileMode{0700})
	destination := t.TempDir()
	sentinel := filepath.Join(destination, "existing")
	os.WriteFile(sentinel, []byte("keep"), 0600)
	if err := Extract(context.Background(), archive, destination); err == nil {
		t.Fatal("accepted non-empty destination")
	}
	if data, _ := os.ReadFile(sentinel); string(data) != "keep" {
		t.Fatal("deleted unrelated files")
	}
	fresh := filepath.Join(t.TempDir(), "fresh")
	if err := Extract(context.Background(), archive, fresh); err != nil {
		t.Fatal(err)
	}
	info, _ := os.Stat(filepath.Join(fresh, "App/program"))
	if info.Mode().Perm() != 0700 {
		t.Fatal("executable permission lost")
	}
}
func packageFixture(t *testing.T, root, version string) {
	t.Helper()
	if err := os.MkdirAll(root, 0700); err != nil {
		t.Fatal(err)
	}
	p := Package{ID: AppID, Version: version, OS: runtime.GOOS, Arch: runtime.GOARCH, Executable: "program", Helper: "helper"}
	raw, _ := json.Marshal(p)
	for name, data := range map[string][]byte{Marker: raw, "program": []byte(version), "helper": []byte("fixture")} {
		if err := os.WriteFile(filepath.Join(root, name), data, 0700); err != nil {
			t.Fatal(err)
		}
	}
}
func requestFixture(t *testing.T) Request {
	t.Helper()
	root := t.TempDir()
	dataRoot := filepath.Join(root, "data")
	store, err := state.Open(filepath.Join(dataRoot, "state.sqlite"))
	if err != nil {
		t.Fatal(err)
	}
	if err = store.SetSetting(context.Background(), "fixture", "before"); err != nil {
		t.Fatal(err)
	}
	if err = store.Close(); err != nil {
		t.Fatal(err)
	}
	updates := filepath.Join(dataRoot, "updates")
	os.MkdirAll(updates, 0700)
	r := Request{Target: filepath.Join(root, "App"), Stage: filepath.Join(root, ".superlink-stage-test"), Token: "test", Version: "0.2.0", ParentPID: os.Getpid(), DataRoot: dataRoot, HealthFile: filepath.Join(updates, "health-test.json"), Report: filepath.Join(updates, "report.json"), StateBackup: filepath.Join(updates, "state-test.sqlite")}
	r.Backup = r.Target + ".backup-" + r.Token
	packageFixture(t, r.Target, "0.1.0")
	packageFixture(t, r.Stage, "0.2.0")
	return r
}
func TestFailedUpgradeRestoresApplicationAndStateSchema(t *testing.T) {
	r := requestFixture(t)
	report := ApplyWithHealth(r, func(r Request) error {
		database, err := sql.Open("sqlite", filepath.Join(r.DataRoot, "state.sqlite"))
		if err != nil {
			return err
		}
		_, err = database.Exec("UPDATE settings SET value='after'; PRAGMA user_version=99")
		closeErr := database.Close()
		return errors.Join(err, closeErr, errors.New("simulated new-version startup failure"))
	})
	if report.Success || !report.RolledBack {
		t.Fatalf("rollback failed: %#v", report)
	}
	p, err := ReadPackage(r.Target)
	if err != nil || p.Version != "0.1.0" {
		t.Fatal("old application not restored")
	}
	store, err := state.Open(filepath.Join(r.DataRoot, "state.sqlite"))
	if err != nil {
		t.Fatal("schema rollback failed", err)
	}
	defer store.Close()
	value, err := store.Setting(context.Background(), "fixture")
	if err != nil || value != "before" {
		t.Fatal("metadata rollback failed")
	}
}
func TestSuccessfulUpgradeRetainsBackupsAndExclusiveWorkspace(t *testing.T) {
	r := requestFixture(t)
	lock, err := instance.Acquire(r.DataRoot)
	if err != nil {
		t.Fatal(err)
	}
	called := false
	report := ApplyWithHealth(r, func(Request) error { called = true; return nil })
	if report.Success || called {
		t.Fatal("updated an active workspace")
	}
	lock.Close()
	report = ApplyWithHealth(r, func(Request) error { return nil })
	if !report.Success || report.RolledBack {
		t.Fatal(report)
	}
	p, err := ReadPackage(r.Target)
	if err != nil || p.Version != "0.2.0" {
		t.Fatal("new application not installed")
	}
	if _, err = os.Stat(r.Backup); err != nil {
		t.Fatal("program backup lost")
	}
	if _, err = os.Stat(r.StateBackup); err != nil {
		t.Fatal("state backup lost")
	}
	r = requestFixture(t)
	r.Stage = r.Target
	if err = ValidateRequest(r); err == nil {
		t.Fatal("accepted self replacement")
	}
}

func TestHelperWorkingDirectoryOutsideInstall(t *testing.T) {
	root := t.TempDir()
	helper := filepath.Join(root, "updates", "helper-test", "helper")
	command := helperCommand(helper, filepath.Join(root, "request.json"))
	if command.Dir != filepath.Dir(helper) {
		t.Fatal("helper inherited application working directory")
	}
}
