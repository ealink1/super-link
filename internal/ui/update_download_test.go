package ui

import (
	"net/http"
	"strings"
	"testing"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/widget"
	"github.com/ealink1/super-link/internal/infra/release"
)

func TestUpdateDownloadShowsProgressAndCancels(t *testing.T) {
	w := shellTestWindow(t)
	w.Releases.HTTP.Transport = updateTestTransport(func(request *http.Request) (*http.Response, error) {
		<-request.Context().Done()
		return nil, request.Context().Err()
	})
	artifact := release.Artifact{Size: 45 << 20, Filename: "SuperLink.zip", URL: "https://github.com/ealink1/super-link/releases/download/v0.2.0/SuperLink.zip"}
	w.downloadUpdate(artifact, "0.2.0")
	var cancelButton *widget.Button
	var text strings.Builder
	for _, overlay := range w.Window.Canvas().Overlays().List() {
		text.WriteString(shellDialogText(overlay))
		walkUpdateDialog(overlay, func(object fyne.CanvasObject) {
			if button, ok := object.(*widget.Button); ok && button.Text == "取消下载" {
				cancelButton = button
			}
		})
	}
	if !strings.Contains(text.String(), "45.0 MiB") {
		t.Fatal("missing download size", text.String())
	}
	if cancelButton == nil {
		t.Fatal("missing cancel button")
	}
	test.Tap(cancelButton)
	waitUI(t, w)
	if w.status.Text != "更新下载已取消" {
		t.Fatal(w.status.Text)
	}
}

func walkUpdateDialog(object fyne.CanvasObject, visit func(fyne.CanvasObject)) {
	visit(object)
	var children []fyne.CanvasObject
	switch item := object.(type) {
	case *fyne.Container:
		children = item.Objects
	case fyne.Widget:
		children = item.CreateRenderer().Objects()
	}
	for _, child := range children {
		walkUpdateDialog(child, visit)
	}
}
