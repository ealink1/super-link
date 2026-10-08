package release

import (
	"context"
	"io"
	"net/http"
	"os"
	"strings"
	"testing"
)

func TestDownloadProgressAndCancellation(t *testing.T) {
	manifest, _, _, _, _ := signedFixture(t)
	artifact := manifest.Artifacts[0]
	{
		client := New("")
		client.HTTP.Transport = transportFunc(func(r *http.Request) (*http.Response, error) {
			return &http.Response{StatusCode: 200, ContentLength: -1, Body: io.NopCloser(strings.NewReader("package"))}, nil
		})
		ctx, cancel := context.WithCancel(context.Background())
		root := t.TempDir()
		var counts []int64
		_, err := client.DownloadWithProgress(ctx, artifact, root, func(downloaded, total int64) {
			if total != artifact.Size {
				t.Errorf("total = %d", total)
			}
			if len(counts) > 0 && downloaded < counts[len(counts)-1] {
				t.Error("progress went backwards")
			}
			counts = append(counts, downloaded)
		})
		cancel()
		if err != nil {
			t.Fatal(err)
		}
		if len(counts) < 2 || counts[0] != 0 || counts[len(counts)-1] != artifact.Size {
			t.Fatalf("progress = %v", counts)
		}
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	root := t.TempDir()
	if _, err := New("").DownloadWithProgress(ctx, artifact, root, func(int64, int64) {}); err == nil {
		t.Fatal("ignored cancellation")
	}
	entries, err := os.ReadDir(root)
	if err != nil || len(entries) != 0 {
		t.Fatal("partial download remains", err)
	}
}
