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

type desktopFontSet struct{ regular, bold, mono, monoBold, italic, boldItalic, monoItalic, monoBoldItalic fyne.Resource }

func (f desktopFontSet) font(style fyne.TextStyle) fyne.Resource {
	if style.Monospace {
		if style.Italic {
			if style.Bold {
				return f.monoBoldItalic
			}
			return f.monoItalic
		}
		if style.Bold {
			return f.monoBold
		}
		return f.mono
	}
	if style.Italic {
		if style.Bold {
			return f.boldItalic
		}
		return f.italic
	}
	if style.Bold {
		return f.bold
	}
	return f.regular
}

func loadDesktopFonts(platform, windowsDir string) desktopFontSet {
	var regular, bold, mono, monoBold, italic, boldItalic, monoItalic, monoBoldItalic []string
	switch platform {
	case "darwin":
		root := "/System/Library/Fonts/Supplemental"
		regular = []string{filepath.Join(root, "Arial.ttf")}
		bold = []string{filepath.Join(root, "Arial Bold.ttf")}
		mono = []string{filepath.Join(root, "Courier New.ttf")}
		monoBold = []string{filepath.Join(root, "Courier New Bold.ttf")}
		italic = []string{filepath.Join(root, "Arial Italic.ttf")}
		boldItalic = []string{filepath.Join(root, "Arial Bold Italic.ttf")}
		monoItalic = []string{filepath.Join(root, "Courier New Italic.ttf")}
		monoBoldItalic = []string{filepath.Join(root, "Courier New Bold Italic.ttf")}
	case "windows":
		if windowsDir == "" {
			windowsDir = `C:\Windows`
		}
		root := filepath.Join(windowsDir, "Fonts")
		regular = []string{filepath.Join(root, "segoeui.ttf"), filepath.Join(root, "arial.ttf")}
		bold = []string{filepath.Join(root, "segoeuib.ttf"), filepath.Join(root, "arialbd.ttf")}
		mono = []string{filepath.Join(root, "consola.ttf"), filepath.Join(root, "cour.ttf")}
		monoBold = []string{filepath.Join(root, "consolab.ttf"), filepath.Join(root, "courbd.ttf")}
		italic = []string{filepath.Join(root, "segoeuii.ttf"), filepath.Join(root, "ariali.ttf")}
		boldItalic = []string{filepath.Join(root, "segoeuiz.ttf"), filepath.Join(root, "arialbi.ttf")}
		monoItalic = []string{filepath.Join(root, "consolai.ttf"), filepath.Join(root, "couri.ttf")}
		monoBoldItalic = []string{filepath.Join(root, "consolaz.ttf"), filepath.Join(root, "courbi.ttf")}
	default:
		for _, root := range []string{"/usr/share/fonts/truetype", "/usr/share/fonts", "/usr/local/share/fonts"} {
			for _, family := range []string{"dejavu", "liberation2", "liberation", "noto"} {
				dir := filepath.Join(root, family)
				regular = append(regular, filepath.Join(dir, "DejaVuSans.ttf"), filepath.Join(dir, "LiberationSans-Regular.ttf"), filepath.Join(dir, "NotoSans-Regular.ttf"))
				bold = append(bold, filepath.Join(dir, "DejaVuSans-Bold.ttf"), filepath.Join(dir, "LiberationSans-Bold.ttf"), filepath.Join(dir, "NotoSans-Bold.ttf"))
				mono = append(mono, filepath.Join(dir, "DejaVuSansMono.ttf"), filepath.Join(dir, "LiberationMono-Regular.ttf"), filepath.Join(dir, "NotoSansMono-Regular.ttf"))
				monoBold = append(monoBold, filepath.Join(dir, "DejaVuSansMono-Bold.ttf"), filepath.Join(dir, "LiberationMono-Bold.ttf"), filepath.Join(dir, "NotoSansMono-Bold.ttf"))
				italic = append(italic, filepath.Join(dir, "DejaVuSans-Oblique.ttf"), filepath.Join(dir, "LiberationSans-Italic.ttf"), filepath.Join(dir, "NotoSans-Italic.ttf"))
				boldItalic = append(boldItalic, filepath.Join(dir, "DejaVuSans-BoldOblique.ttf"), filepath.Join(dir, "LiberationSans-BoldItalic.ttf"), filepath.Join(dir, "NotoSans-BoldItalic.ttf"))
				monoItalic = append(monoItalic, filepath.Join(dir, "DejaVuSansMono-Oblique.ttf"), filepath.Join(dir, "LiberationMono-Italic.ttf"))
				monoBoldItalic = append(monoBoldItalic, filepath.Join(dir, "DejaVuSansMono-BoldOblique.ttf"), filepath.Join(dir, "LiberationMono-BoldItalic.ttf"))
			}
		}
	}
	return desktopFontSet{
		italic:         loadDesktopFont(italic, theme.DefaultTextItalicFont()),
		boldItalic:     loadDesktopFont(boldItalic, theme.DefaultTextBoldItalicFont()),
		monoItalic:     loadDesktopFont(monoItalic, theme.DefaultTextItalicFont()),
		monoBoldItalic: loadDesktopFont(monoBoldItalic, theme.DefaultTextBoldItalicFont()),
		regular:        loadDesktopFont(regular, theme.DefaultTextFont()),
		bold:           loadDesktopFont(bold, theme.DefaultTextBoldFont()),
		mono:           loadDesktopFont(mono, theme.DefaultTextMonospaceFont()),
		monoBold:       loadDesktopFont(monoBold, theme.DefaultTextMonospaceFont()),
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
