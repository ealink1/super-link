package ui

import (
	"image"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/test"
)

// A following blank terminal row must not erase the bottom of an earlier
// Chinese glyph. Compare against the same text painted without row backgrounds.
func TestTerminalChineseInkMatchesStandaloneText(t *testing.T) {
	w := shellTestWindow(t)
	w.switcher.selectMode(1)
	waitUI(t, w)
	for _, dark := range []bool{false, true} {
		w.setDark(dark)
		for _, size := range []float32{12, 14, 18, 24} {
			terminal := newTerminalSurface(nil)
			terminal.textSize = size
			w.Window.SetContent(terminal)
			w.Window.Resize(fyne.NewSize(640, 260))
			text := "中文扫描验证国鬱gj"
			terminal.feed([]byte(text + "\r\n"))
			actual := w.Window.Canvas().Capture()
			r := test.WidgetRenderer(terminal).(*terminalRenderer)
			sample := r.pool[0]
			reference := canvas.NewText(sample.Text, sample.Color)
			reference.TextSize, reference.TextStyle = sample.TextSize, sample.TextStyle
			reference.Move(sample.Position())
			background := canvas.NewRectangle(resolveShellColor(terminalBackground))
			background.Resize(terminal.Size())
			w.Window.SetContent(container.NewWithoutLayout(background, reference))
			expected := w.Window.Canvas().Capture()
			area := image.Rect(0, 0, int(reference.MinSize().Width), int(reference.MinSize().Height)+5)
			if !hostRegionEqual(actual, expected, area) {
				t.Errorf("font size %.0f: terminal rows clipped or covered Chinese ink", size)
			}
			terminal.dispose()
		}
	}
}
