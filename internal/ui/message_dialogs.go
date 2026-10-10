package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/lang"
	"fyne.io/fyne/v2/widget"
)

func messageDialogContent(message string, parent fyne.Window) (*widget.Label, fyne.Size) {
	label := widget.NewLabel(message)
	label.Alignment = fyne.TextAlignCenter
	width := min(600, label.MinSize().Width+32)
	if available := parent.Canvas().Size().Width; available > 0 {
		width = min(width, available*0.9)
	}
	label.Wrapping = fyne.TextWrapWord
	label.Resize(fyne.NewSize(max(32, width-32), label.MinSize().Height))
	return label, fyne.NewSize(width, label.MinSize().Height+80)
}

func newInformationDialog(title, message string, parent fyne.Window) *dialog.CustomDialog {
	content, size := messageDialogContent(message, parent)
	modal := dialog.NewCustom(title, lang.L("OK"), content, parent)
	modal.Resize(size)
	return modal
}

func showInformationDialog(title, message string, parent fyne.Window) {
	newInformationDialog(title, message, parent).Show()
}

func showErrorDialog(err error, parent fyne.Window) {
	newInformationDialog(lang.L("Error"), err.Error(), parent).Show()
}

func newConfirmDialog(title, message string, callback func(bool), parent fyne.Window) *dialog.ConfirmDialog {
	content, size := messageDialogContent(message, parent)
	modal := dialog.NewCustomConfirm(title, lang.L("Yes"), lang.L("No"), content, callback, parent)
	modal.Resize(size)
	return modal
}

func showConfirmDialog(title, message string, callback func(bool), parent fyne.Window) {
	newConfirmDialog(title, message, callback, parent).Show()
}
