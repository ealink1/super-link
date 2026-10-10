package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"image/color"
)

func newNavigatorGroupCount(label *widget.Label) fyne.CanvasObject {
	background := shellRectangle(shellRailColor, 9, color.Transparent)
	return shellFixed(container.NewStack(background, label), 24, 22)
}

type navigatorGroupLabelLayout struct{ row *treeRow }

func (l *navigatorGroupLabelLayout) MinSize(objects []fyne.CanvasObject) fyne.Size {
	return objects[0].MinSize()
}
func (l *navigatorGroupLabelLayout) Layout(objects []fyne.CanvasObject, size fyne.Size) {
	label, badge := objects[0], objects[1]
	width := size.Width
	if badge.Visible() {
		badgeSize := badge.MinSize()
		width = min(max(0, size.Width-badgeSize.Width-4), fyne.MeasureText(l.row.label.Text, theme.TextSize(), l.row.label.TextStyle).Width+2*theme.InnerPadding())
		badge.Resize(badgeSize)
		badge.Move(fyne.NewPos(width+4, (size.Height-badgeSize.Height)/2))
	}
	label.Move(fyne.Position{})
	label.Resize(fyne.NewSize(width, size.Height))
}
