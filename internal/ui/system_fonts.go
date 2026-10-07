package ui

import (
	"bytes"
	"io"
	"os"
	"path/filepath"
	"runtime"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/theme"
	"github.com/go-text/typesetting/font"
)

// Load once before UI construction. Theme.Font only returns cached resources;
// Chinese and other missing glyphs use Fyne's existing system-font fallback.
var desktopFonts = loadDesktopFonts(runtime.GOOS, os.Getenv("WINDIR"))

type desktopFontSet struct{ regular, bold, mono, monoBold fyne.Resource }

func (f desktopFontSet) font(style fyne.TextStyle) fyne.Resource {
	if style.Monospace {
		if style.Bold {
			return f.monoBold
		}
		return f.mono
	}
	if style.Bold {
		return f.bold
	}
	return f.regular
}

func loadDesktopFonts(platform, windowsDir string) desktopFontSet {
	var regular, bold, mono, monoBold []string
	switch platform {
	case "darwin":
		root := "/System/Library/Fonts/Supplemental"
		regular = []string{filepath.Join(root, "Arial.ttf")}
		bold = []string{filepath.Join(root, "Arial Bold.ttf")}
		mono = []string{filepath.Join(root, "Courier New.ttf")}
		monoBold = []string{filepath.Join(root, "Courier New Bold.ttf")}
	case "windows":
		if windowsDir == "" {
			windowsDir = `C:\Windows`
		}
		root := filepath.Join(windowsDir, "Fonts")
		regular = []string{filepath.Join(root, "segoeui.ttf"), filepath.Join(root, "arial.ttf")}
		bold = []string{filepath.Join(root, "segoeuib.ttf"), filepath.Join(root, "arialbd.ttf")}
		mono = []string{filepath.Join(root, "consola.ttf"), filepath.Join(root, "cour.ttf")}
		monoBold = []string{filepath.Join(root, "consolab.ttf"), filepath.Join(root, "courbd.ttf")}
	default:
		for _, root := range []string{"/usr/share/fonts/truetype", "/usr/share/fonts", "/usr/local/share/fonts"} {
			for _, family := range []string{"dejavu", "liberation2", "liberation", "noto"} {
				dir := filepath.Join(root, family)
				regular = append(regular, filepath.Join(dir, "DejaVuSans.ttf"), filepath.Join(dir, "LiberationSans-Regular.ttf"), filepath.Join(dir, "NotoSans-Regular.ttf"))
				bold = append(bold, filepath.Join(dir, "DejaVuSans-Bold.ttf"), filepath.Join(dir, "LiberationSans-Bold.ttf"), filepath.Join(dir, "NotoSans-Bold.ttf"))
				mono = append(mono, filepath.Join(dir, "DejaVuSansMono.ttf"), filepath.Join(dir, "LiberationMono-Regular.ttf"), filepath.Join(dir, "NotoSansMono-Regular.ttf"))
				monoBold = append(monoBold, filepath.Join(dir, "DejaVuSansMono-Bold.ttf"), filepath.Join(dir, "LiberationMono-Bold.ttf"), filepath.Join(dir, "NotoSansMono-Bold.ttf"))
			}
		}
	}
	return desktopFontSet{
		regular:  loadDesktopFont(regular, theme.DefaultTextFont()),
		bold:     loadDesktopFont(bold, theme.DefaultTextBoldFont()),
		mono:     loadDesktopFont(mono, theme.DefaultTextMonospaceFont()),
		monoBold: loadDesktopFont(monoBold, theme.DefaultTextMonospaceFont()),
	}
}

func loadDesktopFont(paths []string, fallback fyne.Resource) fyne.Resource {
	for _, path := range paths {
		file, err := os.Open(path)
		if err != nil {
			continue
		}
		// Bound malformed or unexpectedly large files; never retain an open handle.
		const limit = 32 << 20
		data, err := io.ReadAll(io.LimitReader(file, limit+1))
		file.Close()
		if err != nil || len(data) > limit {
			continue
		}
		if _, err := font.ParseTTF(bytes.NewReader(data)); err != nil {
			continue
		}
		return fyne.NewStaticResource(path, data)
	}
	// Minimal framework fonts keep UI usable on systems without standard fonts.
	return fallback
}
