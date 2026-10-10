package ui

import (
	"context"
	"fmt"
	"strconv"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/layout"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/sqlworkbench"
)

type tableWorkspace struct {
	owner                   *Window
	profile                 domain.Profile
	object                  domain.Object
	item                    *container.TabItem
	request                 domain.TableRequest
	page                    domain.TablePage
	body                    *fyne.Container
	status, paging          *widget.Label
	condition, jump         *widget.Entry
	pageSize                *widget.Select
	filterPanel             *fyne.Container
	filterRows, sortRows    *fyne.Container
	filters                 []*filterRow
	sorts                   []*sortRow
	views                   *resultTabs
	cancel                  context.CancelFunc
	generation              uint64
	busy, closed, uncertain bool
	edits                   map[int]domain.RowChange
	inserts                 []domain.RowChange
	selected                map[int]bool
	grid                    *dataGrid
	commitButton            *tableActionButton
}

func (w *Window) openTable(p domain.Profile, object domain.Object) *tableWorkspace {
	for item, t := range w.tables {
		if !t.closed && t.profile.ID == p.ID && t.object == object {
			w.tabs.Select(item)
			return t
		}
	}
	t := &tableWorkspace{owner: w, profile: p, object: object, request: domain.TableRequest{Object: object, Page: 1, Size: 100}, edits: map[int]domain.RowChange{}, selected: map[int]bool{}}
	t.body = container.NewStack(widget.NewLabel("正在加载表数据…"))
	t.status = widget.NewLabel("")
	t.paging = widget.NewLabel("")
	t.condition = widget.NewEntry()
	t.condition.SetPlaceHolder("手动查询条件，例如 id > 100")
	t.jump = widget.NewEntry()
	t.jump.SetText("1")
	t.pageSize = widget.NewSelect([]string{"50", "100", "200", "500", "1000"}, nil)
	t.pageSize.SetSelected("100")
	t.pageSize.OnChanged = t.changePageSize
	t.filterPanel = t.buildFilter()
	t.filterPanel.Hide()
	t.views = newResultTabs(container.NewTabItem("数据", t.body), container.NewTabItem("字段", widget.NewLabel("加载元数据后显示")), container.NewTabItem("DDL", widget.NewLabel("加载元数据后显示")))
	t.views.bottom = true
	t.commitButton = w.tableAction("save", func() { t.submitChanges("") })
	t.commitButton.Disable()
	tools := container.NewHBox(w.tableAction("refresh", t.refresh), w.tableAction("filter", func() {
		if t.filterPanel.Visible() {
			t.filterPanel.Hide()
		} else {
			t.filterPanel.Show()
		}
	}), w.tableAction("add-row", t.addRow), w.tableAction("trash", t.deleteRows), w.tableAction("cell-select", t.previewChanges), t.commitButton, w.tableAction("rollback", t.discardEdits), w.tableAction("table-design", t.design), w.tableAction("copy", t.copy), w.tableAction("export", t.export), w.tableAction("sql-doc", t.newQuery))
	tools.Add(w.tableAction("import", func() { w.importTable(t.profile, t.object, t.page.Info) }))
	top := container.NewVBox(tools, t.filterPanel)
	footer := container.New(pagingRowLayout{}, layout.NewSpacer(), t.paging, action("首页", "", func() { t.gotoPage(1) }), action("上一页", "", func() { t.gotoPage(t.request.Page - 1) }), t.jump, action("跳", "", func() {
		page, err := strconv.Atoi(t.jump.Text)
		if err == nil {
			t.gotoPage(page)
		}
	}), action("下一页", "", func() { t.gotoPage(t.request.Page + 1) }), action("尾页", "", func() { t.gotoPage(t.pageCount()) }), t.pageSize)
	content := container.NewBorder(top, container.NewVBox(t.status, footer), nil, nil, t.views)
	t.item = container.NewTabItem(object.Name, content)
	w.tables[t.item] = t
	w.tabs.Append(t.item)
	w.tabs.Select(t.item)
	w.syncDocuments()
	t.refresh()
	return t
}

func (t *tableWorkspace) refresh() {
	if t.closed || t.busy || t.body == nil {
		return
	}
	if !t.stageEditors() {
		return
	}
	if t.dirty() {
		t.status.SetText("请先提交或丢弃当前修改，再刷新或切换分页。")
		return
	}
	t.busy = true
	t.generation++
	generation := t.generation
	request := t.request
	request.Revision = t.profile.Revision
	profileID := t.profile.ID
	t.status.SetText("正在读取表结构与数据…")
	t.owner.jobs.runWithCancel(&t.cancel, func(ctx context.Context) (any, error) {
		ctx, cancel := context.WithTimeout(ctx, 30*time.Second)
		defer cancel()
		return t.owner.Engine.TablePage(ctx, profileID, request)
	}, func(value any, err error) {
		if t.closed || generation != t.generation {
			return
		}
		t.busy = false
		if err != nil {
			t.status.SetText("读取失败：" + err.Error())
			return
		}
		t.page = value.(domain.TablePage)
		if t.page.Revision != t.profile.Revision {
			t.status.SetText("连接配置已改变，请刷新后继续。")
			return
		}
		t.showPage()
	})
}

func (t *tableWorkspace) showPage() {
	t.body.Objects = []fyne.CanvasObject{t.tableGrid()}
	t.body.Refresh()
	t.views.Items[1].Content = t.fieldsView()
	ddl := widget.NewMultiLineEntry()
	ddl.SetText(t.page.Info.DDL)
	ddl.TextStyle = fyne.TextStyle{Monospace: true}
	ddl.Disable()
	t.views.Items[2].Content = container.NewThemeOverride(ddl, ddlTextTheme{fyne.CurrentApp().Settings().Theme()})
	t.views.Refresh()
	t.paging.SetText(fmt.Sprintf("当前 %d 条 / 共 %d 条   %d / %d", len(t.page.Result.Rows), t.page.Total, t.page.Page, t.pageCount()))
	t.jump.SetText(strconv.Itoa(t.request.Page))
	t.status.SetText(fmt.Sprintf("读取完成 · %s", t.page.Result.Duration.Round(time.Millisecond)))
	if len(t.page.Info.Warnings) > 0 {
		t.status.SetText(t.status.Text + " · 部分元数据不可用")
	}
}

func (t *tableWorkspace) pageCount() int {
	size := t.page.Size
	if size <= 0 {
		size = max(1, t.request.Size)
	}
	return max(1, int((t.page.Total+int64(size)-1)/int64(size)))
}
func (t *tableWorkspace) gotoPage(page int) {
	if !t.stageEditors() {
		return
	}
	if page < 1 || page > t.pageCount() || t.busy || t.dirty() {
		return
	}
	t.request.Page = page
	t.refresh()
}

func (t *tableWorkspace) changePageSize(value string) {
	if t.busy || t.dirty() {
		t.pageSize.OnChanged = nil
		t.pageSize.SetSelected(strconv.Itoa(t.request.Size))
		t.pageSize.OnChanged = t.changePageSize
		return
	}
	size, err := strconv.Atoi(value)
	if err != nil || size < 1 {
		return
	}
	t.request.Size = size
	t.request.Page = 1
	t.refresh()
}
func (t *tableWorkspace) columnNames() []string {
	names := make([]string, 0, len(t.page.Info.Columns))
	for _, c := range t.page.Info.Columns {
		names = append(names, c.Name)
	}
	return names
}
func (t *tableWorkspace) newQuery() {
	s := t.owner.openWorkspace(t.profile, domain.Draft{})
	s.scope.SetText(t.object.Scope)
	s.schemaName = t.object.Schema
	s.selectedObject = &t.object
	s.previewObject()
	s.refreshObjects()
}
func (t *tableWorkspace) copy() { t.owner.Window.Clipboard().SetContent(t.object.Name) }
func (t *tableWorkspace) export() {
	query, err := sqlworkbench.BuildExportQuery(t.profile.SQLDialect(), t.request, t.page.Info)
	if err != nil {
		t.owner.showError(err)
		return
	}
	name, _ := sqlworkbench.ObjectName(t.profile.SQLDialect(), t.object)
	t.owner.exportDialog(exportSource{profile: t.profile, result: t.page.Result, request: domain.Execution{Text: query, Scope: t.object.Scope, Revision: t.page.Revision}, table: name})
}
func (t *tableWorkspace) design() { t.owner.designTable(t.profile, t.object, t.page.Info) }
