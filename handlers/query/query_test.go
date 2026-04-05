package query

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

func TestUserHandlerMissingID(t *testing.T) {
	r := newQueryTestRouter(t)
	w := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/query/user", nil)
	r.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("unexpected status: %d", w.Code)
	}
	if !strings.Contains(w.Body.String(), "缺少用户 ID 参数") {
		t.Fatalf("unexpected body: %s", w.Body.String())
	}
}

func TestUserHandlerRendersExtendedSections(t *testing.T) {
	oldDownloader := queryImageDownloader
	queryImageDownloader = func(imageURL string, ttl time.Duration, headers map[string]string) htmltemplate.URL {
		return htmltemplate.URL("data:image/png;base64,ZmFrZQ==")
	}
	defer func() { queryImageDownloader = oldDownloader }()

	r := newQueryTestRouter(t)
	w := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/query/user?id=123456&nickname=测试用户&remark=好友备注&qid=qid-001&long_nick=个性签名&sex=female&age=18&reg_year=2020&qq_level=64&role=admin&group_level=火花&title=测试头衔&join_time=2026-03-01%2012:00:00&last_sent_time=2026-03-02%2012:00:00&area=上海&qage=8%20年&birthday=2000-01-01&phone_num=1234567890&email=test%40example.com&category_name=特别关注&is_vip=true&vip_level=7&online_status=10&online_ext_status=1028&mute_until=2026-03-03%2012:00:00&title_expire_time=2026-03-04%2012:00:00&card=群名片&unfriendly=false&is_robot=true", nil)
	r.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("unexpected status: %d body=%s", w.Code, w.Body.String())
	}
	body := w.Body.String()
	for _, want := range []string{"测试用户", "好友备注", "qid-001", "个性签名", "管理员", "测试头衔", "听歌中", "上海", "1234567890", "特别关注"} {
		if !strings.Contains(body, want) {
			t.Fatalf("body missing %q: %s", want, body)
		}
	}
	if strings.Contains(body, "UID") || strings.Contains(body, "登录天数") || strings.Contains(body, "社交数据") || strings.Contains(body, "名片可修改") {
		t.Fatalf("body should not contain removed fields: %s", body)
	}
	if strings.Index(body, "个性签名") > strings.Index(body, "身份标识") {
		t.Fatalf("个性签名应当提前显示: %s", body)
	}
}

func TestGroupHandlerRendersExtendedSections(t *testing.T) {
	oldDownloader := queryImageDownloader
	queryImageDownloader = func(imageURL string, ttl time.Duration, headers map[string]string) htmltemplate.URL {
		return htmltemplate.URL("data:image/png;base64,ZmFrZQ==")
	}
	defer func() { queryImageDownloader = oldDownloader }()

	r := newQueryTestRouter(t)
	w := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodGet, "/query/group?id=987654&name=测试群&remark=群备注&level=5&create_time=2026-03-01%2010:00:00&member_count=88&max_member_count=200&active_member_count=66&derived_active_member_count=51&owner_id=10001&description=这是群介绍&rules=这是群规&join_question=你的推是谁&is_muted_all=true&admin_count=3&robot_count=2&muted_count=4&card_count=40&title_count=8&unfriendly_count=1&male_count=30&female_count=20&unknown_sex_count=10&current_talkative=龙王A&talkative_top=群聊之火A&performer_top=炽焰A&legend_top=传奇A&emotion_top=快乐源泉A&strong_newbie_top=新人A&latest_notice_text=公告内容&latest_notice_time=2026-03-02%2011:00:00&latest_notice_sender_id=12345&latest_notice_read_num=80&lucky_word=Lucky", nil)
	r.ServeHTTP(w, req)

	if w.Code != http.StatusOK {
		t.Fatalf("unexpected status: %d body=%s", w.Code, w.Body.String())
	}
	body := w.Body.String()
	for _, want := range []string{"测试群", "群备注", "88 / 200", "51 人", "龙王A", "公告内容", "Lucky", "群公告内容"} {
		if !strings.Contains(body, want) {
			t.Fatalf("body missing %q: %s", want, body)
		}
	}
	for _, removed := range []string{"绑定频道", "AIO 绑定频道", "公告图片数", "运营信息", "扩展设置"} {
		if strings.Contains(body, removed) {
			t.Fatalf("body should not contain %q: %s", removed, body)
		}
	}
}

func TestHumanizeOnlineStatus(t *testing.T) {
	if got := humanizeOnlineStatus(10, 1028); got != "听歌中" {
		t.Fatalf("humanizeOnlineStatus() = %q, want %q", got, "听歌中")
	}
	if got := humanizeOnlineStatus(60, 0); got != "Q我吧" {
		t.Fatalf("humanizeOnlineStatus() = %q, want %q", got, "Q我吧")
	}
}

func newQueryTestRouter(t *testing.T) *gin.Engine {
	t.Helper()
	r := gin.New()
	r.LoadHTMLFiles(
		"../../templates/layout.html",
		"../../templates/logo.html",
		"../../templates/query/user.html",
		"../../templates/query/group.html",
	)
	r.GET("/query/user", UserHandler)
	r.GET("/query/group", GroupHandler)
	return r
}
