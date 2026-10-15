package ui

import (
	"context"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/driver/desktop"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/domain"
	adapter "github.com/ealink1/super-link/internal/infra/runtime"
	"github.com/ealink1/super-link/internal/upstream/connection"
)

type completionClient struct {
	parityFixture
	objects, columns atomic.Int32
	block            <-chan struct{}
	started          chan struct{}
}

func (c *completionClient) Objects(ctx context.Context, scope string) ([]domain.Object, error) {
	c.objects.Add(1)
	if c.started != nil {
		select {
		case c.started <- struct{}{}:
		default:
		}
	}
	if c.block != nil {
		select {
		case <-c.block:
		case <-ctx.Done():
			return nil, ctx.Err()
		}
	}
	return []domain.Object{{Name: "users", Schema: "public", Scope: scope, Kind: "table"}, {Name: "user_logs", Schema: "public", Scope: scope, Kind: "view"}}, nil
}

func (c *completionClient) TableInfo(context.Context, string, string) (domain.TableInfo, error) {
	c.columns.Add(1)
	return domain.TableInfo{Columns: []connection.ColumnDefinition{{Name: "id"}, {Name: "name"}, {Name: "nickname"}}}, nil
}

func completionWorkspace(t *testing.T, client *completionClient) (*Window, *workspace) {
	t.Helper()
	w, p := parityWindow(t)
	if err := w.Engine.Disconnect(t.Context(), p.ID); err != nil {
		t.Fatal(err)
	}
	w.Engine.Factory = func(context.Context, domain.Profile) (adapter.Client, error) { return client, nil }
	s := w.openWorkspace(p, domain.Draft{ID: "completion", Scope: "main"})
	w.Window.Canvas().Focus(s.code)
	return w, s
}

func completionSource(s *workspace, source string) {
	parts := strings.SplitN(source, "|", 2)
	s.editor.SetText(strings.Join(parts, ""))
	s.code.Entry.TypedKey(&fyne.KeyEvent{Name: fyne.KeyRight})
	lines := strings.Split(parts[0], "\n")
	s.editor.CursorRow, s.editor.CursorColumn = len(lines)-1, len([]rune(lines[len(lines)-1]))
	s.editor.Refresh()
}

func TestSQLCompletionAutomaticallySuggestsAndAcceptsKeywords(t *testing.T) {
	w, s := completionWorkspace(t, &completionClient{})
	s.code.TypedRune('s')
	w.Window.Canvas().Focused().TypedRune('e')
	view := s.completion.view
	if !view.visible() || view.items[0].Insert != "select" || w.Window.Canvas().Focused() != view.keyboard {
		t.Fatalf("typing did not show keyword candidates without stealing focus: text=%q items=%+v visible=%v requested=%v focused=%T selection=%q", s.editor.Text, view.items, view.visible(), view.requested, w.Window.Canvas().Focused(), s.editor.SelectedText())
	}
	s.code.TypedKey(&fyne.KeyEvent{Name: fyne.KeyReturn})
	if s.editor.Text != "select" || view.visible() || len(s.lastResults) != 0 {
		t.Fatal("Enter did not complete only the SQL prefix", s.editor.Text)
	}
	s.code.TypedRune(' ')
	if view.visible() {
		t.Fatal("blank expression prefix displayed unwanted suggestions")
	}
	s.code.TypedKey(&fyne.KeyEvent{Name: fyne.KeyTab})
	if s.editor.Text != "select \t" {
		t.Fatal("Tab outside completion lost normal indentation", s.editor.Text)
	}
	waitUI(t, w)
}

func TestSQLCompletionLoadsTablesAndAliasColumnsOnce(t *testing.T) {
	client := &completionClient{}
	w, s := completionWorkspace(t, client)
	completionSource(s, "SELECT * FROM us|")
	s.code.completion.trigger(false)
	waitUI(t, w)
	view := s.completion.view
	if !view.visible() || len(view.items) != 2 || view.items[0].Kind != "表" {
		t.Fatal("table candidates missing", view.items)
	}
	s.code.TypedKey(&fyne.KeyEvent{Name: fyne.KeyDown})
	want := view.items[view.selected].Insert
	s.code.TypedKey(&fyne.KeyEvent{Name: fyne.KeyTab})
	if s.editor.Text != "SELECT * FROM "+want {
		t.Fatal("Tab did not insert selected table", s.editor.Text)
	}
	completionSource(s, "SELECT u.na|me FROM users AS u")
	s.code.completion.trigger(false)
	waitUI(t, w)
	if !view.visible() || view.items[0].Insert != "name" || view.items[0].Kind != "字段" {
		t.Fatal("alias columns missing", view.items)
	}
	s.code.TypedKey(&fyne.KeyEvent{Name: fyne.KeyReturn})
	if s.editor.Text != "SELECT u.name FROM users AS u" || s.editor.CursorTextOffset() != len("SELECT u.name") {
		t.Fatal("completion damaged suffix or caret position", s.editor.Text, s.editor.CursorTextOffset())
	}
	completionSource(s, "SELECT u.ni| FROM users u")
	s.code.TypedShortcut(&desktop.CustomShortcut{KeyName: fyne.KeySpace, Modifier: fyne.KeyModifierControl})
	waitUI(t, w)
	if !view.visible() || view.items[0].Insert != "nickname" || client.objects.Load() != 1 || client.columns.Load() != 1 {
		t.Fatal("manual completion missed cache or reloaded metadata", client.objects.Load(), client.columns.Load(), view.items)
	}
	test.Tap(view.rows[0])
	if s.editor.Text != "SELECT u.nickname FROM users u" || w.Window.Canvas().Focused() != s.code {
		t.Fatal("mouse acceptance lost edit context", s.editor.Text)
	}
}

func TestSQLCompletionDismissesOnEscapeSelectionAndTabSwitch(t *testing.T) {
	w, s := completionWorkspace(t, &completionClient{})
	s.code.TypedRune('s')
	s.code.TypedKey(&fyne.KeyEvent{Name: fyne.KeyEscape})
	waitUI(t, w)
	if s.completion.view.visible() || s.completion.view.requested {
		t.Fatal("metadata callback reopened dismissed completion")
	}
	s.code.TypedRune('e')
	s.code.TypedShortcut(&fyne.ShortcutSelectAll{})
	s.code.completion.trigger(true)
	if s.completion.view.visible() {
		t.Fatal("completion appeared over selected SQL")
	}
	completionSource(s, "se|")
	s.code.completion.trigger(false)
	w.openWorkspace(s.profile, domain.Draft{ID: "second", Text: "SELECT 1", Scope: "main"})
	if s.completion.view.visible() {
		t.Fatal("completion outlived its active tab")
	}
	w.tabs.Select(s.item)
	w.Window.Canvas().Focus(s.code)
	s.code.completion.trigger(false)
	w.switcher.selectMode(2)
	if s.completion.view.visible() {
		t.Fatal("completion survived switching to Note")
	}
}

func TestSQLCompletionCancelsMetadataWhenScopeChangesOrTabCloses(t *testing.T) {
	client := &completionClient{block: make(chan struct{}), started: make(chan struct{}, 1)}
	w, s := completionWorkspace(t, client)
	completionSource(s, "SELECT * FROM us|")
	s.code.completion.trigger(false)
	if !s.completion.pending {
		t.Fatal("metadata was not scheduled")
	}
	select {
	case <-client.started:
	case <-time.After(3 * time.Second):
		t.Fatal("background metadata request did not start")
	}
	s.scope.SetText("another")
	waitUI(t, w)
	if s.completion.pending || s.completion.loaded || s.completion.view.visible() {
		t.Fatal("old scope metadata survived a database change")
	}
	completionSource(s, "SELECT * FROM us|")
	s.code.completion.trigger(false)
	w.closeTab(s.item)
	waitUI(t, w)
	if !s.closed || s.completion.pending || s.completion.view.visible() {
		t.Fatal("closed tab retained completion work or popup")
	}
}

func TestSQLCompletionDoesNotDismissAnUnrelatedPopup(t *testing.T) {
	w, s := completionWorkspace(t, &completionClient{})
	s.code.TypedRune('s')
	field := widget.NewEntry()
	other := widget.NewPopUp(field, w.Window.Canvas())
	other.Show()
	w.Window.Canvas().Focus(field)
	overlay := w.Window.Canvas().Overlays().Top()
	waitUI(t, w)
	if !other.Visible() || w.Window.Canvas().Overlays().Top() != overlay || w.Window.Canvas().Focused() != field {
		t.Fatal("completion metadata callback dismissed or unfocused another popup")
	}
	if s.completion.view.visible() || len(w.Window.Canvas().Overlays().List()) != 1 {
		t.Fatal("completion overlay survived underneath another popup")
	}
	other.Hide()
}
