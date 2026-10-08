package ui

import "fyne.io/fyne/v2"

// Formatting is a single user action, although Entry internally implements a
// replacement as a delete followed by a paste. Keep one bounded document edit
// per change notification so toolbar and keyboard undo have the same meaning.
type noteEdit struct {
	before, after string
	cursor        int
}
type noteEditHistory struct {
	undo, redo []noteEdit
	current    string
	replay     bool
}

func (e *noteEntry) SetText(value string) {
	e.history.replay = true
	e.Entry.SetText(value)
	e.history = noteEditHistory{current: value}
}
func (e *noteEntry) recordEdit() {
	h := &e.history
	if h.replay || h.current == e.Text {
		return
	}
	h.undo = append(h.undo, noteEdit{h.current, e.Text, e.sourceOffset()})
	h.current = e.Text
	h.redo = nil
	bytes := 0
	for _, edit := range h.undo {
		bytes += len(edit.before) + len(edit.after)
	}
	for len(h.undo) > 1 && (len(h.undo) > 64 || bytes > 4<<20) {
		bytes -= len(h.undo[0].before) + len(h.undo[0].after)
		h.undo = h.undo[1:]
	}
}
func (e *noteEntry) Undo() {
	if !e.MultiLine {
		e.Entry.Undo()
		return
	}
	h := &e.history
	if e.Disabled() || len(h.undo) == 0 {
		return
	}
	edit := h.undo[len(h.undo)-1]
	h.undo = h.undo[:len(h.undo)-1]
	h.redo = append(h.redo, edit)
	e.replayEdit(edit.before, min(edit.cursor, len([]rune(edit.before))))
}
func (e *noteEntry) Redo() {
	if !e.MultiLine {
		e.Entry.Redo()
		return
	}
	h := &e.history
	if e.Disabled() || len(h.redo) == 0 {
		return
	}
	edit := h.redo[len(h.redo)-1]
	h.redo = h.redo[:len(h.redo)-1]
	h.undo = append(h.undo, edit)
	e.replayEdit(edit.after, edit.cursor)
}
func (e *noteEntry) replayEdit(value string, cursor int) {
	e.history.replay = true
	e.Entry.SetText(value)
	e.history.current = value
	e.history.replay = false
	e.selectSource(cursor, cursor)
	e.Refresh()
}
func (e *noteEntry) TypedShortcut(shortcut fyne.Shortcut) {
	switch shortcut.(type) {
	case *fyne.ShortcutUndo:
		e.Undo()
	case *fyne.ShortcutRedo:
		e.Redo()
	default:
		e.Entry.TypedShortcut(shortcut)
	}
}
