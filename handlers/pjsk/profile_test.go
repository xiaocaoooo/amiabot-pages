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

func init() {
	gin.SetMode(gin.TestMode)
}

func TestParsePJSKProfileHeaders(t *testing.T) {
	headers, err := parsePJSKProfileHeaders(`{"x-moe-sekai-token":"secret","x-retry":3,"x-enabled":true}`)
	if err != nil {
		t.Fatalf("parsePJSKProfileHeaders() error = %v", err)
	}

	if got := headers["x-moe-sekai-token"]; got != "secret" {
		t.Fatalf("unexpected token header: %q", got)
	}
	if got := headers["x-retry"]; got != "3" {
		t.Fatalf("unexpected retry header: %q", got)
	}
	if got := headers["x-enabled"]; got != "true" {
		t.Fatalf("unexpected enabled header: %q", got)
	}
}

func TestParsePJSKProfileHeadersInvalidJSON(t *testing.T) {
	if _, err := parsePJSKProfileHeaders(`[]`); err == nil {
		t.Fatal("expected error for non-object headers json")
	}
}

func TestParseBestPJSKChallengeLiveResultObject(t *testing.T) {
	result := parseBestPJSKChallengeLiveResult(json.RawMessage(`{"characterId":1,"highScore":123456}`))
	if result == nil {
		t.Fatal("expected challenge live result")
	}
	if result.CharacterID != 1 || result.HighScore != 123456 {
		t.Fatalf("unexpected challenge live result: %#v", result)
	}
}

func TestParseBestPJSKChallengeLiveResultArray(t *testing.T) {
	result := parseBestPJSKChallengeLiveResult(json.RawMessage(`[{"characterId":1,"highScore":123456},{"characterId":2,"highScore":234567}]`))
	if result == nil {
		t.Fatal("expected challenge live result")
	}
	if result.CharacterID != 2 || result.HighScore != 234567 {
		t.Fatalf("unexpected challenge live result: %#v", result)
	}
}

func TestBuildPJSKProfileHonorViews(t *testing.T) {
	oldLoader := pjskProfileHonorLookupLoader
	defer func() {
		pjskProfileHonorLookupLoader = oldLoader
	}()
	pjskProfileHonorLookupLoader = func(server string) (*pjskHonorLookup, error) {
		return &pjskHonorLookup{
			Honors: map[int]pjskHonor{
				1: {ID: 1, GroupID: 1, HonorRarity: "middle", Name: "一歌ファン", AssetbundleName: "honor_0002", Levels: []pjskHonorLevel{{Level: 2, Description: "角色 rank 10"}}},
			},
			Groups: map[int]pjskHonorGroup{
				1: {ID: 1, Name: "一歌粉丝", HonorType: "character", BackgroundAssetbundleName: "honor_0002"},
			},
			Bonds: map[int]pjskBondsHonor{
				1010201: {ID: 1010201, Name: "一歌と咲希", HonorRarity: "high", GameCharacterUnitID1: 1, GameCharacterUnitID2: 2, Levels: []pjskBondsHonorLevel{{Level: 3, Description: "羁绊 rank 15"}}},
			},
			BondWords: map[int]pjskBondsHonorWord{
				1010202: {ID: 1010202, AssetbundleName: "honorname_0102_01", Name: "スマイルフレンド", Description: "羁绊描述"},
			},
			GameCharacterUnits: map[int]pjskGameCharacterUnit{
				1: {ID: 1, GameCharacterID: 1, Unit: "light_sound", ColorCode: "#33aaee"},
				2: {ID: 2, GameCharacterID: 2, Unit: "light_sound", ColorCode: "#ffdd44"},
			},
		}, nil
	}

	views := buildPJSKProfileHonorViews("jp", &pjskRemoteProfile{
		UserProfileHonors: []pjskProfileHonor{
			{Seq: 1, HonorID: 1, ProfileHonorType: "normal", HonorLevel: 2},
			{Seq: 2, HonorID: 1010201, ProfileHonorType: "bonds", BondsHonorWordID: 1010202, HonorLevel: 3},
		},
	})
	if len(views) != 2 {
		t.Fatalf("unexpected honor view count: got=%d", len(views))
	}
	if got := views[0].Title; got != "一歌ファン" {
		t.Fatalf("unexpected normal honor title: %q", got)
	}
	if got := views[1].Title; got != "一歌と咲希" {
		t.Fatalf("unexpected bonds honor title: %q", got)
	}
	if got := views[1].Subtitle; !strings.Contains(got, "スマイルフレンド") {
		t.Fatalf("unexpected bonds honor subtitle: %q", got)
	}
	if !views[0].IsMain {
		t.Fatalf("expected first honor to be main")
	}
	if views[1].IsMain {
		t.Fatalf("expected second honor to be sub")
	}
	// 文本模式下不再有 Artwork 和固定 Width/Height
	if views[0].HasArtwork {
		t.Fatalf("expected no artwork in text mode")
	}
	if views[1].HasArtwork {
		t.Fatalf("expected no artwork in text mode for bonds honor")
	}
}

func TestProfileHandlerRequestsUpstreamAndRendersLunaStyleData(t *testing.T) {
	oldClient := pjskProfileHTTPClient
	oldAssetDownloader := pjskProfileAssetDownloader
	oldHonorLoader := pjskProfileHonorLookupLoader
	oldCharacterFinder := pjskProfileCharacterFinder
	defer func() {
		pjskProfileHTTPClient = oldClient
		pjskProfileAssetDownloader = oldAssetDownloader
		pjskProfileHonorLookupLoader = oldHonorLoader
		pjskProfileCharacterFinder = oldCharacterFinder
	}()
	pjskProfileAssetDownloader = func(server, label string) htmltemplate.URL { return "" }
	pjskProfileHonorLookupLoader = func(server string) (*pjskHonorLookup, error) {
		return &pjskHonorLookup{
			Honors: map[int]pjskHonor{
				1: {ID: 1, GroupID: 1, HonorRarity: "middle", Name: "一歌ファン", Levels: []pjskHonorLevel{{Level: 2, Description: "角色 rank 10"}}},
			},
			Groups: map[int]pjskHonorGroup{
				1: {ID: 1, Name: "一歌粉丝", HonorType: "character"},
			},
			GameCharacterUnits: map[int]pjskGameCharacterUnit{},
		}, nil
	}
	pjskProfileCharacterFinder = func(server string, charID int) *pjskGameCharacter {
		switch charID {
		case 1:
			return &pjskGameCharacter{ID: 1, FirstName: "星乃", GivenName: "一歌", Unit: "light_sound"}
		case 21:
			return &pjskGameCharacter{ID: 21, FirstName: "初音", GivenName: "ミク", Unit: "piapro"}
		default:
			return nil
		}
	}

	var gotPath string
	var gotToken string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		gotPath = r.URL.Path
		gotToken = r.Header.Get("x-moe-sekai-token")
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(map[string]any{
			"user": map[string]any{
				"userId": 123,
				"name":   "测试玩家",
				"rank":   80,
			},
			"userProfile": map[string]any{
				"word":      "<#123>你好世界",
				"twitterId": "test_user",
			},
			"userProfileHonors": []map[string]any{
				{"seq": 1, "honorId": 1, "profileHonorType": "normal", "honorLevel": 2},
			},
			"userCharacters": []map[string]any{
				{"characterId": 21, "characterRank": 12},
				{"characterId": 1, "characterRank": 34},
			},
			"userMusicDifficultyClearCount": []map[string]any{
				{"musicDifficultyType": "easy", "liveClear": 10, "fullCombo": 9, "allPerfect": 8},
				{"musicDifficultyType": "master", "liveClear": 7, "fullCombo": 6, "allPerfect": 5},
			},
			"userChallengeLiveSoloResult": map[string]any{
				"characterId": 1,
				"highScore":   345678,
			},
			"userChallengeLiveSoloStages": []map[string]any{
				{"characterId": 1, "rank": 53},
				{"characterId": 1, "rank": 57},
			},
		})
	}))
	defer server.Close()

	pjskProfileHTTPClient = server.Client()
	t.Setenv("PJSK_PROFILE_BASEURL", server.URL+"/api")
	t.Setenv("PJSK_PROFILE_HEADERS", `{"x-moe-sekai-token":"secret"}`)

	r := newProfileTestRouter(t)
	w := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/pjsk/profile?id=123&server=jp", nil)
	r.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("unexpected status: got=%d body=%s", w.Code, w.Body.String())
	}
	if gotPath != "/api/jp/123/profile" {
		t.Fatalf("unexpected upstream path: %q", gotPath)
	}
	if gotToken != "secret" {
		t.Fatalf("unexpected upstream token header: %q", gotToken)
	}

	body := w.Body.String()
	for _, want := range []string{"测试玩家", "123", "80", "日服", "一歌ファン", "36", "星乃 一歌", "57", "345678", "你好世界", "test_user"} {
		if !strings.Contains(body, want) {
			t.Fatalf("response body missing %q: %s", want, body)
		}
	}
}

func TestProfileHandlerRequiresBaseURL(t *testing.T) {
	t.Setenv("PJSK_PROFILE_BASEURL", "")
	t.Setenv("PJSK_PROFILE_HEADERS", "")

	r := newProfileTestRouter(t)
	w := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/pjsk/profile?id=123&server=jp", nil)
	r.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("unexpected status: got=%d body=%s", w.Code, w.Body.String())
	}
	if !strings.Contains(w.Body.String(), "PJSK_PROFILE_BASEURL") {
		t.Fatalf("unexpected body: %s", w.Body.String())
	}
}

func TestProfileHandlerMissingID(t *testing.T) {
	r := newProfileTestRouter(t)
	w := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/pjsk/profile?server=jp", nil)
	r.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("unexpected status: got=%d body=%s", w.Code, w.Body.String())
	}
	if !strings.Contains(w.Body.String(), "缺少玩家 ID 参数") {
		t.Fatalf("unexpected body: %s", w.Body.String())
	}
}

func TestProfileHandlerInvalidID(t *testing.T) {
	r := newProfileTestRouter(t)
	w := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/pjsk/profile?id=abc&server=jp", nil)
	r.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("unexpected status: got=%d body=%s", w.Code, w.Body.String())
	}
	if !strings.Contains(w.Body.String(), "无效的玩家 ID: abc") {
		t.Fatalf("unexpected body: %s", w.Body.String())
	}
}

func TestProfileHandlerInvalidServer(t *testing.T) {
	r := newProfileTestRouter(t)
	w := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/pjsk/profile?id=123&server=invalid", nil)
	r.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("unexpected status: got=%d body=%s", w.Code, w.Body.String())
	}
	if !strings.Contains(w.Body.String(), "无效的服务器参数") {
		t.Fatalf("unexpected body: %s", w.Body.String())
	}
}

func newProfileTestRouter(t *testing.T) *gin.Engine {
	t.Helper()

	tpl := htmltemplate.Must(htmltemplate.New("profile-test").Parse(`{{define "pjsk/profile"}}{{if .Error}}{{.Error}}{{else}}{{.Profile.Name}}|{{.Profile.UserID}}|{{.Profile.Rank}}|{{.Profile.Server}}|{{len .Profile.Honors}}|{{if .Profile.Honors}}{{(index .Profile.Honors 0).Title}}|{{(index .Profile.Honors 0).HasArtwork}}{{end}}|{{len .Profile.CharacterRanks}}|{{if .Profile.ChallengeLive.Available}}{{.Profile.ChallengeLive.CharacterName}}|{{.Profile.ChallengeLive.StageRank}}|{{.Profile.ChallengeLive.HighScore}}{{end}}|{{.Profile.Word}}|{{.Profile.TwitterID}}{{end}}{{end}}`))
	r := gin.New()
	r.SetHTMLTemplate(tpl)
	r.GET("/pjsk/profile", ProfileHandler)
	return r
}
