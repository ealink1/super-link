package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/widget"
)

func messageDialogBody(message string) *widget.Label {
	label := widget.NewLabel(message)
	label.Alignment = fyne.TextAlignCenter
	label.Wrapping = fyne.TextWrapWord
	return label
}

func newInformationDialog(title, message string, parent fyne.Window) *cardDialog {
	return newCardDialog(cardSpec{
		Title:   title,
		Body:    messageDialogBody(message),
		Width:   cardDefaultWidth,
		Actions: []cardAction{{Label: "确定", Primary: true, Accept: true}},
	}, parent, nil)
}

func showInformationDialog(title, message string, parent fyne.Window) {
	newInformationDialog(title, message, parent).Show()
}

func showErrorDialog(err error, parent fyne.Window) {
	newInformationDialog("出错了", err.Error(), parent).Show()
}

func newConfirmDialog(title, message string, callback func(bool), parent fyne.Window) *cardDialog {
	return newCardDialog(cardSpec{
		Title: title,
		Body:  messageDialogBody(message),
		Width: cardDefaultWidth,
		Actions: []cardAction{
			{Label: "取消"},
			{Label: "确定", Primary: true, Accept: true},
		},
	}, parent, callback)
}

func showConfirmDialog(title, message string, callback func(bool), parent fyne.Window) {
	newConfirmDialog(title, message, callback, parent).Show()
}
