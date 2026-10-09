package ui

import (
	"context"
	"fmt"
	"sort"
	"strings"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/driver/desktop"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/application"
	"github.com/ealink1/super-link/internal/domain"
)

type databaseTables struct {
	owner          *Window
	profile        domain.Profile
	scope          string
	item           *container.TabItem
	objects, shown []domain.Object
	list           *widget.Table
	statistics     map[string][]any
	metadataNotice string
	selectedTables map[domain.Object]bool
	dragSelection  *catalogSelectionDrag
	catalogCells   map[*canvas.Rectangle]domain.Object
	deleteButton   *widget.Button
	deleting       bool
	deleteCancel   context.CancelFunc
	hoveredTable   string
	search         *widget.Entry
	status         *widget.Label
	cancel         context.CancelFunc
	generation     uint64
	closed         bool
}

func (w *Window) openDatabaseTables(p domain.Profile, scope string) *databaseTables {
	if w.databases == nil {
		w.databases = make(map[*container.TabItem]*databaseTables)
	}
	for item, page := range w.databases {
		if !page.closed && page.profile.ID == p.ID && page.scope == scope {
			w.tabs.Select(item)
			return page
		}
	}
	page := &databaseTables{owner: w, profile: p, scope: scope, status: widget.NewLabel("正在加载表…")}
	page.search = widget.NewEntry()
	page.search.SetPlaceHolder("筛选表名 / Schema")
	page.search.OnChanged = func(string) { page.filter() }
	page.list = page.buildTableGrid()
	page.deleteButton = page.buildDeleteButton()
	page.updateDeleteButton()
	page.item = container.NewTabItem(scope, container.NewBorder(container.NewBorder(nil, nil, container.NewHBox(widget.NewLabel(scope+" · 所有表"), action("刷新", "refresh", page.refresh), page.deleteButton), shellFixed(page.search, 240, 32), nil), page.status, nil, nil, container.NewBorder(widget.NewSeparator(), nil, nil, nil, container.NewThemeOverride(page.list, catalogGridTheme{fyne.CurrentApp().Settings().Theme()}))))
	w.databases[page.item] = page
	w.tabs.Append(page.item)
	w.tabs.Select(page.item)
	w.syncDocuments()
	page.refresh()
	return page
}

func (p *databaseTables) refresh() {
	if p.closed {
		return
	}
	if p.cancel != nil {
		p.cancel()
	}
	p.generation++
	generation := p.generation
	p.status.SetText("正在加载表…")
	p.owner.jobs.runWithCancel(&p.cancel, func(ctx context.Context) (any, error) {
		ctx, cancel := context.WithTimeout(ctx, 30*time.Second)
		defer cancel()
		return p.owner.Engine.TableCatalog(ctx, p.profile, p.scope)
	}, func(value any, err error) {
		if p.closed || generation != p.generation {
			return
		}
		if err != nil {
			p.status.SetText("加载失败：" + err.Error())
			return
		}
		catalog := value.(application.TableCatalog)
		p.statistics = catalog.Statistics
		p.metadataNotice = ""
		if len(catalog.Statistics) > 0 {
			p.metadataNotice = " · 行数为数据库统计值，可能为估算值"
		}
		if catalog.StatisticsError != nil {
			p.metadataNotice = " · 统计信息加载失败：" + catalog.StatisticsError.Error()
		}
		if catalog.Truncated {
			p.metadataNotice += " · 统计信息达到 10,000 行上限"
		}
		p.objects = nil
		for _, object := range catalog.Objects {
			if object.Kind == "table" || object.Kind == "sql" {
				object.Scope = p.scope
				p.objects = append(p.objects, object)
			}
		}
		sort.Slice(p.objects, func(i, j int) bool {
			a, b := p.objects[i], p.objects[j]
			if a.Schema != b.Schema {
				return a.Schema < b.Schema
			}
			return a.Name < b.Name
		})
		p.filter()
	})
}

func (p *databaseTables) filter() {
	p.endCatalogSelection()
	p.shown = nil
	query := strings.ToLower(strings.TrimSpace(p.search.Text))
	for _, object := range p.objects {
		if strings.Contains(strings.ToLower(object.Schema+"."+object.Name), query) {
			p.shown = append(p.shown, object)
		}
	}
	p.hoveredTable = ""
	p.list.UnselectAll()
	p.pruneTableSelection()
	p.list.Refresh()
	p.updateTableSelectionStatus()
}

func (p *databaseTables) updateTableSelectionStatus() {
	p.updateDeleteButton()
	p.status.SetText(fmt.Sprintf("显示 %d / 共 %d 张表 · 双击打开表数据", len(p.shown), len(p.objects)) + p.metadataNotice + p.tableSelectionNotice())
}

func (w *Window) closeDatabaseTables(p *databaseTables) {
	p.closed = true
	if p.deleteCancel != nil {
		p.deleteCancel()
	}
	if p.cancel != nil {
		p.cancel()
	}
	delete(w.databases, p.item)
	w.tabs.Remove(p.item)
	w.syncDocuments()
}

type databaseTableRow struct {
	widget.Label
	object               domain.Object
	open                 func(domain.Object)
	hoverRow             func(domain.Object, bool)
	contextMenu          func(domain.Object, fyne.Position)
	selectRow            func(domain.Object, fyne.KeyModifier)
	beginSelection       func(domain.Object, fyne.KeyModifier)
	dragSelection        func(int)
	endSelection         func()
	rowIndex             int
	pressRowIndex        int
	pressY, pressOffsetY float32
	modifier             fyne.KeyModifier
	mousePressed         bool
}

func (r *databaseTableRow) DoubleTapped(*fyne.PointEvent) { r.open(r.object) }

func (r *databaseTableRow) Tapped(*fyne.PointEvent) {
	if r.mousePressed {
		r.mousePressed = false
		return
	}
	if r.selectRow != nil {
		r.selectRow(r.object, terminalDesktopModifiers())
	}
}
func (r *databaseTableRow) MouseDown(event *desktop.MouseEvent) {
	if event.Button == desktop.MouseButtonPrimary {
		r.modifier = event.Modifier
		r.mousePressed = true
		r.pressRowIndex = r.rowIndex
		r.pressY, r.pressOffsetY = event.AbsolutePosition.Y, event.Position.Y
		if r.beginSelection != nil {
			r.beginSelection(r.object, event.Modifier)
		} else if r.selectRow != nil {
			r.selectRow(r.object, event.Modifier)
		}
	}
}
func (r *databaseTableRow) MouseUp(*desktop.MouseEvent) {
	if r.endSelection != nil {
		r.endSelection()
	}
}
func (r *databaseTableRow) MouseIn(*desktop.MouseEvent)    { r.hoverRow(r.object, true) }
func (r *databaseTableRow) MouseMoved(*desktop.MouseEvent) {}
func (r *databaseTableRow) MouseOut()                      { r.hoverRow(r.object, false) }

func (r *databaseTableRow) TappedSecondary(event *fyne.PointEvent) {
	r.contextMenu(r.object, event.AbsolutePosition)
}
