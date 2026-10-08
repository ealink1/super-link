package ui

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/ealink1/super-link/internal/datafile"
	"github.com/ealink1/super-link/internal/domain"
)

func TestExportCompletesWithoutDialogClosePanic(t *testing.T) {
	w, profile := parityWindow(t)
	path := filepath.Join(t.TempDir(), "export.csv")
	source := exportSource{profile: profile, result: domain.Result{Columns: []domain.Column{{Name: "name"}}, Rows: [][]any{{"sample"}}}}
	w.startExport(source, datafile.Options{Format: "CSV"}, path, false, false)
	waitUI(t, w)
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(data), "name") || !strings.Contains(string(data), "sample") {
		t.Fatal("exported data missing")
	}
	if w.Window.Canvas().Overlays().Top() == nil {
		t.Fatal("completion dialog missing")
	}
}
