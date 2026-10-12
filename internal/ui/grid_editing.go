package ui

import (
	"errors"
	"sort"

	"fyne.io/fyne/v2/widget"
)

type gridEditorDraft struct{ original, text string }

func (c *gridCell) rememberEditor() {
	if c.entry.Text == c.originalText {
		delete(c.model.editorDrafts, c.id)
		return
	}
	c.model.editorDrafts[c.id] = gridEditorDraft{original: c.originalText, text: c.entry.Text}
}

// gridEditEntry stages a value on Enter or blur without performing database I/O.
type gridEditEntry struct {
	widget.Entry
	onBlur func()
}

func newGridEditEntry() *gridEditEntry {
	e := &gridEditEntry{}
	e.ExtendBaseWidget(e)
	return e
}
func (e *gridEditEntry) FocusLost() {
	e.Entry.FocusLost()
	if e.onBlur != nil {
		e.onBlur()
	}
}

func (c *gridCell) commitEditor() error {
	if !c.editing {
		return nil
	}
	if c.model.current != nil && !c.model.current() {
		c.editing = false
		return errors.New("this table page is no longer active")
	}
	value := c.entry.Text
	c.editing = false // Refresh/focus changes during staging must not submit twice.
	if value != c.originalText && c.model.edit != nil {
		if err := c.model.edit(c.id.Row, c.id.Col-2, value, false); err != nil {
			c.editing = true
			c.rememberEditor()
			c.entry.SetValidationError(err)
			return err
		}
	}
	delete(c.model.editorDrafts, c.id)
	c.entry.Hide()
	c.text.Show()
	c.bind(c.id)
	return nil
}

func (g *dataGrid) commitEditors() error {
	if g == nil {
		return nil
	}
	for _, c := range g.cells {
		if err := c.commitEditor(); err != nil {
			return err
		}
	}
	// Virtualization may recycle an invalid editor. Its draft still belongs to
	// the original cell and must block submit/paging until corrected/discarded.
	ids := make([]widget.TableCellID, 0, len(g.model.editorDrafts))
	for id := range g.model.editorDrafts {
		ids = append(ids, id)
	}
	sort.Slice(ids, func(i, j int) bool {
		return ids[i].Row < ids[j].Row || ids[i].Row == ids[j].Row && ids[i].Col < ids[j].Col
	})
	for _, id := range ids {
		if g.model.current != nil && !g.model.current() || g.model.edit == nil {
			return errors.New("this table page is no longer active")
		}
		if err := g.model.edit(id.Row, id.Col-2, g.model.editorDrafts[id].text, false); err != nil {
			return err
		}
		delete(g.model.editorDrafts, id)
	}
	return nil
}
func (g *dataGrid) hasPendingEditor() bool {
	if g == nil {
		return false
	}
	if len(g.model.editorDrafts) > 0 {
		return true
	}
	for _, c := range g.cells {
		if c.editing && c.entry.Text != c.originalText {
			return true
		}
	}
	return false
}

func (g *dataGrid) discardEditors() {
	if g == nil {
		return
	}
	clear(g.model.editorDrafts)
	for _, c := range g.cells {
		c.editing = false
		if c.entry != nil {
			c.entry.Hide()
		}
		c.text.Show()
	}
}

func (t *tableWorkspace) stageEditors() bool {
	if err := t.grid.commitEditors(); err != nil {
		t.status.SetText("请先修正单元格输入：" + err.Error())
		return false
	}
	return true
}
func (d *tableDesigner) stageEditors() bool {
	if err := d.grid.commitEditors(); err != nil {
		d.status.SetText("请先修正字段输入：" + err.Error())
		return false
	}
	return true
}
