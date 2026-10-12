package ui

import (
	"strings"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/test"
	transport "github.com/ealink1/super-link/internal/infra/shell"
)

func TestMonitorNumericInkStaysInsideLabelEdge(t *testing.T) {
	w := shellTestWindow(t)
	w.switcher.selectMode(1)
	waitUI(t, w)
	pane := w.switcher.shell.newPane("SSH")
	defer pane.stop()
	monitor := newShellMonitorView(pane)
	monitor.metrics = transport.Metrics{Uptime: "379980 0"}
	identity := monitor.identityCard().(*fyne.Container)
	card := newMonitorMetricCard("内存", "71%", "", "", .71, false, nil)
	for _, sample := range []struct {
		name, text string
		object     fyne.CanvasObject
	}{
		{"percentage", "71%", test.WidgetRenderer(card).Objects()[3]},
		{"uptime", "105h 33m", identity.Objects[7]},
		{"address", "192.168.0.104", newMonitorIdentityBadge("192.168.0.104", monitorIPColor, nil)},
	} {
		t.Run(sample.name, func(t *testing.T) {
			w.Window.SetContent(container.NewThemeOverride(sample.object, newShellTheme()))
			w.Window.Resize(fyne.NewSize(130, 30))
			w.Window.Canvas().Capture()
			found := false
			walkMonitorRenderedText(sample.object, func(object fyne.CanvasObject) {
				text, ok := object.(*canvas.Text)
				if !ok || strings.TrimSuffix(text.Text, monitorTextRasterSuffix) != sample.text {
					return
				}
				found = true
				origin := w.App.Driver().AbsolutePositionForObject(text)
				advance := text.MinSize().Width
				right := origin.X + advance
				if text.Alignment == fyne.TextAlignTrailing {
					right = origin.X + text.Size().Width
				}
				outer := w.App.Driver().AbsolutePositionForObject(sample.object)
				if gap := outer.X + sample.object.Size().Width - right; gap < 1.5 {
					t.Errorf("%s ends against the clip boundary: %.2f px remaining", sample.text, gap)
				}
			})
			if !found {
				t.Fatalf("full value was truncated: %s", sample.text)
			}
		})
	}
}

func walkMonitorRenderedText(object fyne.CanvasObject, visit func(fyne.CanvasObject)) {
	visit(object)
	var children []fyne.CanvasObject
	switch value := object.(type) {
	case *fyne.Container:
		children = value.Objects
	case fyne.Widget:
		children = test.WidgetRenderer(value).Objects()
	}
	for _, child := range children {
		walkMonitorRenderedText(child, visit)
	}
}
