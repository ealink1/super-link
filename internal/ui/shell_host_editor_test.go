package ui

import (
	"context"
	"strings"
	"testing"

	"fyne.io/fyne/v2/test"
	"github.com/ealink1/super-link/internal/domain"
)

func TestShellHostEditorTestsConnectionWithoutSaving(t *testing.T) {
	w := shellTestWindow(t)
	w.switcher.selectMode(1)
	waitUI(t, w)
	s := w.switcher.shell
	e := newShellHostEditor(s, domain.ShellHost{Name: "probe fixture", Host: "127.0.0.1", Port: 1, User: "root"})
	e.modal.popup.Show()
	waitUI(t, w)
	driver := w.App.Driver()
	left, right := driver.AbsolutePositionForObject(e.test), driver.AbsolutePositionForObject(e.save)
	if left.X >= right.X || left.Y+4 < right.Y || right.Y+4 < left.Y {
		t.Fatalf("测试连接 must sit alone on the footer's left: test=%v save=%v", left, right)
	}
	test.Tap(e.test)
	waitUI(t, w)
	if !strings.Contains(e.hint.Text, "连接失败") {
		t.Fatalf("the refused endpoint was not reported: %q", e.hint.Text)
	}
	hosts, err := s.service.List(context.Background())
	if err != nil || len(hosts) != 0 {
		t.Fatalf("testing a connection wrote to the host list: %v %v", hosts, err)
	}
	// A button still disabled by the finished probe would ignore this tap.
	e.port.SetText("not-a-port")
	test.Tap(e.test)
	if hint := e.hint.Text; hint != "端口必须是数字" {
		t.Fatalf("the second tap did not re-validate the form: %q", hint)
	}
	e.modal.hide()
	waitUI(t, w)
}
