package ui

import (
	"context"
	"sort"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/application"
	"github.com/ealink1/super-link/internal/domain"
)

type shellWorkspace struct {
	owner                       *Window
	service                     *application.ShellHosts
	content                     *container.ThemeOverride
	body                        *fyne.Container
	hosts                       []domain.ShellHost
	search                      *widget.Entry
	hostList                    *shellHostCollection
	visible                     []domain.ShellHost
	tabs                        *container.DocTabs
	panes                       map[*container.TabItem]*shellPane
	status                      *widget.Label
	page                        int
	navigation                  []*shellNavButton
	navHint                     *widget.Label
	group                       string
	groups                      *fyne.Container
	privacy, listMode, expanded bool
	settings                    *shellSettings
	rail                        *fyne.Container
	tag                         string
}

func newShellWorkspace(w *Window) *shellWorkspace {
	s := &shellWorkspace{owner: w, service: &application.ShellHosts{Store: w.Store, Vault: w.Profiles.Vault}, panes: make(map[*container.TabItem]*shellPane)}
	s.status = widget.NewLabel("")
	s.body = container.NewStack()
	s.tabs = container.NewDocTabs()
	s.tabs.CloseIntercept = func(item *container.TabItem) {
		if p := s.panes[item]; p != nil {
			p.stop()
			delete(s.panes, item)
		}
		s.tabs.Remove(item)
		if len(s.tabs.Items) == 0 {
			s.showTerminals()
		}
	}
	s.tabs.CreateTab = func() *container.TabItem { s.openLocal(""); return nil }
	s.search = widget.NewEntry()
	s.search.SetPlaceHolder("搜索主机...")
	s.search.OnChanged = func(string) { s.filterHosts() }
	s.hostList = newShellHostCollection(s)
	s.loadSettings()
	s.content = container.NewThemeOverride(container.NewStack(shellRectangle(shellBackground, 0, nil), shellBorder(nil, nil, s.buildNavigation(), nil, s.body)), newShellTheme())
	s.showHosts()
	s.reloadHosts()
	return s
}

func (s *shellWorkspace) reloadHosts() {
	s.owner.jobs.run(func(ctx context.Context) (any, error) { return s.service.List(ctx) }, func(value any, err error) {
		if err != nil {
			s.owner.showError(err)
			return
		}
		s.hosts = value.([]domain.ShellHost)
		sort.Slice(s.hosts, func(i, j int) bool { return s.hosts[i].Name < s.hosts[j].Name })
		s.filterHosts()
	})
}
func (s *shellWorkspace) filterHosts() {
	s.visible = nil
	needle := strings.ToLower(s.search.Text)
	for _, h := range s.hosts {
		if strings.Contains(strings.ToLower(h.Name+" "+h.Host+" "+h.Group+" "+h.Notes+" "+h.Tags), needle) {
			if (s.group == "" || shellGroup(h.Group) == s.group) && shellHasTag(h.Tags, s.tag) {
				s.visible = append(s.visible, h)
			}
		}
	}
	s.hostList.Refresh()
	s.refreshGroups()
}
func (s *shellWorkspace) showHosts() {
	s.page = 0
	s.body.Objects = []fyne.CanvasObject{s.buildHostsPage()}
	s.refreshPage()
}
func (s *shellWorkspace) showTerminals() {
	s.page = 1
	s.body.Objects = []fyne.CanvasObject{s.buildTerminalPage()}
	s.refreshPage()
}

func (s *shellWorkspace) refreshPage() {
	for i, button := range s.navigation {
		button.selected = i == s.page
		button.Refresh()
	}
	s.body.Refresh()
	if s.content != nil {
		s.content.Refresh()
	}
}
func (s *shellWorkspace) stop() {
	for _, p := range s.panes {
		p.stop()
	}
}
