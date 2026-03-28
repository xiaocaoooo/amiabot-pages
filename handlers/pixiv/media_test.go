package pixiv

import (
	"archive/zip"
	"bytes"
	"encoding/json"
	"image"
	"image/color"
	"image/gif"
	"image/png"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/gin-gonic/gin"
)

func TestBuildPixivMediaManifestSinglePage(t *testing.T) {
	manifest, err := buildPixivMediaManifest(&pixivIllust{
		ID:        123,
		Title:     "单图",
		Type:      "illust",
		PageCount: 1,
		MetaSinglePage: pixivMetaSinglePage{
			OriginalImageURL: "https://i.pximg.net/img-original/img/2024/01/01/00/00/00/123_p0.jpg",
		},
	})
	if err != nil {
		t.Fatalf("buildPixivMediaManifest() error = %v", err)
	}
	if len(manifest.Items) != 1 {
		t.Fatalf("unexpected item count: got=%d", len(manifest.Items))
	}
	if got := manifest.Items[0].Path; got != "/pixiv/image?url=https%3A%2F%2Fi.pximg.net%2Fimg-original%2Fimg%2F2024%2F01%2F01%2F00%2F00%2F00%2F123_p0.jpg" {
		t.Fatalf("unexpected path: %s", got)
	}
}

func TestBuildPixivMediaManifestMultiPage(t *testing.T) {
	manifest, err := buildPixivMediaManifest(&pixivIllust{
		ID:        456,
		Title:     "多图",
		Type:      "manga",
		PageCount: 2,
		MetaPages: []pixivMetaPage{
			{ImageURLs: pixivMetaPageImageURLs{Original: "https://i.pximg.net/img-original/img/a_p0.png"}},
			{ImageURLs: pixivMetaPageImageURLs{Original: "https://i.pximg.net/img-original/img/a_p1.png"}},
		},
	})
	if err != nil {
		t.Fatalf("buildPixivMediaManifest() error = %v", err)
	}
	if len(manifest.Items) != 2 {
		t.Fatalf("unexpected item count: got=%d", len(manifest.Items))
	}
	if manifest.Items[0].Index != 0 || manifest.Items[1].Index != 1 {
		t.Fatalf("unexpected item indexes: %#v", manifest.Items)
	}
}

func TestBuildPixivMediaManifestUgoira(t *testing.T) {
	manifest, err := buildPixivMediaManifest(&pixivIllust{
		ID:        789,
		Title:     "动图",
		Type:      "ugoira",
		PageCount: 1,
	})
	if err != nil {
		t.Fatalf("buildPixivMediaManifest() error = %v", err)
	}
	if len(manifest.Items) != 1 {
		t.Fatalf("unexpected item count: got=%d", len(manifest.Items))
	}
	if got := manifest.Items[0].Path; got != "/pixiv/ugoira/gif?pid=789" {
		t.Fatalf("unexpected ugoira path: %s", got)
	}
}

func TestPixivImageProxyHandlerRejectsNonPximg(t *testing.T) {
	r := gin.New()
	r.GET("/pixiv/image", PixivImageProxyHandler)

	w := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/pixiv/image?url=https://example.com/image.jpg", nil)
	r.ServeHTTP(w, req)

	if w.Code != http.StatusBadRequest {
		t.Fatalf("unexpected status: got=%d body=%s", w.Code, w.Body.String())
	}
	if !strings.Contains(w.Body.String(), "pximg.net") {
		t.Fatalf("unexpected body: %s", w.Body.String())
	}
}

func TestConvertPixivUgoiraToGIF(t *testing.T) {
	zipData := buildTestUgoiraZip(t)
	gifData, err := convertPixivUgoiraToGIF(zipData, []pixivUgoiraFrame{
		{File: "000000.png", Delay: 60},
		{File: "000001.png", Delay: 80},
	})
	if err != nil {
		t.Fatalf("convertPixivUgoiraToGIF() error = %v", err)
	}

	decoded, err := gif.DecodeAll(bytes.NewReader(gifData))
	if err != nil {
		t.Fatalf("gif.DecodeAll() error = %v", err)
	}
	if len(decoded.Image) != 2 {
		t.Fatalf("unexpected gif frame count: got=%d", len(decoded.Image))
	}
	if decoded.Delay[0] != 6 || decoded.Delay[1] != 8 {
		t.Fatalf("unexpected gif delays: %#v", decoded.Delay)
	}
}

func TestIllustMediaHandlerReturnsDownloadPaths(t *testing.T) {
	clearPixivAccessTokenCache()
	defer clearPixivAccessTokenCache()
	t.Setenv("PIXIV_ACCESS_TOKEN", "valid_token")
	t.Setenv("PIXIV_REFRESH_TOKEN", "")

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/v1/illust/detail":
			if got := r.Header.Get("Authorization"); got != "Bearer valid_token" {
				t.Fatalf("unexpected authorization header: %s", got)
			}
			w.Header().Set("Content-Type", "application/json")
			_ = json.NewEncoder(w).Encode(map[string]any{
				"illust": map[string]any{
					"id":               123,
					"title":            "测试作品",
					"type":             "manga",
					"image_urls":       map[string]any{"square_medium": "", "medium": "", "large": ""},
					"user":             map[string]any{"id": 1, "name": "作者", "account": "user", "profile_image_urls": map[string]any{"medium": ""}},
					"tags":             []any{},
					"caption":          "",
					"create_date":      "2024-01-01T00:00:00+09:00",
					"page_count":       2,
					"width":            100,
					"height":           100,
					"meta_single_page": map[string]any{},
					"meta_pages": []map[string]any{
						{"image_urls": map[string]any{"original": "https://i.pximg.net/img-original/img/1_p0.jpg"}},
						{"image_urls": map[string]any{"original": "https://i.pximg.net/img-original/img/1_p1.jpg"}},
					},
					"total_view":      1,
					"total_bookmarks": 1,
					"total_comments":  1,
					"illust_ai_type":  0,
				},
			})
		default:
			w.WriteHeader(http.StatusNotFound)
		}
	}))
	defer server.Close()

	oldClient := pixivHTTPClient
	oldAppURL := pixivAppAPIBaseURL
	pixivHTTPClient = server.Client()
	pixivAppAPIBaseURL = server.URL
	defer func() {
		pixivHTTPClient = oldClient
		pixivAppAPIBaseURL = oldAppURL
	}()

	r := gin.New()
	r.GET("/pixiv/illust/media", IllustMediaHandler)

	w := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/pixiv/illust/media?pid=123", nil)
	r.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("unexpected status: got=%d body=%s", w.Code, w.Body.String())
	}

	var payload pixivMediaManifestResponse
	if err := json.Unmarshal(w.Body.Bytes(), &payload); err != nil {
		t.Fatalf("unmarshal response failed: %v body=%s", err, w.Body.String())
	}
	if payload.PID != 123 || payload.PageCount != 2 || len(payload.Items) != 2 {
		t.Fatalf("unexpected payload: %#v", payload)
	}
	if payload.Items[0].Path == "" || !strings.Contains(payload.Items[0].Path, "/pixiv/image?url=") {
		t.Fatalf("unexpected item path: %#v", payload.Items)
	}
}

func buildTestUgoiraZip(t *testing.T) []byte {
	t.Helper()

	var buffer bytes.Buffer
	writer := zip.NewWriter(&buffer)
	files := []struct {
		name string
		fill color.Color
	}{
		{name: "000000.png", fill: color.RGBA{R: 255, A: 255}},
		{name: "000001.png", fill: color.RGBA{G: 255, A: 255}},
	}

	for _, file := range files {
		entry, err := writer.Create(file.name)
		if err != nil {
			t.Fatalf("writer.Create() error = %v", err)
		}
		img := image.NewRGBA(image.Rect(0, 0, 2, 2))
		for y := 0; y < 2; y++ {
			for x := 0; x < 2; x++ {
				img.Set(x, y, file.fill)
			}
		}
		if err := png.Encode(entry, img); err != nil {
			t.Fatalf("png.Encode() error = %v", err)
		}
	}
	if err := writer.Close(); err != nil {
		t.Fatalf("writer.Close() error = %v", err)
	}
	return buffer.Bytes()
}
