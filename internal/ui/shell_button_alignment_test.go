package ui

import (
	"image"
	"image/color"
	"math"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/driver/desktop"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/widget"
)

func TestShellHostButtonIconAndTextPaintedAtSameHeight(t *testing.T) {
	app := test.NewTempApp(t)
	app.Settings().SetTheme(Theme{})
	w := app.NewWindow("button alignment")
	defer w.Close()
	for _, tc := range []struct {
		text, icon string
		primary    bool
		alignment  widget.ButtonAlign
	}{
		{"全部主机", "layout-grid", false, widget.ButtonAlignLeading},
		{"默认分组  3", "folder", false, widget.ButtonAlignLeading},
		{"隐私模式", "eye-off", false, widget.ButtonAlignCenter},
		{"新建主机", "plus", true, widget.ButtonAlignCenter},
	} {
		button := shellButton(tc.text, tc.icon, tc.primary, nil)
		button.Alignment = tc.alignment
		w.SetContent(shellButtonView(button))
		w.Resize(fyne.NewSize(180, 32))
		w.Show()
		renderer := test.WidgetRenderer(button)
		var icon *canvas.Image
		var label *widget.RichText
		for _, object := range renderer.Objects() {
			switch object := object.(type) {
			case *canvas.Image:
				icon = object
			case *widget.RichText:
				label = object
			}
		}
		if icon == nil || label == nil {
			t.Fatal("button content missing", tc.text)
		}
		// The first paint establishes nested theme scopes. Measure the settled
		// layout, as the native canvas does on subsequent frames.
		w.Canvas().Capture()
		renderer.Layout(button.Size())
		for _, state := range []string{"normal", "hovered", "focused", "disabled"} {
			switch state {
			case "hovered":
				button.MouseIn(&desktop.MouseEvent{})
			case "focused":
				button.MouseOut()
				button.FocusGained()
			case "disabled":
				button.FocusLost()
				button.Disable()
			}
			im := w.Canvas().Capture()
			iconInk := shellButtonInkBounds(im, image.Rect(int(icon.Position().X), 3, int(icon.Position().X+icon.Size().Width), 29), tc.primary)
			textInk := shellButtonInkBounds(im, image.Rect(int(label.Position().X), 3, int(label.Position().X+label.Size().Width), 29), tc.primary && state != "disabled")
			if iconInk.Empty() || textInk.Empty() {
				t.Fatalf("%s %s: missing icon/text ink %v %v", tc.text, state, iconInk, textInk)
			}
			if math.Abs(float64(iconInk.Min.Y+iconInk.Max.Y-textInk.Min.Y-textInk.Max.Y)/2) > 1.5 {
				t.Errorf("%s %s: icon %v and text %v are not vertically aligned", tc.text, state, iconInk, textInk)
			}
		}
	}
}

func shellButtonInkBounds(im image.Image, area image.Rectangle, light bool) image.Rectangle {
	bounds := image.Rectangle{Min: area.Max, Max: area.Min}
	for y := area.Min.Y; y < area.Max.Y; y++ {
		for x := area.Min.X; x < area.Max.X; x++ {
			c := color.NRGBAModel.Convert(im.At(x, y)).(color.NRGBA)
			bright, dark := max(c.R, c.G, c.B), min(c.R, c.G, c.B)
			if bright-dark > 40 || (!light && bright >= 210) || (light && dark < 240) {
				continue
			}
			bounds.Min.X, bounds.Min.Y = min(bounds.Min.X, x), min(bounds.Min.Y, y)
			bounds.Max.X, bounds.Max.Y = max(bounds.Max.X, x+1), max(bounds.Max.Y, y+1)
		}
	}
	return bounds
}
