package gallery

import (
	"encoding/json"
	htmltemplate "html/template"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"
	"time"
)

func TestTagsHandlerMissingGalleryServer(t *testing.T) {
	t.Setenv("GALLERY_SERVER", "")
	r := newGalleryTestRouter(t)
	w := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/gallery/tags", nil)
	r.ServeHTTP(w, req)
	if w.Code != http.StatusOK {
		t.Fatalf("unexpected status: %d", w.Code)
	}
	if !strings.Contains(w.Body.String(), "未配置 GALLERY_SERVER") {
		t.Fatalf("unexpected body: %s", w.Body.String())
	}
}

func TestTagsHandlerRenderSuccess(t *testing.T) {
	oldDownloader := galleryImageDownloader
	oldWorkers := galleryTagPreviewWorkers
	galleryTagPreviewWorkers = 1
	var (
		mu         sync.Mutex
		downloaded []string
	)
	galleryImageDownloader = func(imageURL string, ttl time.Duration, headers map[string]string) htmltemplate.URL {
		mu.Lock()
		defer mu.Unlock()
		downloaded = append(downloaded, imageURL)
		return htmltemplate.URL("data:image/png;base64,ZmFrZQ==")
	}
	defer func() {
		galleryImageDownloader = oldDownloader
		galleryTagPreviewWorkers = oldWorkers
	}()

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/v1/tags":
			if got := r.Header.Get("Authorization"); got != "Bearer read-token" {
				t.Fatalf("unexpected auth header: %q", got)
			}
			w.Header().Set("Content-Type", "application/json")
			_, _ = w.Write(mustGalleryJSON(t, map[string]any{
				"items": []map[string]any{
					{"id": 1, "name": "Cat", "created_at": "2025-01-01T00:00:00Z"},
					{"id": 2, "name": "Cover", "created_at": "2025-01-01T00:00:00Z"},
					{"id": 3, "name": "Empty", "created_at": "2025-01-01T00:00:00Z"},
				},
			}))
		case "/v1/images":
			if got := r.Header.Get("Authorization"); got != "Bearer read-token" {
				t.Fatalf("unexpected auth header: %q", got)
			}
			if r.URL.Query().Get("page") != "1" || r.URL.Query().Get("page_size") != "1" {
				t.Fatalf("unexpected paging query: %s", r.URL.RawQuery)
			}
			tag := r.URL.Query().Get("tags")
			w.Header().Set("Content-Type", "application/json")
			switch tag {
			case "Cat":
				_, _ = w.Write(mustGalleryJSON(t, map[string]any{
					"items": []map[string]any{{
						"id": 101, "filename": "cat.webp", "fid": "fid-101", "file_size": 10,
						"width": 800, "height": 600, "mime_type": "image/webp", "phash": 1,
						"is_animated": false, "description": "", "created_at": "2025-01-01T00:00:00Z",
						"tags": []map[string]any{{"id": 1, "name": "Cat", "created_at": "2025-01-01T00:00:00Z"}},
					}},
					"page": 1, "page_size": 1, "total": 2,
				}))
			case "Cover":
				_, _ = w.Write(mustGalleryJSON(t, map[string]any{
					"items": []map[string]any{{
						"id": 202, "filename": "cover.webp", "fid": "fid-202", "file_size": 10,
						"width": 640, "height": 640, "mime_type": "image/webp", "phash": 2,
						"is_animated": false, "description": "", "created_at": "2025-01-01T00:00:00Z",
						"tags": []map[string]any{{"id": 2, "name": "Cover", "created_at": "2025-01-01T00:00:00Z"}},
					}},
					"page": 1, "page_size": 1, "total": 1,
				}))
			case "Empty":
				_, _ = w.Write(mustGalleryJSON(t, map[string]any{
					"items": []map[string]any{},
					"page":  1, "page_size": 1, "total": 0,
				}))
			default:
				t.Fatalf("unexpected tags query: %q", tag)
			}
		default:
			t.Fatalf("unexpected path: %s", r.URL.Path)
		}
	}))
	defer server.Close()

	t.Setenv("GALLERY_SERVER", server.URL)
	t.Setenv("GALLERY_READ_TOKEN", "read-token")

	r := newGalleryTestRouter(t)
	w := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/gallery/tags", nil)
	r.ServeHTTP(w, req)
	if w.Code != http.StatusOK {
		t.Fatalf("unexpected status: %d body=%s", w.Code, w.Body.String())
	}
	body := w.Body.String()
	for _, want := range []string{"图库标签总览", "共 3 个标签", "#Cat", "2 张图片", "首图 #101", "#Cover", "首图 #202", "#Empty", "暂无图片", "data:image/png;base64,ZmFrZQ=="} {
		if !strings.Contains(body, want) {
			t.Fatalf("body missing %q: %s", want, body)
		}
	}
	mu.Lock()
	defer mu.Unlock()
	if len(downloaded) != 2 {
		t.Fatalf("expected 2 downloaded previews, got %d (%v)", len(downloaded), downloaded)
	}
	joined := strings.Join(downloaded, "\n")
	for _, want := range []string{"/v1/images/101/render", "/v1/images/202/render"} {
		if !strings.Contains(joined, want) {
			t.Fatalf("downloaded previews missing %q: %v", want, downloaded)
		}
	}
}

func mustGalleryJSON(t *testing.T, value any) []byte {
	t.Helper()
	data, err := json.Marshal(value)
	if err != nil {
		t.Fatalf("json.Marshal() error = %v", err)
	}
	return data
}
