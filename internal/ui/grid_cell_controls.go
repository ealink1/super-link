package ui

import "fyne.io/fyne/v2/widget"

func (c *gridCell) hideSelectorControls() {
	if c.check != nil {
		c.check.Hide()
	}
	if c.choice != nil {
		c.choice.Hide()
	}
}

// Virtualized text cells need no editor, checkbox or dropdown renderer until
// that control is actually used. Most result columns remain plain text.
func (c *gridCell) ensureCheck() {
	if c.check == nil {
		c.check = widget.NewCheck("", nil)
	}
}
func (c *gridCell) ensureChoice() {
	if c.choice == nil {
		c.choice = widget.NewSelect(nil, nil)
	}
}
func (c *gridCell) ensureEntry() {
	if c.entry != nil {
		return
	}
	c.entry = newGridEditEntry()
	c.entry.Hide()
	c.entry.OnSubmitted = func(string) { _ = c.commitEditor() }
	c.entry.onBlur = func() { _ = c.commitEditor() }
	c.entry.OnChanged = func(string) {
		if c.editing {
			c.rememberEditor()
			if c.model.pending != nil {
				c.model.pending()
			}
		}
	}
}
