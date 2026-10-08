package ui

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/widget"
	"testing"
)

func TestInputMethodCaretForEntryVariants(t *testing.T) {
	app := test.NewApp()
	defer app.Quit()
	window := app.NewWindow("IME")
	defer window.Close()
	variants := []fyne.Widget{widget.NewEntry(), widget.NewPasswordEntry(), widget.NewMultiLineEntry(), newCodeEntry(), newAIInput(func() {}), newNoteEntry(false), newNoteEntry(true), newGridEditEntry()}
	for _, control := range variants {
		provider, ok := control.(inputMethodCaret)
		if !ok {
			t.Fatalf("missing caret interface: %T", control)
		}
		window.SetContent(control)
		window.Resize(fyne.NewSize(400, 200))
		window.Show()
		window.Canvas().Focus(control.(fyne.Focusable))
		if typed, ok := control.(interface{ TypedRune(rune) }); ok {
			typed.TypedRune('字')
		}
		anchor, position, size := provider.InputMethodCaret()
		if anchor == nil || size.Height < 10 || size.Width <= 0 {
			t.Fatalf("missing caret geometry for %T: %v %v", control, position, size)
		}
	}
}
