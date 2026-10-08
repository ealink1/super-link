package ui

import (
	"context"
	"errors"
	"fmt"
	"runtime"
	"strings"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/application"
	"github.com/ealink1/super-link/internal/domain"
	"github.com/ealink1/super-link/internal/infra/drivers"
	"github.com/ealink1/super-link/internal/infra/release"
	"github.com/ealink1/super-link/internal/infra/state"
	"github.com/ealink1/super-link/internal/infra/update"
)

type Dependencies struct {
	Profiles      *application.Profiles
	Engine        *application.Engine
	Drivers       *drivers.Manager
	Releases      *release.Client
	Root, Version string
	Close         func() error
	dispatch      func(func()) // Native default is fyne.Do; tests drive a serial UI queue.
}
type Window struct {
	App                         fyne.App
	Window                      fyne.Window
	Profiles                    *application.Profiles
	Engine                      *application.Engine
	Store                       *state.Store
	Root                        string
	Version                     string
	Drivers                     *drivers.Manager
	Releases                    *release.Client
	jobs                        *tasks
	profiles                    []domain.Profile
	visible                     []domain.Profile
	list                        *widget.List
	search                      *widget.Entry
	tabs                        *documents
	workspaces                  map[*container.TabItem]*workspace
	status                      *widget.Label
	selected                    string
	dark                        bool
	appearance                  *appearanceState
	shuttingDown                bool
	stopInputMethods            func()
	onClose                     func() error
	ready                       chan struct{}
	startupUpdateScheduled      bool
	updateChecking              bool
	pendingUpdate               *update.Request
	sidebar                     *navigator
	docHeader, docBody, docHost *fyne.Container
	docStrip                    *fyne.Container
	docButtons                  map[*container.TabItem]*documentTab
	docTooltip                  *documentTooltip
	switcher                    *workspaceSwitcher
	tables                      map[*container.TabItem]*tableWorkspace
	databases                   map[*container.TabItem]*databaseTables
	imports                     map[*container.TabItem]*importWorkbench
	designers                   map[*container.TabItem]*tableDesigner
	dispatch                    func(func())
}

func New(app fyne.App, deps Dependencies) *Window {
	w := &Window{App: app, Window: app.NewWindow("SuperLink"), Profiles: deps.Profiles, Engine: deps.Engine, Store: deps.Profiles.Store, Root: deps.Root, Version: deps.Version, Drivers: deps.Drivers, Releases: deps.Releases, jobs: newTasks(deps.dispatch), workspaces: map[*container.TabItem]*workspace{}, onClose: deps.Close, dispatch: deps.dispatch}
	w.ready = make(chan struct{})
	w.tables = make(map[*container.TabItem]*tableWorkspace)
	w.imports = make(map[*container.TabItem]*importWorkbench)
	w.designers = make(map[*container.TabItem]*tableDesigner)
	w.appearance = &appearanceState{palette: &appearancePalette{}}
	app.Settings().SetTheme(Theme{palette: w.appearance.palette})
	w.status = widget.NewLabel("正在载入本地工作区…")
	w.tabs = &documents{}
	w.tabs.CreateTab = func() *container.TabItem { w.newSelectedQuery(); return nil }
	w.tabs.CloseIntercept = w.closeTab
	w.search = widget.NewEntry()
	w.search.SetPlaceHolder("搜索连接 / 类型 / 分组")
	w.search.OnChanged = func(string) { w.filter() }
	w.list = w.connectionList()

	w.buildShell()
	w.Window.Resize(fyne.NewSize(1320, 860))
	w.Window.SetMaster()
	w.Window.SetCloseIntercept(w.shutdown)
	w.Window.SetOnClosed(func() {
		if w.stopInputMethods != nil {
			w.stopInputMethods()
		}
		w.switcher.stop()
		if w.switcher.nativeClose != nil {
			w.switcher.nativeClose()
		}
	})
	quit := fyne.NewMenuItem("退出", w.shutdown)
	quit.IsQuit = true
	if runtime.GOOS != "windows" {
		w.Window.SetMainMenu(fyne.NewMainMenu(fyne.NewMenu("文件", fyne.NewMenuItem("新建连接", w.newWorkspaceConnection), fyne.NewMenuItem("SQL 草稿", func() { w.switcher.selectMode(0); w.draftManager() }), quit)))
	}
	w.load()
	w.checkUpdateReport()
	return w
}
func (w *Window) Show() {
	w.Window.Show()
	if w.stopInputMethods == nil {
		w.stopInputMethods = startInputMethods(w.App, w.jobs.dispatch)
	}
	fyne.Do(w.switcher.installNative)
}
func (w *Window) Ready() <-chan struct{} { return w.ready }
func (w *Window) loadProfiles() {
	w.jobs.run(func(ctx context.Context) (any, error) { return w.Profiles.List(ctx) }, func(value any, err error) {
		if err != nil {
			w.showError(err)
			return
		}
		w.profiles = value.([]domain.Profile)
		w.filter()
		w.status.SetText(fmt.Sprintf("%d 个连接 · v%s", len(w.profiles), w.Version))
		w.restoreDrafts()
	})
}
func (w *Window) reload() {
	w.jobs.run(func(ctx context.Context) (any, error) { return w.Profiles.List(ctx) }, func(value any, err error) {
		if err != nil {
			w.showError(err)
			return
		}
		w.profiles = value.([]domain.Profile)
		w.filter()
	})
}
func (w *Window) filter() {
	needle := strings.ToLower(strings.TrimSpace(w.search.Text))
	w.visible = nil
	for _, p := range w.profiles {
		haystack := strings.ToLower(p.Name + " " + p.Config.Type + " " + p.Group)
		if strings.Contains(haystack, needle) {
			w.visible = append(w.visible, p)
		}
	}
	// List selection uses row indices. Preserve identity when filtering or
	// reordering, so the highlighted row cannot refer to another connection.
	selected := w.selected
	w.list.UnselectAll()
	w.selected = ""
	w.list.Refresh()
	for index, p := range w.visible {
		if p.ID == selected {
			w.list.Select(index)
			w.list.Highlight(index)
			break
		}
	}
	if w.sidebar != nil {
		w.sidebar.syncProfiles()
	}
}
func (w *Window) selectedProfile() (domain.Profile, bool) {
	for _, p := range w.profiles {
		if p.ID == w.selected {
			return p, true
		}
	}
	w.status.SetText("请先选择一个连接")
	return domain.Profile{}, false
}
func (w *Window) openSelected() {
	if p, ok := w.selectedProfile(); ok {
		for item, space := range w.workspaces {
			if space.profile.ID == p.ID && !space.closed {
				w.tabs.Select(item)
				return
			}
		}
		w.newQuery(p)
	}
}
func (w *Window) newSelectedQuery() {
	if p, ok := w.selectedProfile(); ok {
		if w.sidebar != nil {
			if node := w.sidebar.nodes[w.sidebar.selected]; node != nil && node.profileID == p.ID {
				w.sidebar.queryForNode(node)
				return
			}
		}
		w.newQuery(p)
	}
}
func (w *Window) newQuery(p domain.Profile) {
	space := w.openWorkspace(p, domain.Draft{})
	if space == nil {
		return
	}
	if strings.TrimSpace(space.scope.Text) == "" && (space.descriptor.Family == domain.SQL || space.descriptor.Family == domain.Document || space.descriptor.Family == domain.Configuration) {
		space.loadScopes()
	} else {
		space.refreshObjects()
	}
}
func (w *Window) editSelected() {
	p, ok := w.selectedProfile()
	if !ok {
		return
	}
	w.jobs.run(func(ctx context.Context) (any, error) { return w.Profiles.Get(ctx, p.ID) }, func(value any, err error) {
		if err != nil {
			// Credential hydration can fail after metadata has been read. Use the
			// current revision and show the error inside the editor, so saving or
			// cancelling cannot reveal a stale information dialog underneath it.
			if current, ok := value.(domain.Profile); ok && current.ID == p.ID && current.Revision > 0 {
				p = current
			}
			w.editProfileWithHint(p, err.Error())
			return
		}
		w.editProfile(value.(domain.Profile))
	})
}
func (w *Window) disconnectSelected() {
	p, ok := w.selectedProfile()
	if !ok {
		return
	}
	w.cancelProfile(p.ID)
	w.jobs.run(func(ctx context.Context) (any, error) {
		ctx, cancel := context.WithTimeout(ctx, 20*time.Second)
		defer cancel()
		return nil, w.Engine.Disconnect(ctx, p.ID)
	}, func(_ any, err error) {
		if err != nil {
			w.showError(err)
		} else {
			w.status.SetText("已断开：" + p.Name)
			if w.sidebar != nil {
				w.sidebar.resetDisconnectedConnection(p.ID)
			}
		}
	})
}
func (w *Window) deleteSelected() {
	p, ok := w.selectedProfile()
	if !ok {
		return
	}
	dialog.ShowConfirm("删除连接", "删除「"+p.Name+"」及其本地草稿和历史？", func(ok bool) {
		if !ok {
			return
		}
		w.cancelProfile(p.ID)
		w.jobs.run(func(ctx context.Context) (any, error) {
			if err := w.Engine.Disconnect(ctx, p.ID); err != nil {
				return nil, err
			}
			return nil, w.Profiles.Delete(ctx, p.ID, p.Revision)
		}, func(_ any, err error) {
			if err != nil {
				w.showError(err)
				return
			}
			w.removeProfileDocuments(p.ID)
			w.selected = ""
			w.reload()
		})
	}, w.Window)
}
func (w *Window) cancelProfile(id string) {
	for _, page := range w.databases {
		if page.profile.ID == id && page.cancel != nil {
			page.cancel()
		}
	}
	for _, space := range w.workspaces {
		if space.profile.ID == id && space.cancel != nil {
			space.cancel()
		}
	}
	for _, t := range w.tables {
		if t.profile.ID == id {
			if t.cancel != nil {
				t.cancel()
			}
			t.status.SetText("正在取消请求…")
		}
	}
	for _, i := range w.imports {
		if i.profile.ID == id && i.cancel != nil {
			i.cancel()
		}
	}
	for _, d := range w.designers {
		if d.profile.ID == id && d.cancel != nil {
			d.cancel()
		}
	}
}
func (w *Window) showError(err error) {
	if errors.Is(err, context.Canceled) {
		w.status.SetText("操作已取消")
		return
	}
	dialog.ShowError(err, w.Window)
}
func (w *Window) about() {
	dialog.ShowInformation("SuperLink", fmt.Sprintf("版本 %s\n独立 Go + Fyne 项目\n本地目录：%s\n数据库权限是最终保护边界。\n记住的密码加密保存在本机，草稿可能包含业务数据。", w.Version, w.Root), w.Window)
}
func (w *Window) shutdown() {
	if w.shuttingDown {
		return
	}
	dirty := 0
	for _, t := range w.tables {
		if t.dirty() {
			dirty++
		}
	}
	for _, d := range w.designers {
		if d.dirty() {
			dirty++
		}
	}
	if dirty > 0 {
		dialog.ShowConfirm("未提交的修改", fmt.Sprintf("%d 个页面存在未提交的数据或结构修改。丢弃这些修改并退出？", dirty), func(ok bool) {
			if ok {
				for _, t := range w.tables {
					t.clearEdits()
				}
				for _, d := range w.designers {
					d.reset(d.info)
				}
				w.shutdown()
			}
		}, w.Window)
		return
	}
	flushNotes := w.noteShutdownSnapshot()
	w.shuttingDown = true
	w.switcher.stop()
	w.status.SetText("正在取消操作并保存草稿和笔记…")
	// Snapshot UI state on the UI goroutine before joining background workers.
	drafts := []domain.Draft{}
	for _, space := range w.workspaces {
		space.cancelOperation()
		drafts = append(drafts, space.draft())
	}
	w.jobs.closing.Store(true)
	w.jobs.cancel()
	dark := w.dark
	go func() {
		w.jobs.wg.Wait()
		ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
		defer cancel()
		err := w.saveAppearance(ctx, dark)
		err = errors.Join(err, flushNotes(ctx))
		for _, draft := range drafts {
			err = errors.Join(err, w.Store.SaveDraft(ctx, draft))
		}
		if err != nil {
			fyne.Do(func() {
				w.jobs = newTasks(w.dispatch)
				w.restartNoteAutosave()
				w.shuttingDown = false
				w.restartAI()
				for _, space := range w.workspaces {
					space.finish()
				}
				w.pendingUpdate = nil
				w.showError(fmt.Errorf("草稿或笔记尚未保存，关闭已停止：%w", err))
			})
			return
		}
		err = errors.Join(err, w.onClose())
		if err == nil && w.pendingUpdate != nil {
			err = update.Launch(context.Background(), *w.pendingUpdate)
		}
		fyne.Do(func() {
			if err != nil {
				w.Window.SetCloseIntercept(nil)
				modal := dialog.NewInformation("关闭时出现问题", err.Error(), w.Window)
				modal.SetOnClosed(w.App.Quit)
				modal.Show()
				return
			}
			w.App.Quit()
		})
	}()
}

// FlushAfterRun handles termination paths such as SIGTERM after the native
// event loop has stopped. No rendering or callback dispatch is performed here.
func (w *Window) FlushAfterRun() error {
	if w.shuttingDown {
		return nil // The normal close path already saved drafts and closed services.
	}
	w.jobs.closing.Store(true)
	w.jobs.cancel()
	w.jobs.wg.Wait()
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancel()
	err := w.saveAppearance(ctx, w.dark)
	err = errors.Join(err, w.noteShutdownSnapshot()(ctx))
	for _, space := range w.workspaces {
		err = errors.Join(err, w.Store.SaveDraft(ctx, space.draft()))
	}
	return err
}
