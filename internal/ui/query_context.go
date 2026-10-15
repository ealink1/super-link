package ui

import (
	"fmt"
	"slices"

	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
)

func (s *workspace) buildConnectionPicker() *widget.Select {
	profiles := map[string]domain.Profile{}
	names := []string{}
	selected := ""
	for _, p := range s.owner.profiles {
		d, err := domain.Resolve(p.Config.Type)
		if err != nil || d.Family != domain.SQL {
			continue
		}
		label := p.Name
		if _, exists := profiles[label]; exists {
			label = fmt.Sprintf("%s [%s]", p.Name, p.ID[:min(8, len(p.ID))])
		}
		profiles[label] = p
		names = append(names, label)
		if p.ID == s.profile.ID {
			selected = label
		}
	}
	picker := widget.NewSelect(names, nil)
	picker.SetSelected(selected)
	picker.OnChanged = func(label string) {
		p, ok := profiles[label]
		if !ok || s.busy || p.ID == s.profile.ID {
			return
		}
		d, err := domain.Resolve(p.Config.Type)
		if err != nil {
			return
		}
		s.profile = p
		s.descriptor = d
		if s.completion != nil {
			s.completion.reset()
		}
		s.title, s.savedID, s.savedRevision = "", "", 0
		s.scope.SetText(p.Config.Database)
		s.setScopeOptions(nil)
		s.schemaName = ""
		s.schemaPicker.ClearSelected()
		s.owner.selected = p.ID
		s.scheduleSave()
		s.owner.syncDocuments()
		s.loadScopes()
	}
	s.connectionPicker = picker
	return picker
}
func (s *workspace) updateSchemas() {
	if s.schemaPicker == nil {
		return
	}
	names := []string{}
	for _, object := range s.objects {
		if object.Schema != "" && !slices.Contains(names, object.Schema) {
			names = append(names, object.Schema)
		}
	}
	slices.Sort(names)
	if len(names) == 0 {
		s.schemaPicker.Hide()
		s.schemaHost.Hide()
		s.schemaName = ""
		return
	}
	s.schemaPicker.SetOptions(names)
	s.schemaPicker.Show()
	s.schemaHost.Show()
	selected := s.schemaName
	if !slices.Contains(names, selected) {
		selected = names[0]
		if slices.Contains(names, "public") {
			selected = "public"
		}
	}
	s.schemaPicker.SetSelected(selected)
}
