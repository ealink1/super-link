package ui

import (
	"fmt"
	"sort"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
)

func (s *shellWorkspace) buildHostsPage() fyne.CanvasObject {
	newHost := shellButton("新建主机", "plus", true, func() { s.editHost(domain.ShellHost{Port: 22, User: "root", Remember: true, OperatingSystem: "Linux"}) })
	privacy := shellButton("隐私模式", "eye-off", false, func() { s.privacy = !s.privacy; s.hostList.Refresh() })
	privacy.Importance = widget.LowImportance
	tools := shellHBox(shellOutlined(privacy), shellFixed(layout.NewSpacer(), 12, 0), shellButtonView(newHost))
	title := shellText("主机管理", 14, true, shellTextColor).(*shellPrimitive)
	title.textOffset = 6
	heading := shellHBox(shellImage("server", true, 24), shellFixed(layout.NewSpacer(), 8, 0), title)
	header := shellPanel(shellFixed(shellBorder(nil, nil, heading, tools, layout.NewSpacer()), 0, 32), shellPanelColor, 0, 12)
	grid := shellButton("", "layout-grid", false, func() { s.listMode = false; s.hostList.Refresh() })
	list := shellButton("", "list", false, func() { s.listMode = true; s.hostList.Refresh() })
	protocol := widget.NewSelect([]string{"SSH"}, nil)
	protocol.SetSelected("SSH")
	filter := shellInset(shellBorder(nil, nil, shellHBox(shellFixed(s.search, 224, 30), shellFixed(layout.NewSpacer(), 12, 0), shellFixed(protocol, 92, 30), shellFixed(layout.NewSpacer(), 12, 0), shellOutlined(shellButton("", "refresh-cw", false, s.reloadHosts))), shellHBox(shellOutlined(grid), shellOutlined(list)), layout.NewSpacer()), 16)
	s.groups = shellVBox()
	s.refreshGroups()
	groupTitle := shellText("分组导航", 11, true, shellMutedColor)
	tags := s.buildTagFilters()
	groupSidebar := shellPanel(shellBorder(shellVBox(groupTitle, shellFixed(layout.NewSpacer(), 0, 10), s.groups), tags, nil, nil, layout.NewSpacer()), shellPanelColor, 0, 12)
	main := shellBorder(shellVBox(filter, shellLine()), nil, nil, nil, shellInset(s.hostList, 16))
	return shellBorder(header, nil, shellFixed(groupSidebar, 224, 0), nil, main)
}

func shellGroup(group string) string {
	if strings.TrimSpace(group) == "" {
		return "默认分组"
	}
	return group
}

func shellHasTag(raw, tag string) bool {
	if tag == "" {
		return true
	}
	for _, item := range strings.FieldsFunc(raw, func(r rune) bool { return r == ',' || r == '，' }) {
		if strings.TrimSpace(item) == tag {
			return true
		}
	}
	return false
}

func (s *shellWorkspace) refreshGroups() {
	if s.groups == nil {
		return
	}
	counts := make(map[string]int)
	counts["默认分组"] = 0
	for _, h := range s.hosts {
		counts[shellGroup(h.Group)]++
	}
	names := make([]string, 0, len(counts))
	for name := range counts {
		names = append(names, name)
	}
	sort.Strings(names)
	all := shellButton("全部主机", "layout-grid", false, func() { s.group = ""; s.filterHosts() })
	all.Alignment = widget.ButtonAlignLeading
	var allView fyne.CanvasObject = shellButtonView(all)
	if s.group == "" {
		allView = shellTinted(all)
	}
	s.groups.Objects = []fyne.CanvasObject{shellFixed(allView, 0, 30), shellFixed(layout.NewSpacer(), 0, 12), shellLine(), shellFixed(layout.NewSpacer(), 0, 12)}
	for _, name := range names {
		group := name
		button := shellButton(fmt.Sprintf("%s  %d", name, counts[name]), "folder", false, func() { s.group = group; s.filterHosts() })
		button.Alignment = widget.ButtonAlignLeading
		var view fyne.CanvasObject = shellButtonView(button)
		if s.group == name {
			view = shellTinted(button)
		}
		s.groups.Add(shellFixed(view, 0, 32))
	}
	s.groups.Refresh()
	if s.content != nil {
		s.content.Refresh()
	}
}
