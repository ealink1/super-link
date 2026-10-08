package ui

import (
	"image"
	"image/color"
	"math"
	"os"
	"testing"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/test"
	transport "github.com/ealink1/super-link/internal/infra/shell"
)

// Compare painted glyphs with the icon, rather than only checking layout boxes.
func TestShellFileRowPaintedContentIsVerticallyAligned(t *testing.T) {
	w, _ := fileActionFixture(t)
	row := newShellFileRow()
	row.update(transport.File{Name: "beanstalk_data", Directory: true, Modified: time.Date(2026, 4, 24, 10, 44, 0, 0, time.Local)})
	w.Window.SetContent(container.NewThemeOverride(row, newShellTheme()))
	w.Window.Resize(fyne.NewSize(420, 74))
	row.Resize(fyne.NewSize(420, 74))
	// A first paint can initialize system fallback fonts, especially under
	// race instrumentation. Lay out once more with the resolved metrics.
	w.Window.Canvas().Capture()
	test.WidgetRenderer(row).Layout(row.Size())
	im := w.Window.Canvas().Capture()
	icon, ok := shellFileInkBounds(im, image.Rect(10, 0, 40, 74), color.NRGBA{112, 101, 255, 255})
	if !ok {
		t.Fatal("folder icon is missing")
	}
	name, ok := shellFileInkBounds(im, image.Rect(44, 0, 245, 74), color.NRGBA{78, 80, 104, 255})
	if !ok {
		t.Fatal("filename is missing")
	}
	if math.Abs(float64(icon.Min.Y+icon.Max.Y-name.Min.Y-name.Max.Y)/2) > 2 {
		t.Fatalf("painted icon and filename are not centered together: icon %v, text %v", icon, name)
	}
}
func shellFileInkBounds(im image.Image, area image.Rectangle, shade color.NRGBA) (image.Rectangle, bool) {
	bounds := image.Rectangle{Min: image.Pt(area.Max.X, area.Max.Y), Max: area.Min}
	found := false
	for y := area.Min.Y; y < area.Max.Y; y++ {
		for x := area.Min.X; x < area.Max.X; x++ {
			c := color.NRGBAModel.Convert(im.At(x, y)).(color.NRGBA)
			if shellInkMatches(c, shade) {
				found = true
				bounds.Min.X = min(bounds.Min.X, x)
				bounds.Min.Y = min(bounds.Min.Y, y)
				bounds.Max.X = max(bounds.Max.X, x+1)
				bounds.Max.Y = max(bounds.Max.Y, y+1)
			}
		}
	}
	return bounds, found
}
func TestShellFileRowsReferenceCapture(t *testing.T) {
	if os.Getenv("SUPERLINK_UI_CAPTURE_DIR") == "" {
		t.Skip("optional visual capture")
	}
	w, f := fileActionFixture(t)
	f.files = nil
	for _, name := range []string{"beanstalk_data", "ffmate-docker", "ftp-docker", "kafka-docker", "lnmp-docker", "meilisearch-docker", "meilisearch-ui", "mysql-docker", "nextcloud-ragflow-docker", "nginx-docker", "openim-docker", "ott"} {
		f.files = append(f.files, transport.File{Name: name, Directory: true, Modified: time.Date(2026, 4, 24, 10, 44, 0, 0, time.Local)})
	}
	f.files = append(f.files, transport.File{Name: "ott_member_code_pool.zip", Size: 398479851, Modified: time.Date(2026, 6, 2, 20, 20, 0, 0, time.Local)}, transport.File{Name: "test.txt", Size: 4, Modified: time.Date(2026, 4, 1, 15, 24, 0, 0, time.Local)})
	f.filterFiles()
	f.status.SetText("14 项")
	f.list.Select(3)
	w.Window.SetContent(container.NewThemeOverride(f.content, newShellTheme()))
	captureShellFixtureSize(t, w, "files-rows-reference.png", fyne.NewSize(420, 760))
}

// Small fonts may contain only antialiased pixels. Match foreground coverage
// along the blend with a light background, rather than requiring solid pixels.
func shellInkMatches(c, shade color.NRGBA) bool {
	channels := [3]float64{float64(c.R), float64(c.G), float64(c.B)}
	target := [3]float64{float64(shade.R), float64(shade.G), float64(shade.B)}
	var numerator, denominator float64
	for i := range channels {
		d := 255 - target[i]
		numerator += (255 - channels[i]) * d
		denominator += d * d
	}
	if denominator == 0 {
		return false
	}
	coverage := numerator / denominator
	if coverage < 0.4 || coverage > 1.1 {
		return false
	}
	for i := range channels {
		if math.Abs(channels[i]-(255-coverage*(255-target[i]))) > 16 {
			return false
		}
	}
	return true
}
