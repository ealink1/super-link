package ui

import (
	"context"

	"github.com/ealink1/super-link/internal/infra/filedialog"
)

// File panels and filesystem validation run outside the rendering goroutine.
// jobs.run delivers the result back on the Fyne goroutine.
func (w *Window) chooseLocalPath(title string, directory bool, extensions []string, selected func(string)) {
	w.jobs.run(func(ctx context.Context) (any, error) {
		return filedialog.SelectFiltered(ctx, title, directory, extensions)
	}, func(value any, err error) {
		if err != nil {
			w.showError(err)
			return
		}
		if path, ok := value.(string); ok && path != "" {
			selected(path)
		}
	})
}
