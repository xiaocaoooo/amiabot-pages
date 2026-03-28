package pixiv

import (
	"encoding/json"
	"html/template"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"

	"github.com/gin-gonic/gin"
)

func init() {
	gin.SetMode(gin.TestMode)
}

func TestNormalizePixivCaption(t *testing.T) {
	raw := `<p>Hello<br />World</p><ul><li>Tag 1</li><li>Tag 2</li></ul>&amp; done`
	got := normalizePixivCaption(raw)
	want := "Hello\nWorld\n• Tag 1\n• Tag 2\n& done"
	if got != want {
		t.Fatalf("unexpected caption:\ngot:  %q\nwant: %q", got, want)
	}
}

func TestIllustInfoHandlerMissingPID(t *testing.T) {
	clearPixivAccessTokenCache()
	t.Setenv("PIXIV_ACCESS_TOKEN", "")
	t.Setenv("PIXIV_REFRESH_TOKEN", "")

	r := newPixivTestRouter(t)
	w := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/pixiv/illust/info", nil)
	r.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("unexpected status: got=%d body=%s", w.Code, w.Body.String())
	}
	if !strings.Contains(w.Body.String(), "缺少插画 PID 参数") {
		t.Fatalf("unexpected body: %s", w.Body.String())
	}
}

func TestIllustInfoHandlerInvalidPID(t *testing.T) {
	clearPixivAccessTokenCache()
	t.Setenv("PIXIV_ACCESS_TOKEN", "")
	t.Setenv("PIXIV_REFRESH_TOKEN", "")

	r := newPixivTestRouter(t)
	w := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/pixiv/illust/info?pid=abc", nil)
	r.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("unexpected status: got=%d body=%s", w.Code, w.Body.String())
	}
	if !strings.Contains(w.Body.String(), "无效的插画 PID: abc") {
		t.Fatalf("unexpected body: %s", w.Body.String())
	}
}

func TestIllustInfoHandlerMissingCredentials(t *testing.T) {
	clearPixivAccessTokenCache()
	t.Setenv("PIXIV_ACCESS_TOKEN", "")
	t.Setenv("PIXIV_REFRESH_TOKEN", "")

	r := newPixivTestRouter(t)
	w := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/pixiv/illust/info?pid=1", nil)
	r.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("unexpected status: got=%d body=%s", w.Code, w.Body.String())
	}
	if !strings.Contains(w.Body.String(), "缺少 Pixiv token") {
		t.Fatalf("unexpected body: %s", w.Body.String())
	}
}

func TestLoadPixivTagBlacklist(t *testing.T) {
	t.Setenv("PIXIV_TAG_BLACKLIST", " R-18, 原创，AI生成 ; translated-hidden\n保留 ")

	blacklist := loadPixivTagBlacklist()
	for _, want := range []string{"r-18", "原创", "ai生成", "translated-hidden", "保留"} {
		if _, ok := blacklist[want]; !ok {
			t.Fatalf("blacklist missing %q: %#v", want, blacklist)
		}
	}
}

func TestBuildPixivIllustPageDataFiltersBlacklistedTags(t *testing.T) {
	t.Setenv("PIXIV_TAG_BLACKLIST", "原创, TRANSLATED-HIDDEN")

	data := buildPixivIllustPageData(&pixivIllust{
		ID:    123,
		Title: "测试作品",
		User: pixivUser{
			Name:    "测试作者",
			Account: "tester",
		},
		Tags: []pixivTag{
			{Name: "原创", TranslatedName: "original"},
			{Name: "保留标签", TranslatedName: "keep"},
			{Name: "另一个标签", TranslatedName: "translated-hidden"},
		},
	})

	if len(data.Tags) != 1 {
		t.Fatalf("unexpected tag count: got=%d tags=%#v", len(data.Tags), data.Tags)
	}
	if got := data.Tags[0].Name; got != "保留标签" {
		t.Fatalf("unexpected remaining tag: got=%q", got)
	}
}

func TestIllustInfoHandlerRefreshesExpiredTokenAndRendersPage(t *testing.T) {
	clearPixivAccessTokenCache()
	defer clearPixivAccessTokenCache()

	t.Setenv("PIXIV_ACCESS_TOKEN", "stale_token")
	t.Setenv("PIXIV_REFRESH_TOKEN", "refresh_token")

	var detailCalls int32
	var refreshCalls int32
	var serverURL string

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/v1/illust/detail":
			atomic.AddInt32(&detailCalls, 1)
			switch r.Header.Get("Authorization") {
			case "Bearer stale_token":
				w.Header().Set("Content-Type", "application/json")
				w.WriteHeader(http.StatusBadRequest)
				_, _ = w.Write([]byte(`{"error":{"message":"OAuth process failed"}}`))
			case "Bearer fresh_token":
				w.Header().Set("Content-Type", "application/json")
				_ = json.NewEncoder(w).Encode(map[string]any{
					"illust": map[string]any{
						"id":          123,
						"title":       "测试作品",
						"type":        "illust",
						"image_urls":  map[string]any{"square_medium": serverURL + "/img/main.jpg", "medium": serverURL + "/img/main.jpg", "large": serverURL + "/img/main.jpg"},
						"caption":     "<p>第一行<br />第二行</p>",
						"create_date": "2024-12-26T01:27:47+09:00",
						"page_count":  2,
						"width":       1000,
						"height":      2000,
						"user": map[string]any{
							"id":      456,
							"name":    "测试作者",
							"account": "tester",
							"profile_image_urls": map[string]any{
								"medium": serverURL + "/img/avatar.jpg",
							},
						},
						"tags":             []map[string]any{{"name": "原创", "translated_name": "original"}},
						"meta_single_page": map[string]any{"original_image_url": serverURL + "/img/main.jpg"},
						"meta_pages": []map[string]any{
							{"image_urls": map[string]any{"square_medium": serverURL + "/img/p1.jpg", "medium": serverURL + "/img/p1.jpg", "large": serverURL + "/img/p1.jpg", "original": serverURL + "/img/p1.jpg"}},
							{"image_urls": map[string]any{"square_medium": serverURL + "/img/p2.jpg", "medium": serverURL + "/img/p2.jpg", "large": serverURL + "/img/p2.jpg", "original": serverURL + "/img/p2.jpg"}},
						},
						"total_view":      12000,
						"total_bookmarks": 3456,
						"total_comments":  78,
						"illust_ai_type":  2,
						"series":          map[string]any{"id": 999, "title": "系列标题"},
					},
				})
			default:
				w.WriteHeader(http.StatusUnauthorized)
			}
		case "/auth/token":
			atomic.AddInt32(&refreshCalls, 1)
			if err := r.ParseForm(); err != nil {
				t.Fatalf("parse form failed: %v", err)
			}
			if got := r.Form.Get("refresh_token"); got != "refresh_token" {
				t.Fatalf("unexpected refresh token: %q", got)
			}
			w.Header().Set("Content-Type", "application/json")
			_, _ = w.Write([]byte(`{"response":{"access_token":"fresh_token","expires_in":3600,"refresh_token":"refresh_token"}}`))
		case "/img/main.jpg", "/img/p1.jpg", "/img/p2.jpg", "/img/avatar.jpg":
			w.Header().Set("Content-Type", "image/jpeg")
			_, _ = w.Write([]byte{0xff, 0xd8, 0xff, 0xd9})
		default:
			w.WriteHeader(http.StatusNotFound)
		}
	}))
	defer server.Close()
	serverURL = server.URL

	oldClient := pixivHTTPClient
	oldAppURL := pixivAppAPIBaseURL
	oldOAuthURL := pixivOAuthBaseURL
	pixivHTTPClient = server.Client()
	pixivAppAPIBaseURL = server.URL
	pixivOAuthBaseURL = server.URL
	defer func() {
		pixivHTTPClient = oldClient
		pixivAppAPIBaseURL = oldAppURL
		pixivOAuthBaseURL = oldOAuthURL
	}()

	r := newPixivTestRouter(t)
	w := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/pixiv/illust/info?pid=123", nil)
	r.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("unexpected status: got=%d body=%s", w.Code, w.Body.String())
	}

	body := w.Body.String()
	for _, want := range []string{"测试作品", "123", "测试作者", "第一行", "第二行", "2", "AI=true"} {
		if !strings.Contains(body, want) {
			t.Fatalf("response body missing %q: %s", want, body)
		}
	}
	if got := atomic.LoadInt32(&detailCalls); got != 2 {
		t.Fatalf("unexpected detail call count: got=%d want=2", got)
	}
	if got := atomic.LoadInt32(&refreshCalls); got != 1 {
		t.Fatalf("unexpected refresh call count: got=%d want=1", got)
	}
}

func newPixivTestRouter(t *testing.T) *gin.Engine {
	t.Helper()

	tpl := template.Must(template.New("pixiv-test").Parse(`{{define "pixiv/illust"}}{{if .Error}}{{.Error}}{{else}}{{.Illust.Title}}|{{.Illust.ID}}|{{.Illust.AuthorName}}|{{.Illust.PageCount}}|{{.Illust.Caption}}|AI={{.Illust.IsAIGenerated}}{{end}}{{end}}`))
	r := gin.New()
	r.SetHTMLTemplate(tpl)
	r.GET("/pixiv/illust/info", IllustInfoHandler)
	return r
}
