package query

import (
	htmltemplate "html/template"
	"net/http"
	"strconv"
	"strings"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/xiaocaoooo/amiabot-pages/pkg/imgcache"
)

const queryTextLimit = 180

var queryImageDownloader = func(imageURL string, ttl time.Duration, headers map[string]string) htmltemplate.URL {
	return imgcache.Default.Download(strings.TrimSpace(imageURL), ttl, headers)
}

type displayItem struct {
	Label string
	Value string
}

type displaySection struct {
	Title        string
	GridTemplate string
	Items        []displayItem
}

type textBlock struct {
	Title string
	Value string
}

type userPageData struct {
	Avatar      htmltemplate.URL
	DisplayName string
	ID          string
	Badges      []string
	Sections    []displaySection
	TextBlocks  []textBlock
}

type groupPageData struct {
	Avatar     htmltemplate.URL
	Name       string
	ID         string
	Badges     []string
	Sections   []displaySection
	TextBlocks []textBlock
}

func UserHandler(c *gin.Context) {
	id := strings.TrimSpace(c.Query("id"))
	if id == "" {
		renderQueryError(c, "query/user", "缺少用户 ID 参数")
		return
	}
	if !isDigits(id) {
		renderQueryError(c, "query/user", "无效的用户 ID")
		return
	}

	nickname := clampText(c.Query("nickname"), 48)
	card := clampText(c.Query("card"), 48)
	roleText := humanizeRole(c.Query("role"))
	title := clampText(c.Query("title"), 32)
	vipText := humanizeVIP(c.Query("is_vip"), c.Query("is_years_vip"), parsePositiveInt(c.Query("vip_level")))
	onlineText := humanizeOnlineStatus(parsePositiveInt(c.Query("online_status")), parsePositiveInt(c.Query("online_ext_status")))

	sections := make([]displaySection, 0, 6)
	sections = appendSection(sections, "身份标识", "repeat(2, minmax(0, 1fr))", []displayItem{
		item("昵称", firstNonEmpty(nickname, id)),
		itemIfDiff("备注", c.Query("remark"), nickname),
		item("QID", c.Query("qid")),
		item("分组名称", c.Query("category_name")),
		item("分组 ID", c.Query("category_id")),
	})
	sections = appendSection(sections, "基础资料", "repeat(3, minmax(0, 1fr))", []displayItem{
		item("性别", humanizeSex(c.Query("sex"))),
		item("年龄", humanizeInt(parsePositiveInt(c.Query("age")), "未知", " 岁")),
		item("Q龄", firstNonEmpty(c.Query("qage"), humanizeQAge(parsePositiveInt(c.Query("reg_year"))))),
		item("注册年份", humanizeInt(parsePositiveInt(c.Query("reg_year")), "未知", " 年注册")),
		item("QQ 等级", humanizeInt(parsePositiveInt(c.Query("qq_level")), "未知", " 级")),
		item("生日", c.Query("birthday")),
		item("手机号", c.Query("phone_num")),
		item("邮箱", c.Query("email")),
	})
	sections = appendSection(sections, "群内信息", "repeat(3, minmax(0, 1fr))", []displayItem{
		item("群名片", nonEmptyOr(c.Query("card"), "未设置")),
		item("群等级", nonEmptyOr(c.Query("group_level"), "未提供")),
		item("地区", nonEmptyOr(c.Query("area"), "未提供")),
		item("是否机器人", humanizeOptionalBool(c.Query("is_robot"))),
		item("不良记录", humanizeOptionalBool(c.Query("unfriendly"))),
	})
	sections = appendSection(sections, "动态状态", "repeat(2, minmax(0, 1fr))", []displayItem{
		item("在线状态", firstNonEmpty(onlineText, "未提供")),
		item("角色", roleText),
		item("入群时间", c.Query("join_time")),
		item("最后发言", c.Query("last_sent_time")),
		item("禁言至", c.Query("mute_until")),
		item("头衔到期", c.Query("title_expire_time")),
	})

	textBlocks := make([]textBlock, 0, 1)
	if value := clampText(c.Query("long_nick"), queryTextLimit); value != "" {
		textBlocks = append(textBlocks, textBlock{Title: "个性签名", Value: value})
	}

	badges := make([]string, 0, 4)
	if roleText != "未提供" {
		badges = append(badges, roleText)
	}
	if title != "" {
		badges = append(badges, title)
	}
	if vipText != "未提供" {
		badges = append(badges, vipText)
	}
	if onlineText != "" {
		badges = append(badges, onlineText)
	}

	data := userPageData{
		Avatar:      queryImageDownloader(qqAvatarURL(id), -1, nil),
		DisplayName: firstNonEmpty(card, nickname, id),
		ID:          id,
		Badges:      badges,
		Sections:    sections,
		TextBlocks:  textBlocks,
	}

	c.HTML(http.StatusOK, "query/user", gin.H{"User": data})
}

func GroupHandler(c *gin.Context) {
	id := strings.TrimSpace(c.Query("id"))
	if id == "" {
		renderQueryError(c, "query/group", "缺少群 ID 参数")
		return
	}
	if !isDigits(id) {
		renderQueryError(c, "query/group", "无效的群 ID")
		return
	}

	level := parsePositiveInt(c.Query("level"))
	mutedAll, mutedAllKnown := parseOptionalBool(c.Query("is_muted_all"))
	canAtAll, canAtAllKnown := parseOptionalBool(c.Query("can_at_all"))
	badges := []string{humanizeInt(level, "未公开", " 级")}
	if mutedAllKnown {
		badges = append(badges, map[bool]string{true: "全员禁言中", false: "未开启全员禁言"}[mutedAll])
	}
	if remark := clampText(c.Query("remark"), 64); remark != "" {
		badges = append(badges, "备注："+remark)
	}
	if canAtAllKnown {
		badges = append(badges, map[bool]string{true: "可 @全体", false: "不可 @全体"}[canAtAll])
	}

	sexParts := make([]string, 0, 3)
	maleCount := parsePositiveInt(c.Query("male_count"))
	femaleCount := parsePositiveInt(c.Query("female_count"))
	unknownCount := parsePositiveInt(c.Query("unknown_sex_count"))
	if maleCount > 0 {
		sexParts = append(sexParts, "男 "+strconv.Itoa(maleCount))
	}
	if femaleCount > 0 {
		sexParts = append(sexParts, "女 "+strconv.Itoa(femaleCount))
	}
	if unknownCount > 0 {
		sexParts = append(sexParts, "未知 "+strconv.Itoa(unknownCount))
	}

	sections := make([]displaySection, 0, 7)
	sections = appendSection(sections, "规模统计", "repeat(3, minmax(0, 1fr))", []displayItem{
		item("群人数", humanizeMembers(parsePositiveInt(c.Query("member_count")), parsePositiveInt(c.Query("max_member_count")))),
		item("活跃人数", humanizeInt(parsePositiveInt(c.Query("active_member_count")), "未提供", " 人")),
		item("近 7 日活跃", humanizeInt(parsePositiveInt(c.Query("derived_active_member_count")), "未提供", " 人")),
		item("管理员数", humanizeInt(parsePositiveInt(c.Query("admin_count")), "未提供", " 人")),
		item("机器人数", humanizeInt(parsePositiveInt(c.Query("robot_count")), "未提供", " 个")),
		item("被禁言人数", humanizeInt(parsePositiveInt(c.Query("muted_count")), "未提供", " 人")),
		// item("头衔人数", humanizeInt(parsePositiveInt(c.Query("title_count")), "未提供", " 人")),
		// item("名片人数", humanizeInt(parsePositiveInt(c.Query("card_count")), "未提供", " 人")),
		item("不良记录人数", humanizeInt(parsePositiveInt(c.Query("unfriendly_count")), "未提供", " 人")),
		item("性别分布", nonEmptyOr(strings.Join(sexParts, " / "), "未提供")),
	})
	sections = appendSection(sections, "基础信息", "repeat(2, minmax(0, 1fr))", []displayItem{
		item("群主", c.Query("owner_id")),
		item("创建时间", c.Query("create_time")),
		item("群备注", c.Query("remark")),
		item("幸运字符", c.Query("lucky_word")),
	})
	sections = appendSection(sections, "群荣誉", "repeat(2, minmax(0, 1fr))", []displayItem{
		item("当前龙王", c.Query("current_talkative")),
		item("群聊之火 TOP", c.Query("talkative_top")),
		item("群聊炽焰 TOP", c.Query("performer_top")),
		item("龙王榜 TOP", c.Query("legend_top")),
		item("快乐源泉 TOP", c.Query("emotion_top")),
		item("冒尖小春笋 TOP", c.Query("strong_newbie_top")),
	})
	sections = appendSection(sections, "公告信息", "repeat(2, minmax(0, 1fr))", []displayItem{
		item("最新公告时间", c.Query("latest_notice_time")),
		item("公告发送者", c.Query("latest_notice_sender_id")),
		item("公告阅读数", humanizeInt(parsePositiveInt(c.Query("latest_notice_read_num")), "未提供", "")),
	})

	textBlocks := make([]textBlock, 0, 4)
	if value := clampText(c.Query("description"), queryTextLimit); value != "" {
		textBlocks = append(textBlocks, textBlock{Title: "群介绍", Value: value})
	}
	if value := clampText(c.Query("rules"), queryTextLimit); value != "" {
		textBlocks = append(textBlocks, textBlock{Title: "群规", Value: value})
	}
	if value := clampText(c.Query("join_question"), queryTextLimit); value != "" {
		textBlocks = append(textBlocks, textBlock{Title: "加群问题", Value: value})
	}
	if value := clampText(c.Query("latest_notice_text"), queryTextLimit); value != "" {
		textBlocks = append(textBlocks, textBlock{Title: "群公告内容", Value: value})
	}

	data := groupPageData{
		Avatar:     queryImageDownloader(groupAvatarURL(id), -1, nil),
		Name:       firstNonEmpty(clampText(c.Query("name"), 48), "群聊 "+id),
		ID:         id,
		Badges:     compactStrings(badges),
		Sections:   sections,
		TextBlocks: textBlocks,
	}

	c.HTML(http.StatusOK, "query/group", gin.H{"Group": data})
}

func renderQueryError(c *gin.Context, templateName, errMsg string) {
	c.HTML(http.StatusOK, templateName, gin.H{"Error": strings.TrimSpace(errMsg)})
}

func qqAvatarURL(id string) string {
	return "https://q1.qlogo.cn/g?b=qq&nk=" + strings.TrimSpace(id) + "&s=0"
}

func groupAvatarURL(id string) string {
	trimmed := strings.TrimSpace(id)
	return "https://p.qlogo.cn/gh/" + trimmed + "/" + trimmed + "/0"
}

func item(label, value string) displayItem {
	value = strings.TrimSpace(value)
	if value == "" {
		return displayItem{}
	}
	return displayItem{Label: label, Value: value}
}

func itemIfDiff(label, value, compare string) displayItem {
	value = strings.TrimSpace(value)
	if value == "" || value == strings.TrimSpace(compare) {
		return displayItem{}
	}
	return displayItem{Label: label, Value: value}
}

func appendSection(sections []displaySection, title string, grid string, items []displayItem) []displaySection {
	filtered := make([]displayItem, 0, len(items))
	for _, it := range items {
		if strings.TrimSpace(it.Label) == "" || strings.TrimSpace(it.Value) == "" {
			continue
		}
		filtered = append(filtered, it)
	}
	if len(filtered) == 0 {
		return sections
	}
	sections = append(sections, displaySection{Title: title, GridTemplate: grid, Items: filtered})
	return sections
}

func compactStrings(values []string) []string {
	result := make([]string, 0, len(values))
	for _, value := range values {
		value = strings.TrimSpace(value)
		if value != "" {
			result = append(result, value)
		}
	}
	return result
}

func humanizeSex(raw string) string {
	switch normalizeEnum(raw) {
	case "male":
		return "男"
	case "female":
		return "女"
	default:
		return "未知"
	}
}

func humanizeRole(raw string) string {
	switch normalizeEnum(raw) {
	case "owner":
		return "群主"
	case "admin":
		return "管理员"
	case "member":
		return "成员"
	default:
		return "未提供"
	}
}

func humanizeInt(value int, empty string, suffix string) string {
	if value <= 0 {
		return empty
	}
	return strconv.Itoa(value) + suffix
}

func humanizeQAge(regYear int) string {
	if regYear <= 0 {
		return "未知"
	}
	nowYear := time.Now().Year()
	if regYear > nowYear {
		return "未知"
	}
	qAge := nowYear - regYear
	if qAge < 0 {
		return "未知"
	}
	return strconv.Itoa(qAge) + " 年"
}

func humanizeVIP(isVIPRaw, isYearsVIPRaw string, level int) string {
	isVIP, hasVIP := parseOptionalBool(isVIPRaw)
	isYearsVIP, hasYearsVIP := parseOptionalBool(isYearsVIPRaw)
	if !hasVIP && !hasYearsVIP && level <= 0 {
		return "未提供"
	}
	parts := make([]string, 0, 3)
	if isVIP {
		parts = append(parts, "VIP")
	}
	if isYearsVIP {
		parts = append(parts, "年费")
	}
	if level > 0 {
		parts = append(parts, "Lv."+strconv.Itoa(level))
	}
	if len(parts) == 0 {
		return "普通用户"
	}
	return strings.Join(parts, " · ")
}

func humanizeMembers(memberCount, maxMemberCount int) string {
	if memberCount <= 0 && maxMemberCount <= 0 {
		return "未提供"
	}
	if maxMemberCount <= 0 {
		return strconv.Itoa(memberCount)
	}
	if memberCount <= 0 {
		return "0 / " + strconv.Itoa(maxMemberCount)
	}
	return strconv.Itoa(memberCount) + " / " + strconv.Itoa(maxMemberCount)
}

func humanizeOptionalBool(raw string) string {
	value, ok := parseOptionalBool(raw)
	if !ok {
		return "未提供"
	}
	if value {
		return "是"
	}
	return "否"
}

func humanizeOnlineStatus(status, extStatus int) string {
	statusMap := map[int]string{
		10: "在线",
		30: "离开",
		40: "隐身",
		50: "忙碌",
		60: "Q我吧",
		70: "请勿打扰",
	}
	extMap := map[int]string{
		1000: "我的电量",
		1011: "信号弱",
		1016: "睡觉中",
		1018: "学习中",
		1021: "追剧中",
		1027: "timi中",
		1028: "听歌中",
		1030: "今日天气",
		1032: "熬夜中",
		1051: "恋爱中",
		1052: "我没事",
		1056: "嗨到飞起",
		1058: "元气满满",
		1059: "悠哉哉",
		1060: "无聊中",
		1061: "想静静",
		1062: "我太难了",
		1063: "一言难尽",
		1070: "宝宝认证",
		1071: "好运锦鲤",
		1201: "水逆退散",
		1300: "摸鱼中",
		1401: "emo中",
		2001: "难得糊涂",
		2003: "出去浪",
		2006: "爱你",
		2012: "肝作业",
		2013: "我想开了",
		2014: "被掏空",
		2015: "去旅行",
		2019: "我crash了",
		2023: "搬砖中",
		2025: "一起元梦",
		2026: "求星搭子",
		2037: "春日限定",
	}
	if label := extMap[extStatus]; label != "" {
		return label
	}
	if label := statusMap[status]; label != "" {
		return label
	}
	if status <= 0 && extStatus <= 0 {
		return ""
	}
	return "状态 " + strconv.Itoa(status) + " / " + strconv.Itoa(extStatus)
}

func parsePositiveInt(raw string) int {
	value, err := strconv.Atoi(strings.TrimSpace(raw))
	if err != nil || value <= 0 {
		return 0
	}
	return value
}

func parseOptionalBool(raw string) (bool, bool) {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return false, false
	}
	value, err := strconv.ParseBool(raw)
	if err != nil {
		return false, false
	}
	return value, true
}

func normalizeEnum(raw string) string {
	return strings.ToLower(strings.TrimSpace(raw))
}

func clampText(raw string, limit int) string {
	raw = strings.TrimSpace(raw)
	if raw == "" || limit <= 0 {
		return raw
	}
	runes := []rune(raw)
	if len(runes) <= limit {
		return raw
	}
	return strings.TrimSpace(string(runes[:limit])) + "…"
}

func firstNonEmpty(values ...string) string {
	for _, value := range values {
		value = strings.TrimSpace(value)
		if value != "" {
			return value
		}
	}
	return ""
}

func nonEmptyOr(value, fallback string) string {
	value = strings.TrimSpace(value)
	if value == "" {
		return fallback
	}
	return value
}

func isDigits(s string) bool {
	s = strings.TrimSpace(s)
	if s == "" {
		return false
	}
	for _, ch := range s {
		if ch < '0' || ch > '9' {
			return false
		}
	}
	return true
}
