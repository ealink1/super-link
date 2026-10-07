package ui

import (
	"bytes"
	"io"
	"log"
	"os"
	"path/filepath"
	"runtime"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/theme"
	"github.com/go-text/typesetting/font"
	"github.com/go-text/typesetting/fontscan"
)

func TestDesktopFontSkipsMissingAndInvalidFiles(t *testing.T) {
	dir := t.TempDir()
	invalid := filepath.Join(dir, "invalid.ttf")
	valid := filepath.Join(dir, "valid.ttf")
	fallback := theme.DefaultTextFont()
	if err := os.WriteFile(invalid, []byte("invalid"), 0600); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(valid, fallback.Content(), 0600); err != nil {
		t.Fatal(err)
	}
	if got := loadDesktopFont([]string{filepath.Join(dir, "missing"), invalid, valid}, fallback); got.Name() != valid {
		t.Fatal("did not select valid system file")
	}
	if got := loadDesktopFont([]string{invalid}, fallback); got != fallback {
		t.Fatal("missing fallback")
	}
}

func TestDesktopFontStyles(t *testing.T) {
	f := desktopFontSet{theme.DefaultTextFont(), theme.DefaultTextBoldFont(), theme.DefaultTextMonospaceFont(), theme.DefaultTextBoldItalicFont()}
	for _, c := range []struct {
		style fyne.TextStyle
		want  fyne.Resource
	}{
		{fyne.TextStyle{}, f.regular}, {fyne.TextStyle{Bold: true}, f.bold},
		{fyne.TextStyle{Monospace: true}, f.mono}, {fyne.TextStyle{Monospace: true, Bold: true}, f.monoBold},
	} {
		if got := f.font(c.style); got != c.want {
			t.Fatalf("wrong font for %+v", c.style)
		}
	}
}

func TestHostDesktopFontsDecodeAndMonospaceAdvances(t *testing.T) {
	f := desktopFonts
	for _, resource := range []fyne.Resource{f.regular, f.bold, f.mono, f.monoBold} {
		if runtime.GOOS == "darwin" || runtime.GOOS == "windows" {
			if !filepath.IsAbs(resource.Name()) {
				t.Fatalf("system font not found: %s", resource.Name())
			}
		}
		face, err := font.ParseTTF(bytes.NewReader(resource.Content()))
		if err != nil {
			t.Fatal(err)
		}
		for _, r := range "SuperLink0123" {
			if _, ok := face.NominalGlyph(r); !ok {
				t.Fatalf("missing %c", r)
			}
		}
	}
	face, err := font.ParseTTF(bytes.NewReader(f.mono.Content()))
	if err != nil {
		t.Fatal(err)
	}
	a, _ := face.NominalGlyph('i')
	b, _ := face.NominalGlyph('W')
	if face.HorizontalAdvance(a) != face.HorizontalAdvance(b) {
		t.Fatal("code font is not monospace")
	}
	t.Logf("system fonts: %s; %s; %s", f.regular.Name(), f.bold.Name(), f.mono.Name())
}

func TestHostChineseSystemFallback(t *testing.T) {
	if runtime.GOOS != "darwin" {
		t.Skip("host CJK verification requires macOS system fonts")
	}
	fm := fontscan.NewFontMap(log.New(io.Discard, "", 0))
	if err := fm.UseSystemFonts(t.TempDir()); err != nil {
		t.Fatal(err)
	}
	fm.SetQuery(fontscan.Query{Families: []string{fontscan.SansSerif}})
	for _, r := range "新建连接数据库中文查询" {
		face := fm.ResolveFace(r)
		if face == nil {
			t.Fatalf("no system fallback for %c", r)
		}
		if _, ok := face.NominalGlyph(r); !ok {
			t.Fatalf("system fallback lacks %c", r)
		}
	}
}
