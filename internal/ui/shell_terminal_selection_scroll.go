package ui

import (
	"time"

	"fyne.io/fyne/v2"
)

func (t *terminalSurface) selectionScrollDirection() int {
	if t.altScreen {
		return 0
	}
	if t.selection.pointer.Y < 0 && t.scroll < t.emulator.ScrollbackLen() {
		return 1
	}
	if t.selection.pointer.Y >= t.Size().Height && t.scroll > 0 {
		return -1
	}
	return 0
}
func (t *terminalSurface) updateSelectionScroll() {
	if t.selectionScrollDirection() == 0 {
		t.stopSelectionScroll()
		return
	}
	if t.selection.animation != nil {
		return
	}
	// Driver animations tick on the UI goroutine; no timer goroutine touches
	// the emulator or outlives a disposed terminal.
	animation := fyne.NewAnimation(time.Second, func(float32) {
		if time.Since(t.selection.lastScroll) < 80*time.Millisecond {
			return
		}
		t.stepSelectionScroll()
	})
	animation.RepeatCount = fyne.AnimationRepeatForever
	t.selection.animation = animation
	t.selection.lastScroll = time.Now()
	animation.Start()
}
func (t *terminalSurface) stepSelectionScroll() {
	if t.closed || !t.selection.down || t.selection.remote || t.active != nil && !t.active() {
		t.endSelectionDrag()
		return
	}
	direction := t.selectionScrollDirection()
	if direction == 0 {
		t.stopSelectionScroll()
		return
	}
	t.selection.lastScroll = time.Now()
	t.scroll = min(t.emulator.ScrollbackLen(), max(0, t.scroll+direction))
	t.selection.head = t.selectionPoint(t.selection.pointer)
	t.Refresh()
}
func (t *terminalSurface) stopSelectionScroll() {
	if t.selection.animation != nil {
		t.selection.animation.Stop()
		t.selection.animation = nil
	}
}
