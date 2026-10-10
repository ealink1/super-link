package ui

import (
	"fmt"

	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/sqlworkbench"
)

func (d *tableDesigner) refreshIndexes() {
	if d.views != nil {
		d.views.Items[1].Content = d.indexPanel()
		d.views.Refresh()
	}
}

func (d *tableDesigner) removeIndexes(selected map[int]bool, undo bool) {
	if !d.canEdit() || !d.stageEditors() {
		return
	}
	old, added := map[string]bool{}, map[string]bool{}
	existing := indexDisplayRows(d.info.Indexes)
	for row, value := range selected {
		if value && row >= 0 && row < len(existing) {
			old[existing[row].name] = true
		}
	}
	row := len(existing)
	for _, change := range d.indexChanges {
		if change.Kind == "addIndex" {
			if selected[row] && !undo {
				added[change.Index.Name] = true
			}
			row++
		}
	}
	kept := make([]domain.StructureChange, 0, len(d.indexChanges)+len(old))
	dropped := map[string]bool{}
	for _, change := range d.indexChanges {
		if change.Kind == "addIndex" && added[change.Index.Name] || change.Kind == "dropIndex" && old[change.OriginalName] && undo {
			continue
		}
		if change.Kind == "dropIndex" {
			dropped[change.OriginalName] = true
		}
		kept = append(kept, change)
	}
	if !undo {
		for _, index := range d.info.Indexes {
			if old[index.Name] && !dropped[index.Name] {
				kept = append(kept, domain.StructureChange{Kind: "dropIndex", OriginalName: index.Name})
				dropped[index.Name] = true
			}
		}
	}
	d.indexChanges = kept
	d.refreshIndexes()
	d.status.SetText("索引修改已暂存，保存前请查看 SQL。")
}

func (d *tableDesigner) newIndexSQL(index domain.NewIndex) ([]string, error) {
	request := d.request()
	request.Changes = append(request.Changes, domain.StructureChange{Kind: "addIndex", Index: index})
	return sqlworkbench.BuildStructure(d.profile.SQLDialect(), request, d.info)
}

func (d *tableDesigner) previewNewIndex(index domain.NewIndex, entry *widget.Entry, submit *widget.Button) {
	statements, err := d.newIndexSQL(index)
	if err != nil {
		entry.SetText(err.Error())
		submit.Disable()
		return
	}
	entry.SetText(sqlworkbench.PreviewStructure(statements))
	if d.canEdit() {
		submit.Enable()
	} else {
		submit.Disable()
	}
}

func (d *tableDesigner) stageNewIndex(index domain.NewIndex) bool {
	if !d.canEdit() || !d.stageEditors() {
		return false
	}
	if _, err := d.newIndexSQL(index); err != nil {
		d.status.SetText(err.Error())
		return false
	}
	d.indexChanges = append(d.indexChanges, domain.StructureChange{Kind: "addIndex", Index: index})
	d.refreshIndexes()
	d.status.SetText(fmt.Sprintf("索引 %s 已暂存", index.Name))
	return true
}
