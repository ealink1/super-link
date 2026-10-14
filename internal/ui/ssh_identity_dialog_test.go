package ui

import (
	"context"
	"errors"
	"image/png"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/test"
	"github.com/ealink1/super-link/internal/domain"
)

const identityFixtureFingerprint = "SHA256:gpsrq6Cvr10sl/QYuKjKWMjKBGLsP3We7lMYpgGPeM"

type identityClipboardWindow struct {
	fyne.Window
	clipboard fyne.Clipboard
}

func (w identityClipboardWindow) Clipboard() fyne.Clipboard { return w.clipboard }

func TestSSHIdentityDialogCopiesOriginalAndCompletesOnce(t *testing.T) {
	w := shellTestWindow(t)
	w.Window = identityClipboardWindow{w.Window, w.App.Clipboard()}
	w.Window.Resize(fyne.NewSize(1000, 760))
	count := 0
	accepted, remember := false, true
	d := newSSHIdentityDialog(w.Window, domain.ShellHost{User: "fixture", Host: "localhost", Port: 2222}, identityFixtureFingerprint, func(ok, save bool) { count++; accepted, remember = ok, save })
	d.show()
	test.Tap(d.copy)
	if w.Window.Clipboard().Content() != identityFixtureFingerprint || d.copy.Text != "已复制" {
		t.Fatal("copy changed the actual fingerprint")
	}
	d.remember.SetChecked(false)
	d.remember.TypedKey(&fyne.KeyEvent{Name: fyne.KeyReturn})
	d.confirm.OnTapped()
	if count != 1 || !accepted || remember || d.popup.Visible() {
		t.Fatal("confirmation ignored one-time choice or ran twice")
	}
	d = newSSHIdentityDialog(w.Window, domain.ShellHost{}, identityFixtureFingerprint, func(ok, _ bool) {
		if ok {
			t.Fatal("Escape accepted host")
		}
		count++
	})
	d.show()
	d.copy.TypedKey(&fyne.KeyEvent{Name: fyne.KeyEscape})
	d.cancel.OnTapped()
	if count != 2 || d.popup.Visible() {
		t.Fatal("Escape did not cancel exactly once")
	}
}

func TestSSHOneTimeApprovalRejectsChangedHostAndDoesNotPersist(t *testing.T) {
	w := shellTestWindow(t)
	w.switcher.selectMode(1)
	waitUI(t, w)
	service := w.switcher.shell.service
	h, err := service.Save(context.Background(), domain.ShellHost{Name: "fixture", User: "test", Host: "localhost", Port: 22})
	if err != nil {
		t.Fatal(err)
	}
	approval := &shellHostApproval{host: h, fingerprint: identityFixtureFingerprint}
	pinned, err := applyShellHostApproval(h, approval)
	if err != nil || pinned.Fingerprint != identityFixtureFingerprint {
		t.Fatal("approved connection is not pinned")
	}
	stored, err := service.Get(context.Background(), h.ID)
	if err != nil || stored.Fingerprint != "" {
		t.Fatal("one-time trust persisted")
	}
	for _, change := range []func(*domain.ShellHost){
		func(h *domain.ShellHost) { h.ID = "different" }, func(h *domain.ShellHost) { h.Revision++ },
		func(h *domain.ShellHost) { h.Host = "other" }, func(h *domain.ShellHost) { h.Port++ },
		func(h *domain.ShellHost) { h.User = "other" }, func(h *domain.ShellHost) { h.Fingerprint = "SHA256:other" },
	} {
		current := h
		change(&current)
		if _, err := applyShellHostApproval(current, approval); !errors.Is(err, domain.ErrConflict) {
			t.Fatal("approval accepted a changed identity")
		}
	}
	if err := service.Trust(context.Background(), h.ID, h.Revision, identityFixtureFingerprint); err != nil {
		t.Fatal(err)
	}
	stored, err = service.Get(context.Background(), h.ID)
	if err != nil || stored.Fingerprint != identityFixtureFingerprint {
		t.Fatal("remembered approval was not persisted")
	}
}

func TestSSHIdentityDialogGeometryAndThemes(t *testing.T) {
	w := shellTestWindow(t)
	for _, dark := range []bool{false, true} {
		w.App.Settings().SetTheme(Theme{Dark: dark})
		for _, size := range []fyne.Size{fyne.NewSize(1000, 760), fyne.NewSize(420, 480)} {
			w.Window.Resize(size)
			d := newSSHIdentityDialog(w.Window, domain.ShellHost{User: "root", Host: "example.invalid", Port: 2222}, identityFixtureFingerprint, nil)
			d.show()
			_ = w.Window.Canvas().Capture()
			if d.popup.Size().Width > size.Width || d.popup.Size().Height > size.Height {
				t.Fatal("dialog exceeds viewport")
			}
			if d.confirm.Size().Width < d.confirm.MinSize().Width || d.cancel.Size().Width < d.cancel.MinSize().Width {
				t.Fatal("actions clipped")
			}
			text := shellDialogText(d.popup.Content)
			compact := strings.ReplaceAll(text, " ", "")
			if !strings.Contains(compact, identityFixtureFingerprint) || !strings.Contains(text, "root@example.invalid:2222") || !strings.Contains(text, "SuperLink 本地主机记录") {
				t.Fatal("dialog lost actual identity or truthful persistence hint")
			}
			if directory := os.Getenv("SUPERLINK_IDENTITY_CAPTURE"); directory != "" && size.Width == 1000 {
				if err := os.MkdirAll(directory, 0700); err != nil {
					t.Fatal(err)
				}
				name := "light.png"
				if dark {
					name = "dark.png"
				}
				file, err := os.Create(filepath.Join(directory, name))
				if err != nil {
					t.Fatal(err)
				}
				err = png.Encode(file, w.Window.Canvas().Capture())
				file.Close()
				if err != nil {
					t.Fatal(err)
				}
			}
			d.finish(false)
		}
	}
}
