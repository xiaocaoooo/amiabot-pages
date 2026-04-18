package gallery

import (
	htmltemplate "html/template"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"
	"time"
)

func TestImagesHandlerMissingTags(t *testing.T) {
	r := newGalleryTestRouter(t)
	w := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/gallery/images", nil)
	r.ServeHTTP(w, req)
	if w.Code != http.StatusOK {
		t.Fatalf("unexpected status: %d", w.Code)
	}
	if !strings.Contains(w.Body.String(), "缺少标签参数，无法生成图片列表页") {
		t.Fatalf("unexpected body: %s", w.Body.String())
	}
}

func TestImagesHandlerRenderSuccessWithPagination(t *testing.T) {
	oldDownloader := galleryImageDownloader
	oldPageSize := galleryImageListPageSize
	galleryImageListPageSize = 2
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
		galleryImageListPageSize = oldPageSize
	}()

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/v1/images" {
			t.Fatalf("unexpected path: %s", r.URL.Path)
		}
		if got := r.Header.Get("Authorization"); got != "Bearer read-token" {
			t.Fatalf("unexpected auth header: %q", got)
		}
		if r.URL.Query().Get("tags") != "Cat" {
			t.Fatalf("unexpected tags query: %s", r.URL.RawQuery)
		}
		page := r.URL.Query().Get("page")
		pageSize := r.URL.Query().Get("page_size")
		if pageSize != "2" {
			t.Fatalf("unexpected page_size: %q", pageSize)
		}
		w.Header().Set("Content-Type", "application/json")
		switch page {
		case "1":
			_, _ = w.Write(mustGalleryJSON(t, map[string]any{
				"items": []map[string]any{
					{
						"id": 1, "filename": "a.webp", "fid": "fid-1", "file_size": 10,
						"width": 1000, "height": 800, "mime_type": "image/webp", "phash": 1,
						"is_animated": false, "description": "", "created_at": "2025-01-01T00:00:00Z",
						"tags": []map[string]any{{"id": 1, "name": "Cat", "created_at": "2025-01-01T00:00:00Z"}},
					},
					{
						"id": 2, "filename": "b.webp", "fid": "fid-2", "file_size": 10,
						"width": 900, "height": 900, "mime_type": "image/webp", "phash": 2,
						"is_animated": false, "description": "", "created_at": "2025-01-02T00:00:00Z",
						"tags": []map[string]any{{"id": 1, "name": "Cat", "created_at": "2025-01-01T00:00:00Z"}, {"id": 2, "name": "Cover", "created_at": "2025-01-01T00:00:00Z"}},
					},
				},
				"page": 1, "page_size": 2, "total": 3,
			}))
		case "2":
			_, _ = w.Write(mustGalleryJSON(t, map[string]any{
				"items": []map[string]any{
					{
						"id": 3, "filename": "c.webp", "fid": "fid-3", "file_size": 10,
						"width": 640, "height": 480, "mime_type": "image/webp", "phash": 3,
						"is_animated": false, "description": "", "created_at": "2025-01-03T00:00:00Z",
						"tags": []map[string]any{{"id": 1, "name": "Cat", "created_at": "2025-01-01T00:00:00Z"}},
					},
				},
				"page": 2, "page_size": 2, "total": 3,
			}))
		default:
			t.Fatalf("unexpected page query: %q", page)
		}
	}))
	defer server.Close()

	t.Setenv("GALLERY_SERVER", server.URL)
	t.Setenv("GALLERY_READ_TOKEN", "read-token")

	r := newGalleryTestRouter(t)
	w := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/gallery/images?tags=Cat", nil)
	r.ServeHTTP(w, req)
	if w.Code != http.StatusOK {
		t.Fatalf("unexpected status: %d body=%s", w.Code, w.Body.String())
	}
	body := w.Body.String()
	for _, want := range []string{"标签图片列表：#Cat", "共 3 张图片", "图片 #1", "图片 #2", "图片 #3", "1000×800", "900×900", "640×480", "#Cover", "data:image/png;base64,ZmFrZQ=="} {
		if !strings.Contains(body, want) {
			t.Fatalf("body missing %q: %s", want, body)
		}
	}
	mu.Lock()
	defer mu.Unlock()
	if len(downloaded) != 3 {
		t.Fatalf("expected 3 downloaded previews, got %d (%v)", len(downloaded), downloaded)
	}
	joined := strings.Join(downloaded, "\n")
	for _, want := range []string{"/v1/images/1/render", "/v1/images/2/render", "/v1/images/3/render"} {
		if !strings.Contains(joined, want) {
			t.Fatalf("downloaded previews missing %q: %v", want, downloaded)
		}
	}
}
