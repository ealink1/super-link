//go:build !darwin

package filedialog

import (
	"context"

	"github.com/ncruces/zenity"
)

func platformPicker(context.Context, string, bool, []string) func(...zenity.Option) (string, error) {
	return zenity.SelectFile
}
