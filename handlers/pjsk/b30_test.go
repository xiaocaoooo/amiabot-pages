package pjsk

import (
	"encoding/json"
	htmltemplate "html/template"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/gin-gonic/gin"
)

func TestB30HandlerInvalidIDDoesNotLeakInput(t *testing.T) {
	tpl := htmltemplate.Must(htmltemplate.New("b30-test").Parse(`{{define "pjsk/b30"}}{{if .Error}}{{.Error}}{{else}}{{.B30.Name}}|{{.B30.Server}}{{end}}{{end}}`))
	r := gin.New()
	r.SetHTMLTemplate(tpl)
	r.GET("/pjsk/b30", B30Handler)

	w := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/pjsk/b30?id=abc&server=jp", nil)
	r.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("unexpected status: got=%d body=%s", w.Code, w.Body.String())
	}

	body := w.Body.String()
	if !strings.Contains(body, "无效的玩家 ID") {
		t.Fatalf("unexpected body: %s", body)
	}
	if strings.Contains(body, "abc") {
		t.Fatalf("response body leaked invalid id: %s", body)
	}
}

func TestFetchSuiteMusicResultsFallbackNameDoesNotLeakUserID(t *testing.T) {
	oldClient := pjskB30HTTPClient
	defer func() {
		pjskB30HTTPClient = oldClient
	}()

	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/public/jp/suite/123456" {
			t.Fatalf("unexpected upstream path: %s", r.URL.Path)
		}
		if err := json.NewEncoder(w).Encode(map[string]any{
			"userMusicResults": []map[string]any{{
				"musicId":             1,
				"musicDifficultyType": "master",
				"playResult":          "clear",
				"fullComboFlg":        false,
				"fullPerfectFlg":      false,
			}},
			"upload_time": 0,
			"userProfile": map[string]any{
				"userId": "123456",
				"name":   "",
			},
		}); err != nil {
			t.Fatalf("encode response failed: %v", err)
		}
	}))
	defer server.Close()

	pjskB30HTTPClient = server.Client()
	t.Setenv("PJSK_PROFILE_BASEURL", "")
	t.Setenv("PJSK_PROFILE_HEADERS", "")

	results, name, uploadTime, err := fetchSuiteMusicResults(server.URL, "jp", "123456")
	if err != nil {
		t.Fatalf("fetchSuiteMusicResults() error = %v", err)
	}
	if len(results) != 1 {
		t.Fatalf("unexpected results count: %d", len(results))
	}
	if uploadTime != "" {
		t.Fatalf("unexpected uploadTime: %q", uploadTime)
	}
	if name != "玩家" {
		t.Fatalf("unexpected fallback name: %q", name)
	}
	if strings.Contains(name, "123456") {
		t.Fatalf("fallback name leaked user id: %q", name)
	}
}
