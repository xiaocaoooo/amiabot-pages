package paramid

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	miniredis "github.com/alicebob/miniredis/v2"
	"github.com/gin-gonic/gin"
	"github.com/redis/go-redis/v9"
)

func init() {
	gin.SetMode(gin.TestMode)
}

func TestMiddlewareNoParamIDPassThrough(t *testing.T) {
	mw := newMiniredisMiddleware(t, nil)
	r := newEchoRouter(mw.Handler())

	w := performRequest(r, "/echo?id=1001&server=jp")
	if w.Code != http.StatusOK {
		t.Fatalf("unexpected status: got=%d body=%s", w.Code, w.Body.String())
	}

	body := decodeEchoBody(t, w)
	assertQueryValues(t, body.Query, "id", []string{"1001"})
	assertQueryValues(t, body.Query, "server", []string{"jp"})
	if _, ok := body.Query[QueryParamName]; ok {
		t.Fatalf("param_id should not exist in query: %#v", body.Query)
	}
}

func TestMiddlewareInjectAndMergeWithURLPriority(t *testing.T) {
	payload := `{"server":"cn","id":"1001","tag":["a","b"],"enabled":true,"limit":12}`
	mw := newMiniredisMiddleware(t, map[string]string{
		keyForID(DefaultKeyTemplate, "abc"): payload,
	})
	r := newEchoRouter(mw.Handler())

	w := performRequest(r, "/echo?param_id=abc&id=999&extra=manual&server=jp")
	if w.Code != http.StatusOK {
		t.Fatalf("unexpected status: got=%d body=%s", w.Code, w.Body.String())
	}

	body := decodeEchoBody(t, w)
	assertQueryValues(t, body.Query, "id", []string{"999"})
	assertQueryValues(t, body.Query, "server", []string{"jp"})
	assertQueryValues(t, body.Query, "extra", []string{"manual"})
	assertQueryValues(t, body.Query, "tag", []string{"a", "b"})
	assertQueryValues(t, body.Query, "enabled", []string{"true"})
	assertQueryValues(t, body.Query, "limit", []string{"12"})
	if _, ok := body.Query[QueryParamName]; ok {
		t.Fatalf("param_id should be removed from merged query: %#v", body.Query)
	}
}

func TestMiddlewareParamIDEmptyReturns400(t *testing.T) {
	mw := newMiniredisMiddleware(t, nil)
	r := newEchoRouter(mw.Handler())

	w := performRequest(r, "/echo?param_id=")
	if w.Code != http.StatusBadRequest {
		t.Fatalf("unexpected status: got=%d body=%s", w.Code, w.Body.String())
	}

	assertErrorContains(t, w, "param_id 不能为空")
}

func TestMiddlewareParamIDMissReturns400(t *testing.T) {
	mw := newMiniredisMiddleware(t, nil)
	r := newEchoRouter(mw.Handler())

	w := performRequest(r, "/echo?param_id=missing")
	if w.Code != http.StatusBadRequest {
		t.Fatalf("unexpected status: got=%d body=%s", w.Code, w.Body.String())
	}

	assertErrorContains(t, w, "param_id 无效或已过期")
}

func TestMiddlewareInvalidJSONReturns400(t *testing.T) {
	mw := newMiniredisMiddleware(t, map[string]string{
		keyForID(DefaultKeyTemplate, "bad"): "{",
	})
	r := newEchoRouter(mw.Handler())

	w := performRequest(r, "/echo?param_id=bad")
	if w.Code != http.StatusBadRequest {
		t.Fatalf("unexpected status: got=%d body=%s", w.Code, w.Body.String())
	}

	assertErrorContains(t, w, "Valkey 参数格式错误")
}

func TestMiddlewareNestedObjectReturns400(t *testing.T) {
	mw := newMiniredisMiddleware(t, map[string]string{
		keyForID(DefaultKeyTemplate, "nested"): `{"server":{"name":"jp"}}`,
	})
	r := newEchoRouter(mw.Handler())

	w := performRequest(r, "/echo?param_id=nested")
	if w.Code != http.StatusBadRequest {
		t.Fatalf("unexpected status: got=%d body=%s", w.Code, w.Body.String())
	}

	assertErrorContains(t, w, "不支持嵌套对象")
}

func TestMiddlewareValkeyUnavailableReturns502(t *testing.T) {
	client := redis.NewClient(&redis.Options{
		Addr:         "127.0.0.1:1",
		DialTimeout:  100 * time.Millisecond,
		ReadTimeout:  100 * time.Millisecond,
		WriteTimeout: 100 * time.Millisecond,
	})
	t.Cleanup(func() { _ = client.Close() })

	mw, err := New(client, "")
	if err != nil {
		t.Fatalf("create middleware failed: %v", err)
	}

	r := newEchoRouter(mw.Handler())
	w := performRequest(r, "/echo?param_id=network")
	if w.Code != http.StatusBadGateway {
		t.Fatalf("unexpected status: got=%d body=%s", w.Code, w.Body.String())
	}

	assertErrorContains(t, w, "Valkey 不可用")
}

func TestIntegrationRouteCanReadInjectedQuery(t *testing.T) {
	mw := newMiniredisMiddleware(t, map[string]string{
		keyForID(DefaultKeyTemplate, "card_1"): `{"id":"1001","server":"cn"}`,
	})

	r := gin.New()
	r.GET("/pjsk/card", mw.Handler(), func(c *gin.Context) {
		c.JSON(http.StatusOK, gin.H{
			"id":     c.Query("id"),
			"server": c.DefaultQuery("server", "jp"),
		})
	})

	w := performRequest(r, "/pjsk/card?param_id=card_1")
	if w.Code != http.StatusOK {
		t.Fatalf("unexpected status: got=%d body=%s", w.Code, w.Body.String())
	}

	var body struct {
		ID     string `json:"id"`
		Server string `json:"server"`
	}
	if err := json.Unmarshal(w.Body.Bytes(), &body); err != nil {
		t.Fatalf("decode response failed: %v", err)
	}
	if body.ID != "1001" || body.Server != "cn" {
		t.Fatalf("unexpected response: %#v", body)
	}
}

func newMiniredisMiddleware(t *testing.T, kv map[string]string) *Middleware {
	t.Helper()

	s := miniredis.RunT(t)
	for key, value := range kv {
		s.Set(key, value)
	}

	client := redis.NewClient(&redis.Options{Addr: s.Addr()})
	t.Cleanup(func() { _ = client.Close() })

	mw, err := New(client, "")
	if err != nil {
		t.Fatalf("create middleware failed: %v", err)
	}
	return mw
}

func newEchoRouter(mw gin.HandlerFunc) *gin.Engine {
	r := gin.New()
	r.GET("/echo", mw, func(c *gin.Context) {
		c.JSON(http.StatusOK, gin.H{
			"query": c.Request.URL.Query(),
		})
	})
	return r
}

func performRequest(r http.Handler, path string) *httptest.ResponseRecorder {
	w := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, path, nil)
	r.ServeHTTP(w, req)
	return w
}

func keyForID(template string, id string) string {
	return strings.ReplaceAll(template, "{id}", id)
}

type echoBody struct {
	Query map[string][]string `json:"query"`
}

func decodeEchoBody(t *testing.T, w *httptest.ResponseRecorder) echoBody {
	t.Helper()
	var body echoBody
	if err := json.Unmarshal(w.Body.Bytes(), &body); err != nil {
		t.Fatalf("decode response failed: %v; body=%s", err, w.Body.String())
	}
	return body
}

func assertQueryValues(t *testing.T, query map[string][]string, key string, expected []string) {
	t.Helper()
	actual, ok := query[key]
	if !ok {
		t.Fatalf("missing query key %q in %#v", key, query)
	}
	if len(actual) != len(expected) {
		t.Fatalf("unexpected value count for %q: got=%v want=%v", key, actual, expected)
	}
	for i := range expected {
		if actual[i] != expected[i] {
			t.Fatalf("unexpected value for %q[%d]: got=%q want=%q", key, i, actual[i], expected[i])
		}
	}
}

func assertErrorContains(t *testing.T, w *httptest.ResponseRecorder, wantSubstr string) {
	t.Helper()
	var body struct {
		Error string `json:"error"`
	}
	if err := json.Unmarshal(w.Body.Bytes(), &body); err != nil {
		t.Fatalf("decode error response failed: %v; body=%s", err, w.Body.String())
	}
	if !strings.Contains(body.Error, wantSubstr) {
		t.Fatalf("unexpected error message: got=%q want substring=%q", body.Error, wantSubstr)
	}
}
