package ui

import (
	"fmt"
	"image"
	"image/color"
	"strings"
	"testing"
	"unicode/utf8"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/driver/software"
	"fyne.io/fyne/v2/test"
	transport "github.com/ealink1/super-link/internal/infra/shell"
)

// Compare the painted glyphs against the same font explicitly assigned to a
// canvas.Text. Label geometry alone cannot detect a too-small glyph texture.
func TestMonitorNumericGlyphsMatchTheirMeasuredFont(t *testing.T) {
	w := shellTestWindow(t)
	w.switcher.selectMode(1)
	waitUI(t, w)
	pane := w.switcher.shell.newPane("SSH")
	defer pane.stop()
	monitor := newShellMonitorView(pane)
	monitor.metrics = transport.Metrics{Uptime: "380520 0"}
	identity := monitor.identityCard().(*fyne.Container)
	for _, sample := range []struct {
		name, text string
		size       float32
		shade      color.Color
		object     fyne.CanvasObject
	}{
		{"CPU", "28.1%", 11, monitorAccent("CPU"), test.WidgetRenderer(newMonitorMetricCard("CPU", "28.1%", "", "", 0, false, nil)).Objects()[3]},
		{"memory", "70%", 11, monitorAccent("内存"), test.WidgetRenderer(newMonitorMetricCard("内存", "70%", "", "", 0, false, nil)).Objects()[3]},
		{"disk", "54%", 11, monitorAccent("磁盘"), test.WidgetRenderer(newMonitorMetricCard("磁盘", "54%", "", "", 0, false, nil)).Objects()[3]},
		{"memory-76", "76%", 11, monitorAccent("内存"), test.WidgetRenderer(newMonitorMetricCard("内存", "76%", "", "", 0, false, nil)).Objects()[3]},
		{"disk-80", "80%", 11, monitorAccent("磁盘"), test.WidgetRenderer(newMonitorMetricCard("磁盘", "80%", "", "", 0, false, nil)).Objects()[3]},
		{"uptime", "105h 42m", 10, monitorUptimeColor, identity.Objects[7]},
		{"address", "192.168.0.104", 11, monitorIPColor, newMonitorIdentityBadge("192.168.0.104", monitorIPColor, nil)},
	} {
		for _, scale := range []float32{1, 1.25, 1.5, 2} {
			t.Run(fmt.Sprintf("%s/scale-%g", sample.name, scale), func(t *testing.T) {
				w.Window.Canvas().(software.WindowlessCanvas).SetScale(scale)
				shade := color.NRGBAModel.Convert(resolveShellColor(sample.shade)).(color.NRGBA)
				w.Window.SetContent(container.NewThemeOverride(sample.object, newShellTheme()))
				w.Window.Resize(fyne.NewSize(160, 40))
				actual := monitorInkCrop(t, w.Window.Canvas().Capture(), shade)
				reference := canvas.NewText(sample.text+" ", shade)
				reference.TextSize = sample.size
				reference.TextStyle.Monospace = true
				reference.FontSource = monitorMonoFont
				var content fyne.CanvasObject = container.NewPadded(reference)
				if sample.name == "address" {
					content = container.NewStack(shellRectangle(monitorBadgeFill, 6, nil), content)
				}
				w.Window.SetContent(content)
				expected := monitorInkCrop(t, w.Window.Canvas().Capture(), shade)
				if actual.Bounds().Size() != expected.Bounds().Size() {
					t.Fatalf("painted glyphs are clipped: got %v, want %v", actual.Bounds(), expected.Bounds())
				}
				for y := 0; y < expected.Bounds().Dy(); y++ {
					for x := 0; x < expected.Bounds().Dx(); x++ {
						if actual.At(x, y) != expected.At(x, y) {
							t.Fatalf("glyph ink differs at %d,%d", x, y)
						}
					}
				}
			})
		}
	}
}

func monitorInkCrop(t *testing.T, im image.Image, shade color.NRGBA) *image.Gray {
	t.Helper()
	bounds, ok := shellFileInkBounds(im, im.Bounds(), shade)
	if !ok {
		t.Fatal("text ink missing")
	}
	out := image.NewGray(image.Rect(0, 0, bounds.Dx(), bounds.Dy()))
	for y := bounds.Min.Y; y < bounds.Max.Y; y++ {
		for x := bounds.Min.X; x < bounds.Max.X; x++ {
			pixel := color.NRGBAModel.Convert(im.At(x, y)).(color.NRGBA)
			// Compare the glyph mask, independently of the badge's background.
			if shellInkMatches(pixel, shade) {
				out.SetGray(x-bounds.Min.X, y-bounds.Min.Y, color.Gray{Y: 255})
			}
		}
	}
	return out
}

func TestMonitorTextTruncationUsesThePaintedFont(t *testing.T) {
	shellTestWindow(t)
	for _, text := range []string{"192.168.0.104", "2001:db8:abcd:1234::104", "105h 42m", "100.0%", "· Linux kaifaserver", "测试主机中文"} {
		for _, width := range []float32{0, 5, 10, 25, 50, 90, 160} {
			got := monitorFitText(text, 11, width)
			if !utf8.ValidString(got) || monitorTextSize(got, 11).Width > width {
				t.Fatalf("invalid or overflowing text %q at width %g", got, width)
			}
			if monitorTextSize(text, 11).Width <= width && got != text {
				t.Fatalf("fitting text was truncated: %q to %q", text, got)
			}
			if got != "" && got != text && (!strings.HasSuffix(got, "…") || !strings.HasPrefix(text, strings.TrimSuffix(got, "…"))) {
				t.Fatalf("invalid ellipsis: %q to %q", text, got)
			}
		}
	}
}
