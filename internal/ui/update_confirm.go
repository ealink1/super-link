package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
	"image/color"
)

func (w *Window) showUpdateConfirm(message string, accept func()) {
	label := widget.NewLabel(message)
	label.Alignment = fyne.TextAlignCenter
	modal := dialog.NewCustomWithoutButtons("发现新版本", label, w.Window)
	modal.SetIcon(theme.QuestionIcon())
	cancel := widget.NewButtonWithIcon("", theme.CancelIcon(), modal.Hide)
	cancel.Importance = widget.LowImportance
	border := canvas.NewRectangle(color.Transparent)
	border.CornerRadius = 6
	border.StrokeColor = color.NRGBA{R: 120, G: 130, B: 142, A: 255}
	border.StrokeWidth = 1.5
	cancelBox := container.NewStack(border, cancel)
	confirm := widget.NewButtonWithIcon("", theme.ConfirmIcon(), func() { modal.Hide(); accept() })
	confirm.Importance = widget.HighImportance
	// HBox retains its normal padding; this spacer adds 20 pixels between buttons.
	gap := canvas.NewRectangle(color.Transparent)
	gap.SetMinSize(fyne.NewSize(max(0, 20-theme.Padding()), 1))
	row := container.NewHBox(layout.NewSpacer(), container.NewGridWrap(fyne.NewSize(36, 32), cancelBox), gap, container.NewGridWrap(fyne.NewSize(36, 32), confirm), layout.NewSpacer())
	modal.SetButtons([]fyne.CanvasObject{row})
	modal.Show()
}
