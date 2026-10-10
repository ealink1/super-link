package painter

import (
	"image/color"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/theme"
)

type cacheTestTheme struct {
	regular, bold fyne.Resource
	tint          uint8
}

func (t cacheTestTheme) Color(fyne.ThemeColorName, fyne.ThemeVariant) color.Color {
	return color.NRGBA{R: 10, G: 20, B: 30, A: t.tint}
}
func (t cacheTestTheme) Icon(fyne.ThemeIconName) fyne.Resource { return nil }
func (t cacheTestTheme) Size(fyne.ThemeSizeName) float32       { return 0 }
func (t cacheTestTheme) Font(style fyne.TextStyle) fyne.Resource {
	if style.Bold {
		return t.bold
	}
	return t.regular
}

type cacheTestSettings struct{ th fyne.Theme }

func (s *cacheTestSettings) Theme() fyne.Theme                    { return s.th }
func (s *cacheTestSettings) SetTheme(fyne.Theme)                  {}
func (s *cacheTestSettings) ThemeVariant() fyne.ThemeVariant      { return theme.VariantLight }
func (s *cacheTestSettings) Scale() float32                       { return 1 }
func (s *cacheTestSettings) PrimaryColor() string                 { return "" }
func (s *cacheTestSettings) AddChangeListener(chan fyne.Settings) {}
func (s *cacheTestSettings) AddListener(func(fyne.Settings))      {}
func (s *cacheTestSettings) BuildType() fyne.BuildType            { return fyne.BuildStandard }
func (s *cacheTestSettings) ShowAnimations() bool                 { return false }

func warmFaceCache() {
	fontCache.Store(cacheID{style: fyne.TextStyle{}}, &FontCacheItem{})
}

func faceCacheWarm() bool {
	warm := false
	fontCache.Range(func(cacheID, *FontCacheItem) bool { warm = true; return false })
	return warm
}

func TestApplyThemeCachesKeepsFacesUntilTheFontMappingChanges(t *testing.T) {
	night := &cacheTestSettings{th: cacheTestTheme{
		regular: fyne.NewStaticResource("regular.ttf", []byte("regular")),
		bold:    fyne.NewStaticResource("bold.ttf", []byte("bold")),
		tint:    200,
	}}
	ApplyThemeCaches(night)
	warmFaceCache()

	day := &cacheTestSettings{th: cacheTestTheme{
		regular: fyne.NewStaticResource("regular.ttf", []byte("regular")),
		bold:    fyne.NewStaticResource("bold.ttf", []byte("bold")),
		tint:    30,
	}}
	ApplyThemeCaches(day)
	if !faceCacheWarm() {
		t.Fatal("a colour-only theme switch reloaded every font face")
	}

	// The same resource names now back different styles: the faces must be reloaded.
	permuted := &cacheTestSettings{th: cacheTestTheme{
		regular: fyne.NewStaticResource("bold.ttf", []byte("bold")),
		bold:    fyne.NewStaticResource("regular.ttf", []byte("regular")),
		tint:    30,
	}}
	ApplyThemeCaches(permuted)
	if faceCacheWarm() {
		t.Fatal("faces were reused after the theme remapped its fonts")
	}

	// Another component may empty the caches without going through this path, so
	// a later switch must not assume the faces it recorded are still stored.
	ApplyThemeCaches(day)
	warmFaceCache()
	ClearFontCache()
	ApplyThemeCaches(day)
	if faceCacheWarm() {
		t.Fatal("ClearFontCache left a stale record that skipped the reload")
	}
}
