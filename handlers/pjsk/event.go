package pjsk

import (
	"encoding/base64"
	"encoding/json"
	"fmt"
	htmltemplate "html/template"
	"net/http"
	"os"
	"path/filepath"
	"strconv"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/xiaocaoooo/amiabot-pages/pkg/imgcache"
)

var validServers = map[string]bool{
	"jp": true, "cn": true, "en": true, "tw": true, "kr": true,
}

type pjskEvent struct {
	ID              int    `json:"id"`
	EventType       string `json:"eventType"`
	Name            string `json:"name"`
	AssetbundleName string `json:"assetbundleName"`
	StartAt         int64  `json:"startAt"`
	AggregateAt     int64  `json:"aggregateAt"`
	ClosedAt        int64  `json:"closedAt"`
	Unit            string `json:"unit"`
}

type pjskEventCard struct {
	CardID  int `json:"cardId"`
	EventID int `json:"eventId"`
}

type pjskCard struct {
	ID              int    `json:"id"`
	CharacterID     int    `json:"characterId"`
	CardRarityType  string `json:"cardRarityType"`
	Attr            string `json:"attr"`
	Prefix          string `json:"prefix"`
	AssetbundleName string `json:"assetbundleName"`
}

type currentEventResp struct {
	EventID int `json:"eventId"`
}

// cardDisplay 传给模板的卡面展示数据
type cardDisplay struct {
	Prefix    string
	Rarity    string
	Attr      string
	Thumbnail htmltemplate.URL
	Frame     htmltemplate.URL // 边框 data URL
	AttrIcon  htmltemplate.URL // 属性图标 data URL
	Stars     []int            // 星星 x 坐标列表
	StarIcon  htmltemplate.URL // 星星/birthday 图标 data URL
}

var eventTypeNames = map[string]string{
	"marathon":          "马拉松",
	"cheerful_carnival": "欢乐嘉年华",
	"world_bloom":       "世界绽放",
	"world_link":        "世界连结",
}

var unitNames = map[string]string{
	"idol":           "MORE MORE JUMP!",
	"light_sound":    "Leo/need",
	"school_refusal": "25时、ナイトコードで。",
	"street":         "Vivid BAD SQUAD",
	"theme_park":     "ワンダーランズ×ショウタイム",
	"none":           "混合",
}

var serverNames = map[string]string{
	"jp": "日服", "cn": "国服", "en": "国际服", "tw": "台服", "kr": "韩服",
}

var rarityNames = map[string]string{
	"rarity_1":        "★",
	"rarity_2":        "★★",
	"rarity_3":        "★★★",
	"rarity_4":        "★★★★",
	"rarity_birthday": "Birthday",
}

var attrNames = map[string]string{
	"cool":       "Cool",
	"cute":       "Cute",
	"happy":      "Happy",
	"mysterious": "Mysterious",
	"pure":       "Pure",
}

// --- 卡面 SVG 静态资源 (启动时加载为 data URL) ---

const cardAssetDir = "static/pjsk/card"

var (
	frameDataURLs    map[string]htmltemplate.URL // rarity -> frame data URL
	attrIconDataURLs map[string]htmltemplate.URL // attr -> icon data URL
	starDataURL      htmltemplate.URL
	birthdayDataURL  htmltemplate.URL
)

func loadPNGAsDataURL(path string) htmltemplate.URL {
	data, err := os.ReadFile(path)
	if err != nil {
		return htmltemplate.URL("")
	}
	return htmltemplate.URL("data:image/png;base64," + base64.StdEncoding.EncodeToString(data))
}

func init() {
	frameDataURLs = map[string]htmltemplate.URL{
		"rarity_1":        loadPNGAsDataURL(filepath.Join(cardAssetDir, "cardFrame_S_1.png")),
		"rarity_2":        loadPNGAsDataURL(filepath.Join(cardAssetDir, "cardFrame_S_2.png")),
		"rarity_3":        loadPNGAsDataURL(filepath.Join(cardAssetDir, "cardFrame_S_3.png")),
		"rarity_4":        loadPNGAsDataURL(filepath.Join(cardAssetDir, "cardFrame_S_4.png")),
		"rarity_birthday": loadPNGAsDataURL(filepath.Join(cardAssetDir, "cardFrame_S_bd.png")),
	}
	attrIconDataURLs = map[string]htmltemplate.URL{
		"cool":       loadPNGAsDataURL(filepath.Join(cardAssetDir, "icon_attribute_cool.png")),
		"cute":       loadPNGAsDataURL(filepath.Join(cardAssetDir, "icon_attribute_cute.png")),
		"happy":      loadPNGAsDataURL(filepath.Join(cardAssetDir, "icon_attribute_happy.png")),
		"mysterious": loadPNGAsDataURL(filepath.Join(cardAssetDir, "icon_attribute_mysterious.png")),
		"pure":       loadPNGAsDataURL(filepath.Join(cardAssetDir, "icon_attribute_pure.png")),
	}
	starDataURL = loadPNGAsDataURL(filepath.Join(cardAssetDir, "rarity_star_normal.png"))
	birthdayDataURL = loadPNGAsDataURL(filepath.Join(cardAssetDir, "rarity_birthday.png"))
}

// starPositions 根据稀有度返回星星的 x 坐标列表
func starPositions(rarity string) []int {
	switch rarity {
	case "rarity_1":
		return []int{10}
	case "rarity_2":
		return []int{10, 36}
	case "rarity_3":
		return []int{10, 36, 62}
	case "rarity_4":
		return []int{10, 36, 62, 88}
	case "rarity_birthday":
		return []int{10}
	default:
		return nil
	}
}

// --- 数据查询（基于 assets 缓存）---

// findEvent 从缓存的 events.json 中查找活动
func findEvent(server string, eventID int) (*pjskEvent, error) {
	data, err := ReadCachedJSON(server, "events.json")
	if err != nil {
		return nil, err
	}

	var events []pjskEvent
	if err := json.Unmarshal(data, &events); err != nil {
		return nil, fmt.Errorf("解析活动数据失败: %w", err)
	}

	for i := range events {
		if events[i].ID == eventID {
			return &events[i], nil
		}
	}
	return nil, fmt.Errorf("未找到活动 ID: %d (服务器: %s)", eventID, server)
}

// findEventCards 查找活动关联的卡面信息
func findEventCards(server string, eventID int) []cardDisplay {
	ecData, err := ReadCachedJSON(server, "eventCards.json")
	if err != nil {
		return nil
	}
	var eventCards []pjskEventCard
	if err := json.Unmarshal(ecData, &eventCards); err != nil {
		return nil
	}

	// 收集该活动的 cardID
	var cardIDs []int
	for _, ec := range eventCards {
		if ec.EventID == eventID {
			cardIDs = append(cardIDs, ec.CardID)
		}
	}
	if len(cardIDs) == 0 {
		return nil
	}

	cData, err := ReadCachedJSON(server, "cards.json")
	if err != nil {
		return nil
	}
	var cards []pjskCard
	if err := json.Unmarshal(cData, &cards); err != nil {
		return nil
	}

	// 建 cardID -> card 索引
	cardMap := make(map[int]*pjskCard, len(cards))
	for i := range cards {
		cardMap[cards[i].ID] = &cards[i]
	}

	var result []cardDisplay
	for _, cid := range cardIDs {
		card, ok := cardMap[cid]
		if !ok {
			continue
		}
		thumbURL := "https://storage.sekai.best/sekai-jp-assets/thumbnail/chara/" + card.AssetbundleName + "_normal.webp"
		thumb := imgcache.Default.Download(thumbURL, -1, nil)

		rarity := card.CardRarityType
		if name, ok := rarityNames[rarity]; ok {
			rarity = name
		}
		attr := card.Attr
		if name, ok := attrNames[attr]; ok {
			attr = name
		}

		// SVG 资源
		frame := frameDataURLs[card.CardRarityType]
		attrIcon := attrIconDataURLs[card.Attr]
		stars := starPositions(card.CardRarityType)
		starIcon := starDataURL
		if card.CardRarityType == "rarity_birthday" {
			starIcon = birthdayDataURL
		}

		result = append(result, cardDisplay{
			Prefix:    card.Prefix,
			Rarity:    rarity,
			Attr:      attr,
			Thumbnail: thumb,
			Frame:     frame,
			AttrIcon:  attrIcon,
			Stars:     stars,
			StarIcon:  starIcon,
		})
	}
	return result
}

func getAssetURL(server, assetbundleName string) string {
	return "https://storage.sekai.best/sekai-" + server + "-assets/event/" + assetbundleName + "/screen/bg.png"
}

func getLogoURL(server, assetbundleName string) string {
	return "https://storage.sekai.best/sekai-" + server + "-assets/event/" + assetbundleName + "/logo/logo.webp"
}

func getBannerURL(server, assetbundleName string) string {
	return "https://storage.sekai.best/sekai-" + server + "-assets/home/banner/" + assetbundleName + "/" + assetbundleName + ".webp"
}

func formatMillisTime(ms int64) string {
	if ms <= 0 {
		return ""
	}
	return time.Unix(ms/1000, 0).Local().Format("2006-01-02 15:04:05")
}

func eventTypeName(t string) string {
	if name, ok := eventTypeNames[t]; ok {
		return name + " (" + t + ")"
	}
	return t
}

func eventStatus(startAt, closedAt int64) string {
	now := time.Now().UnixMilli()
	if now < startAt {
		return "未开始"
	}
	if now > closedAt {
		return "已结束"
	}
	return "进行中"
}

func unitName(u string) string {
	if name, ok := unitNames[u]; ok {
		return name
	}
	return u
}

// eventProgress 返回活动进度百分比（0~100），未开始返回 0，已结束返回 100
func eventProgress(startAt, closedAt int64) float64 {
	now := time.Now().UnixMilli()
	if now <= startAt {
		return 0
	}
	if now >= closedAt {
		return 100
	}
	total := float64(closedAt - startAt)
	elapsed := float64(now - startAt)
	return elapsed / total * 100
}

func renderEventError(c *gin.Context, errMsg string) {
	c.HTML(http.StatusOK, "pjsk/event", gin.H{
		"Error": errMsg,
	})
}

func EventHandler(c *gin.Context) {
	server := c.DefaultQuery("server", "jp")
	if !validServers[server] {
		renderEventError(c, "无效的服务器参数，支持: jp, cn, en, tw, kr")
		return
	}

	idStr := c.Query("id")
	if idStr == "" {
		c.Redirect(http.StatusFound, "/pjsk/event/current?server="+server)
		return
	}

	eventID, err := strconv.Atoi(idStr)
	if err != nil {
		renderEventError(c, "无效的活动 ID: "+idStr)
		return
	}

	target, err := findEvent(server, eventID)
	if err != nil {
		renderEventError(c, err.Error())
		return
	}

	// 下载背景图、徽标、横幅
	bgURL := getAssetURL(server, target.AssetbundleName)
	bgDataURL := imgcache.Default.Download(bgURL, -1, nil)

	logoURL := getLogoURL(server, target.AssetbundleName)
	logoDataURL := imgcache.Default.Download(logoURL, -1, nil)

	bannerURL := getBannerURL(server, target.AssetbundleName)
	bannerDataURL := imgcache.Default.Download(bannerURL, -1, nil)

	// 查找活动关联卡面
	cards := findEventCards(server, eventID)

	c.HTML(http.StatusOK, "pjsk/event", gin.H{
		"Name":        target.Name,
		"ID":          target.ID,
		"EventType":   eventTypeName(target.EventType),
		"Server":      serverNames[server],
		"ServerKey":   server,
		"Unit":        unitName(target.Unit),
		"Background":  bgDataURL,
		"Logo":        logoDataURL,
		"Banner":      bannerDataURL,
		"StartAt":     formatMillisTime(target.StartAt),
		"ClosedAt":    formatMillisTime(target.ClosedAt),
		"AggregateAt": formatMillisTime(target.AggregateAt),
		"Status":      eventStatus(target.StartAt, target.ClosedAt),
		"Progress":    fmt.Sprintf("%.1f", eventProgress(target.StartAt, target.ClosedAt)),
		"Cards":       cards,
	})
}

func CurrentEventHandler(c *gin.Context) {
	server := c.DefaultQuery("server", "jp")
	if !validServers[server] {
		c.JSON(http.StatusBadRequest, gin.H{"error": "无效的服务器参数，支持: jp, cn, en, tw, kr"})
		return
	}

	apiURL := "https://strapi.sekai.best/sekai-current-event"
	if server != "jp" {
		apiURL += "-" + server
	}

	req, err := http.NewRequest(http.MethodGet, apiURL, nil)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "创建请求失败: " + err.Error()})
		return
	}
	req.Header.Set("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")

	client := &http.Client{Timeout: 10 * time.Second}
	resp, err := client.Do(req)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "请求当前活动数据失败: " + err.Error()})
		return
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		c.JSON(http.StatusInternalServerError, gin.H{"error": fmt.Sprintf("当前活动接口返回异常状态码: %d", resp.StatusCode)})
		return
	}

	var current currentEventResp
	if err := json.NewDecoder(resp.Body).Decode(&current); err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "解析当前活动数据失败: " + err.Error()})
		return
	}

	if current.EventID <= 0 {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "未获取到当前活动 ID"})
		return
	}

	redirectURL := fmt.Sprintf("/pjsk/event?id=%d&server=%s", current.EventID, server)
	c.Redirect(http.StatusFound, redirectURL)
}
