package gallery

import (
	htmltemplate "html/template"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/gin-gonic/gin"
)

func init() {
	gin.SetMode(gin.TestMode)
}

func TestDuplicateHandlerMissingParams(t *testing.T) {
	r := newGalleryTestRouter(t)
	w := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/gallery/duplicate", nil)
	r.ServeHTTP(w, req)
	if w.Code != http.StatusOK {
		t.Fatalf("unexpected status: %d", w.Code)
	}
	if !strings.Contains(w.Body.String(), "缺少必要参数，无法生成重复图片对比页") {
		t.Fatalf("unexpected body: %s", w.Body.String())
	}
}

func TestDuplicateHandlerRenderSuccess(t *testing.T) {
	t.Setenv("GALLERY_PAGES_RENDER_BASE", "http://gallery-server:8080")
	oldDownloader := galleryImageDownloader
	galleryImageDownloader = func(imageURL string, ttl time.Duration, headers map[string]string) htmltemplate.URL {
		return htmltemplate.URL("data:image/png;base64,ZmFrZQ==")
	}
	defer func() { galleryImageDownloader = oldDownloader }()

	r := newGalleryTestRouter(t)
	w := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/gallery/duplicate?current_image_url=http://example.com/current.png&duplicate_id=123&current_tags=cat,cover&existing_tags=cat", nil)
	r.ServeHTTP(w, req)
	if w.Code != http.StatusOK {
		t.Fatalf("unexpected status: %d body=%s", w.Code, w.Body.String())
	}
	body := w.Body.String()
	for _, want := range []string{"检测到重复上传", "这张图片已存在于图库中，已收录图片 ID：#123", "本次上传图片", "图库中已收录的图片", "cat", "cover", "data:image/png;base64,ZmFrZQ=="} {
		if !strings.Contains(body, want) {
			t.Fatalf("body missing %q: %s", want, body)
		}
	}
}

func TestDuplicateHandlerBuildsExistingURLFromEnv(t *testing.T) {
	t.Setenv("GALLERY_PAGES_RENDER_BASE", "http://gallery-server:8080")
	t.Setenv("GALLERY_READ_TOKEN", "read-token")
	oldDownloader := galleryImageDownloader
	var downloaded []string
	galleryImageDownloader = func(imageURL string, ttl time.Duration, headers map[string]string) htmltemplate.URL {
		downloaded = append(downloaded, imageURL)
		return htmltemplate.URL("data:image/png;base64,ZmFrZQ==")
	}
	defer func() { galleryImageDownloader = oldDownloader }()

	r := newGalleryTestRouter(t)
	w := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/gallery/duplicate?current_image_url=http://example.com/current.png&duplicate_id=123", nil)
	r.ServeHTTP(w, req)
	if w.Code != http.StatusOK {
		t.Fatalf("unexpected status: %d body=%s", w.Code, w.Body.String())
	}
	if len(downloaded) != 2 {
		t.Fatalf("expected 2 downloads, got %d (%v)", len(downloaded), downloaded)
	}
	if downloaded[1] != "http://gallery-server:8080/v1/images/123/render" {
		t.Fatalf("unexpected built existing image url: %q", downloaded[1])
	}
}

func newGalleryTestRouter(t *testing.T) *gin.Engine {
	t.Helper()
	r := gin.New()
	r.LoadHTMLFiles(
		"../../templates/layout.html",
		"../../templates/logo.html",
		"../../templates/gallery/duplicate.html",
		"../../templates/gallery/tags.html",
		"../../templates/gallery/images.html",
	)
	r.GET("/gallery/duplicate", DuplicateHandler)
	r.GET("/gallery/tags", TagsHandler)
	r.GET("/gallery/images", ImagesHandler)
	return r
}
