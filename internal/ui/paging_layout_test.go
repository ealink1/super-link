package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/widget"
	"image/color"
	"testing"
)

func TestPagingRowCentersDifferentControlHeights(t *testing.T) {
	short := canvas.NewRectangle(color.White)
	short.SetMinSize(fyne.NewSize(40, 20))
	tall := canvas.NewRectangle(color.White)
	tall.SetMinSize(fyne.NewSize(60, 36))
	objects := []fyne.CanvasObject{layout.NewSpacer(), short, tall}
	pagingRowLayout{}.Layout(objects, fyne.NewSize(200, 40))
	for _, object := range objects[1:] {
		if object.Position().Y+object.Size().Height/2 != 20 || object.Size().Height != object.MinSize().Height {
			t.Fatal("paging control was stretched or not centered", object.Position(), object.Size())
		}
	}
	if short.Position().X >= tall.Position().X {
		t.Fatal("control order changed")
	}
}

func TestPagingRowCorrectsNumericAndChineseTextPositions(t *testing.T) {
	app := test.NewApp()
	defer app.Quit()
	app.Settings().SetTheme(Theme{})
	entry := widget.NewEntry()
	entry.SetText("1")
	selectBox := widget.NewSelect([]string{"100"}, nil)
	selectBox.SetSelected("100")
	button := action("上一页", "", nil)
	label := widget.NewLabel("当前 0 条 / 共 0 条")
	objects := []fyne.CanvasObject{label, button, entry, selectBox}
	row := pagingRowLayout{}
	size := row.MinSize(objects)
	row.Layout(objects, size)
	center := func(object fyne.CanvasObject) float32 { return object.Position().Y + object.Size().Height/2 }
	if center(button) != center(label)-1 || center(entry) != center(label)+2 || center(selectBox) != center(entry) {
		t.Fatal("paging text optical alignment lost")
	}
	for _, object := range objects {
		if object.Position().Y < 0 || object.Position().Y+object.Size().Height > size.Height {
			t.Fatal("paging control clipped")
		}
	}
}
