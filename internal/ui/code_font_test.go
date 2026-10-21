package ui

import (
	"strings"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/theme"
	"github.com/ealink1/super-link/internal/domain"
)

func TestSQLTypingChineseKeepsLineBoxesStable(t *testing.T) {
	w, p := parityWindow(t)
	s := w.openWorkspace(p, domain.Draft{ID: "stable-lines", Scope: "main"})
	waitUI(t, w)
	var positions []fyne.Position
	var caret fyne.Position
	for i, source := range []string{
		"select * from ott_dealer\nselect 2",
		"select * from ott_dealer发\nselect 2",
		"select * from ott_dealer\nselect 2",
		"select 中文 from ott_dealer\nselect 2",
	} {
		s.editor.SetText(source)
		w.Window.Canvas().Capture()
		r := test.WidgetRenderer(s.code.layer).(*syntaxRenderer)
		var current []fyne.Position
		for _, piece := range r.pieces {
			if piece.text.Text == "select" {
				current = append(current, piece.text.Position())
			}
		}
		s.editor.CursorRow, s.editor.CursorColumn = 1, 6
		s.editor.Refresh()
		if i == 0 {
			positions, caret = current, s.editor.CursorPosition()
		} else if len(current) != 2 || current[0] != positions[0] || current[1] != positions[1] || s.editor.CursorPosition() != caret {
			t.Fatalf("CJK edit moved the line box: initial=%v/%v got=%v/%v", positions, caret, current, s.editor.CursorPosition())
		}
	}
}

func TestSQLMixedScriptUsesNativeCaretMetricsAndBaseline(t *testing.T) {
	w, p := parityWindow(t)
	s := w.openWorkspace(p, domain.Draft{ID: "mixed-font", Scope: "main"})
	waitUI(t, w)
	for _, source := range []string{
		"select * from ott_dealer 方法 房东税发送到",
		"select '中文 备注' as 中文字段, name -- 中文 comment",
		"select 中文\tfrom 方法",
		"方法, abc",
	} {
		s.editor.SetText(source)
		w.Window.Canvas().Capture()
		driver := w.App.Driver()
		_, baseline := driver.RenderedTextSize(source, queryEditorTextSize, s.editor.TextStyle, nil)
		r := test.WidgetRenderer(s.code.layer).(*syntaxRenderer)
		offset := 0
		for _, piece := range r.pieces {
			offset += strings.Index(source[offset:], piece.text.Text) + len(piece.text.Text)
			column := len([]rune(source[:offset]))
			s.editor.CursorRow, s.editor.CursorColumn = 0, column
			s.editor.Refresh()
			_, base := driver.RenderedTextSize(piece.text.Text, queryEditorTextSize, s.editor.TextStyle, nil)
			if piece.y+base < baseline+3.9 || piece.y+base > baseline+4.1 {
				t.Errorf("mixed-script baseline differs for %q: got %v, want %v", piece.text.Text, piece.y+base, baseline+4)
			}
			if end := piece.x + piece.text.MinSize().Width; end < s.editor.CursorPosition().X-.5 || end > s.editor.CursorPosition().X+.5 {
				t.Errorf("caret is displaced from visible SQL after %q in %q: ink=%v caret=%v", piece.text.Text, source, end, s.editor.CursorPosition().X)
			}
		}
	}
}

func TestSQLTextSizeFollowsEditorTheme(t *testing.T) {
	w, p := parityWindow(t)
	s := w.openWorkspace(p, domain.Draft{ID: "font-layout", Text: "select * from ott_dealer", Scope: "main"})
	waitUI(t, w)
	for _, dark := range []bool{false, true} {
		w.App.Settings().SetTheme(Theme{Dark: dark})
		for _, source := range []string{"select * from ott_dealer", "select id, '中文备注' from ott_dealer"} {
			s.editor.SetText(source)
			s.editor.Refresh()
			w.Window.Canvas().Capture()
			if got := s.code.Theme().Size(theme.SizeNameText); got != queryEditorTextSize {
				t.Fatalf("native editor text size = %v, want 18", got)
			}
			for _, object := range test.WidgetRenderer(s.code.layer).Objects() {
				if text, ok := object.(*canvas.Text); ok && text.TextSize != queryEditorTextSize {
					t.Fatalf("visible SQL text size = %v, want 18 (dark=%v)", text.TextSize, dark)
				}
			}
			s.editor.CursorRow, s.editor.CursorColumn = 0, 0
			s.editor.Refresh()
			start := s.editor.CursorPosition()
			s.editor.CursorColumn = 6
			s.editor.Refresh()
			want := fyne.MeasureText("select", queryEditorTextSize, s.editor.TextStyle).Width
			if got := s.editor.CursorPosition().X - start.X; got < want-1 || got > want+1 {
				t.Fatalf("caret and SQL glyph widths disagree: got %v, want %v", got, want)
			}
		}
	}
}
