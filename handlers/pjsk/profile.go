package pjsk

import (
	"bytes"
	"encoding/json"
	"fmt"
	htmltemplate "html/template"
	"io"
	"math"
	"net/http"
	"os"
	"regexp"
	"strings"
	"time"

	"github.com/gin-gonic/gin"
)

var (
	pjskProfileHTTPClient         = &http.Client{Timeout: 15 * time.Second}
	pjskProfileAssetDownloader    = downloadAssetByLabel
	pjskProfileHonorLookupLoader  = loadPJSKHonorLookup
	pjskProfileCharacterFinder    = findCharacter
	pjskProfileWordPlaceholderRE  = regexp.MustCompile(`<#.*?>`)
	pjskProfileCharacterGridOrder = []int{
		21, 22, 23, 24, 25, 26,
		1, 2, 3, 4, 0, 0,
		5, 6, 7, 8, 0, 0,
		9, 10, 11, 12, 0, 0,
		13, 14, 15, 16, 0, 0,
		17, 18, 19, 20, 0, 0,
	}
	pjskProfileRadarOrder = []int{21, 22, 23, 24, 25, 26, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20}
	pjskProfileUnitColors = map[string]string{
		"light_sound":    "#4455DD",
		"idol":           "#88DD44",
		"street":         "#EE1166",
		"theme_park":     "#FF9900",
		"school_refusal": "#884499",
		"piapro":         "#33CCBB",
	}
	pjskProfileCharacterColors = map[int]string{
		1: "#33aaee", 2: "#ffdd44", 3: "#ee6666", 4: "#bbdd22",
		5: "#ffccaa", 6: "#99ccff", 7: "#ffaacc", 8: "#99eedd",
		9: "#ff6699", 10: "#00bbdd", 11: "#ff7722", 12: "#0077dd",
		13: "#ffbb00", 14: "#ff66bb", 15: "#33dd99", 16: "#bb88ee",
		17: "#bb6688", 18: "#8888cc", 19: "#ccaa88", 20: "#ddaacc",
		21: "#33ccbb", 22: "#ffcc11", 23: "#ffee11", 24: "#ffbbcc", 25: "#dd4444", 26: "#3366cc",
	}
)

type pjskProfileAPIConfig struct {
	BaseURL string
	Headers map[string]string
}

type pjskProfileUser struct {
	UserID json.Number `json:"userId"`
	Name   string      `json:"name"`
	Rank   int         `json:"rank"`
}

type pjskProfileUserProfile struct {
	Word      string `json:"word"`
	TwitterID string `json:"twitterId"`
}

type pjskProfileUserDeck struct {
	Member1 int `json:"member1"`
	Member2 int `json:"member2"`
	Member3 int `json:"member3"`
	Member4 int `json:"member4"`
	Member5 int `json:"member5"`
}

type pjskProfileUserCard struct {
	CardID                int    `json:"cardId"`
	DefaultImage          string `json:"defaultImage"`
	SpecialTrainingStatus string `json:"specialTrainingStatus"`
	Level                 int    `json:"level"`
	MasterRank            int    `json:"masterRank"`
}

type pjskProfileDifficultyCount struct {
	MusicDifficultyType string `json:"musicDifficultyType"`
	LiveClear           int    `json:"liveClear"`
	FullCombo           int    `json:"fullCombo"`
	AllPerfect          int    `json:"allPerfect"`
}

type pjskProfileHonor struct {
	Seq                int    `json:"seq"`
	HonorID            int    `json:"honorId"`
	ProfileHonorType   string `json:"profileHonorType"`
	BondsHonorWordID   int    `json:"bondsHonorWordId"`
	HonorLevel         int    `json:"honorLevel"`
	BondsHonorViewType string `json:"bondsHonorViewType"`
}

type pjskProfileUserCharacter struct {
	CharacterID   int `json:"characterId"`
	CharacterRank int `json:"characterRank"`
}

type pjskProfileChallengeLiveResult struct {
	CharacterID int   `json:"characterId"`
	HighScore   int64 `json:"highScore"`
}

type pjskProfileChallengeLiveStage struct {
	CharacterID int `json:"characterId"`
	Rank        int `json:"rank"`
}

type pjskRemoteProfile struct {
	User                          pjskProfileUser                 `json:"user"`
	UserProfile                   pjskProfileUserProfile          `json:"userProfile"`
	UserDeck                      pjskProfileUserDeck             `json:"userDeck"`
	UserCards                     []pjskProfileUserCard           `json:"userCards"`
	UserProfileHonors             []pjskProfileHonor              `json:"userProfileHonors"`
	UserMusicDifficultyClearCount []pjskProfileDifficultyCount    `json:"userMusicDifficultyClearCount"`
	UserCharacters                []pjskProfileUserCharacter      `json:"userCharacters"`
	UserChallengeLiveSoloResult   json.RawMessage                 `json:"userChallengeLiveSoloResult"`
	UserChallengeLiveSoloStages   []pjskProfileChallengeLiveStage `json:"userChallengeLiveSoloStages"`
	UpdateTime                    int64                           `json:"update_time"`
	UploadTime                    int64                           `json:"upload_time"`
}

type pjskProfileCardView struct {
	ID            int
	Prefix        string
	CharacterName string
	CharacterUnit string
	Rarity        string
	Attr          string
	Level         int
	MasterRank    int
	ImageMode     string
	Thumbnail     htmltemplate.URL
	Frame         htmltemplate.URL
	AttrIcon      htmltemplate.URL
	Stars         []int
	StarIcon      htmltemplate.URL
}

type pjskProfileDifficultyColumn struct {
	Label           string
	BackgroundStyle htmltemplate.CSS
	CellStyle       htmltemplate.CSS
}

type pjskProfilePlayStatsCell struct {
	Value int
	Style htmltemplate.CSS
}

type pjskProfilePlayStatsRow struct {
	Label  string
	Values []pjskProfilePlayStatsCell
}

type pjskProfileHonorView struct {
	Slot        string
	Title       string
	Subtitle    string
	Level       int
	Kind        string
	Rarity      string
	Description string
	IsMain      bool
	HasArtwork  bool
	Artwork     htmltemplate.HTML
	Width       int
	Height      int
}

type pjskProfileCharacterRankView struct {
	Name  string
	Rank  int
	Empty bool
}

type pjskProfileChallengeLiveView struct {
	Available     bool
	CharacterName string
	StageRank     int
	HighScore     int64
}

type pjskProfilePageData struct {
	Name               string
	UserID             string
	Rank               int
	Server             string
	Word               string
	TwitterID          string
	UpdatedAt          string
	DeckCards          []pjskProfileCardView
	HasLeaderCard      bool
	LeaderCard         pjskProfileCardView
	Honors             []pjskProfileHonorView
	HasHonors          bool
	DifficultyColumns  []pjskProfileDifficultyColumn
	PlayStatsRows      []pjskProfilePlayStatsRow
	HasPlayStats       bool
	CharacterRanks     []pjskProfileCharacterRankView
	HasCharacterRanks  bool
	RadarChart         htmltemplate.HTML
	HasRadarChart      bool
	ChallengeLive      pjskProfileChallengeLiveView
	HasTrainingSection bool
}

type pjskHonorLevel struct {
	Level           int    `json:"level"`
	Description     string `json:"description"`
	HonorRarity     string `json:"honorRarity"`
	AssetbundleName string `json:"assetbundleName"`
}

type pjskHonor struct {
	ID               int              `json:"id"`
	GroupID          int              `json:"groupId"`
	HonorRarity      string           `json:"honorRarity"`
	Name             string           `json:"name"`
	AssetbundleName  string           `json:"assetbundleName"`
	HonorMissionType string           `json:"honorMissionType"`
	Levels           []pjskHonorLevel `json:"levels"`
}

type pjskHonorGroup struct {
	ID                        int    `json:"id"`
	Name                      string `json:"name"`
	HonorType                 string `json:"honorType"`
	BackgroundAssetbundleName string `json:"backgroundAssetbundleName"`
	FrameName                 string `json:"frameName"`
}

type pjskBondsHonorLevel struct {
	Level       int    `json:"level"`
	Description string `json:"description"`
}

type pjskBondsHonor struct {
	ID                    int                   `json:"id"`
	Name                  string                `json:"name"`
	HonorRarity           string                `json:"honorRarity"`
	GameCharacterUnitID1  int                   `json:"gameCharacterUnitId1"`
	GameCharacterUnitID2  int                   `json:"gameCharacterUnitId2"`
	Levels                []pjskBondsHonorLevel `json:"levels"`
	ConfigurableUnitVirtualSinger bool `json:"configurableUnitVirtualSinger"`
}


type pjskBondsHonorWord struct {
	ID              int    `json:"id"`
	AssetbundleName string `json:"assetbundleName"`
	Name            string `json:"name"`
	Description     string `json:"description"`
}

type pjskGameCharacterUnit struct {
	ID              int    `json:"id"`
	GameCharacterID int    `json:"gameCharacterId"`
	Unit            string `json:"unit"`
	ColorCode       string `json:"colorCode"`
}

type pjskHonorLookup struct {
	Honors             map[int]pjskHonor
	Groups             map[int]pjskHonorGroup
	Bonds              map[int]pjskBondsHonor
	BondWords          map[int]pjskBondsHonorWord
	GameCharacterUnits map[int]pjskGameCharacterUnit
}

func parsePJSKProfileHeaders(raw string) (map[string]string, error) {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return nil, nil
	}

	var payload map[string]any
	if err := json.Unmarshal([]byte(raw), &payload); err != nil {
		return nil, fmt.Errorf("需要是 JSON 对象: %w", err)
	}

	headers := make(map[string]string, len(payload))
	for key, value := range payload {
		key = strings.TrimSpace(key)
		if key == "" {
			continue
		}

		switch v := value.(type) {
		case string:
			headers[key] = v
		case float64, bool, json.Number:
			headers[key] = fmt.Sprint(v)
		case nil:
			headers[key] = ""
		default:
			return nil, fmt.Errorf("请求头 %q 的值类型不受支持", key)
		}
	}
	return headers, nil
}

func loadPJSKProfileAPIConfigFromEnv() (*pjskProfileAPIConfig, error) {
	baseURL := strings.TrimRight(strings.TrimSpace(os.Getenv("PJSK_PROFILE_BASEURL")), "/")
	if baseURL == "" {
		return nil, fmt.Errorf("未配置 PJSK Profile API 地址，请设置环境变量 PJSK_PROFILE_BASEURL")
	}

	headers, err := parsePJSKProfileHeaders(os.Getenv("PJSK_PROFILE_HEADERS"))
	if err != nil {
		return nil, fmt.Errorf("解析 PJSK_PROFILE_HEADERS 失败: %w", err)
	}

	return &pjskProfileAPIConfig{
		BaseURL: baseURL,
		Headers: headers,
	}, nil
}

func renderProfileError(c *gin.Context, errMsg string) {
	c.HTML(http.StatusOK, "pjsk/profile", gin.H{"Error": errMsg})
}

// ProfileRawHandler 返回 Profile 原始获取的数据（供插件调用）
func ProfileRawHandler(c *gin.Context) {
	server := c.DefaultQuery("server", "jp")
	if !validServers[server] {
		c.JSON(http.StatusBadRequest, gin.H{"error": "无效的服务器参数，支持: jp, cn, en, tw, kr"})
		return
	}

	userID := strings.TrimSpace(c.Query("id"))
	if userID == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "缺少玩家 ID 参数"})
		return
	}
	if !isDigits(userID) {
		c.JSON(http.StatusBadRequest, gin.H{"error": "无效的玩家 ID: " + userID})
		return
	}

	cfg, err := loadPJSKProfileAPIConfigFromEnv()
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	profile, err := fetchRemotePJSKProfile(cfg, server, userID)
	if err != nil {
		c.JSON(http.StatusBadGateway, gin.H{"error": err.Error()})
		return
	}

	c.JSON(http.StatusOK, profile)
}

func ProfileHandler(c *gin.Context) {
	server := c.DefaultQuery("server", "jp")
	if !validServers[server] {
		renderProfileError(c, "无效的服务器参数，支持: jp, cn, en, tw, kr")
		return
	}

	userID := strings.TrimSpace(c.Query("id"))
	if userID == "" {
		renderProfileError(c, "缺少玩家 ID 参数")
		return
	}
	if !isDigits(userID) {
		renderProfileError(c, "无效的玩家 ID: "+userID)
		return
	}

	cfg, err := loadPJSKProfileAPIConfigFromEnv()
	if err != nil {
		renderProfileError(c, err.Error())
		return
	}

	profile, err := fetchRemotePJSKProfile(cfg, server, userID)
	if err != nil {
		renderProfileError(c, err.Error())
		return
	}

	page := buildPJSKProfilePageData(server, profile)
	c.HTML(http.StatusOK, "pjsk/profile", gin.H{"Profile": page})
}

func fetchRemotePJSKProfile(cfg *pjskProfileAPIConfig, server, userID string) (*pjskRemoteProfile, error) {
	endpoint := cfg.BaseURL + "/" + server + "/" + userID + "/profile"
	req, err := http.NewRequest(http.MethodGet, endpoint, nil)
	if err != nil {
		return nil, fmt.Errorf("创建 Profile API 请求失败: %w", err)
	}
	req.Header.Set("User-Agent", "amiabot-pages/1.0")
	req.Header.Set("Accept", "application/json")
	for key, value := range cfg.Headers {
		req.Header.Set(key, value)
	}

	resp, err := pjskProfileHTTPClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("请求 Profile API 失败: %w", err)
	}
	defer resp.Body.Close()

	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, fmt.Errorf("读取 Profile API 响应失败: %w", err)
	}

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("Profile API 返回 %d: %s", resp.StatusCode, shortenProfileErrorBody(body))
	}

	var profile pjskRemoteProfile
	decoder := json.NewDecoder(bytes.NewReader(body))
	decoder.UseNumber()
	if err := decoder.Decode(&profile); err != nil {
		return nil, fmt.Errorf("解析 Profile API 响应失败: %w", err)
	}

	if strings.TrimSpace(profile.User.UserID.String()) == "" {
		return nil, fmt.Errorf("Profile API 未返回有效的用户信息")
	}

	return &profile, nil
}

func shortenProfileErrorBody(body []byte) string {
	text := strings.TrimSpace(string(body))
	if text == "" {
		return "空响应"
	}
	text = strings.ReplaceAll(text, "\n", " ")
	text = strings.ReplaceAll(text, "\r", " ")
	text = strings.Join(strings.Fields(text), " ")
	if len(text) > 240 {
		return text[:240] + "..."
	}
	return text
}

func buildPJSKProfilePageData(server string, profile *pjskRemoteProfile) pjskProfilePageData {
	userID := strings.TrimSpace(profile.User.UserID.String())
	name := strings.TrimSpace(profile.User.Name)
	if name == "" {
		name = "玩家 " + userID
	}

	serverName := serverNames[server]
	if serverName == "" {
		serverName = strings.ToUpper(server)
	}

	deckCards := buildPJSKProfileDeckCards(server, profile)
	difficultyColumns, playStatsRows := buildPJSKProfilePlayStats(profile.UserMusicDifficultyClearCount)
	honors := buildPJSKProfileHonorViews(server, profile)
	characterRanks := buildPJSKProfileCharacterRanks(server, profile.UserCharacters)
	radarChart := buildPJSKProfileRadarChart(server, profile.UserCharacters)
	challengeLive := buildPJSKProfileChallengeLiveView(server, profile.UserChallengeLiveSoloResult, profile.UserChallengeLiveSoloStages)

	page := pjskProfilePageData{
		Name:               name,
		UserID:             userID,
		Rank:               profile.User.Rank,
		Server:             serverName,
		Word:               sanitizePJSKProfileWord(profile.UserProfile.Word),
		TwitterID:          strings.TrimSpace(profile.UserProfile.TwitterID),
		UpdatedAt:          resolvePJSKProfileUpdatedAt(profile),
		DeckCards:          deckCards,
		Honors:             honors,
		HasHonors:          len(honors) > 0,
		DifficultyColumns:  difficultyColumns,
		PlayStatsRows:      playStatsRows,
		HasPlayStats:       len(difficultyColumns) > 0 && len(playStatsRows) > 0,
		CharacterRanks:     characterRanks,
		HasCharacterRanks:  len(characterRanks) > 0,
		RadarChart:         radarChart,
		HasRadarChart:      radarChart != "",
		ChallengeLive:      challengeLive,
		HasTrainingSection: radarChart != "" || challengeLive.Available,
	}
	if len(deckCards) > 0 {
		page.HasLeaderCard = true
		page.LeaderCard = deckCards[0]
	}
	return page
}

func resolvePJSKProfileUpdatedAt(profile *pjskRemoteProfile) string {
	if profile.UpdateTime > 0 {
		return formatPJSKProfileTimestamp(profile.UpdateTime)
	}
	if profile.UploadTime > 0 {
		return formatPJSKProfileTimestamp(profile.UploadTime)
	}
	return ""
}

func formatPJSKProfileTimestamp(ts int64) string {
	if ts <= 0 {
		return ""
	}
	if ts < 100000000000 {
		return time.Unix(ts, 0).Local().Format("2006-01-02 15:04:05")
	}
	return time.UnixMilli(ts).Local().Format("2006-01-02 15:04:05")
}

func sanitizePJSKProfileWord(word string) string {
	word = pjskProfileWordPlaceholderRE.ReplaceAllString(word, "")
	return strings.TrimSpace(word)
}

func buildPJSKProfileDeckCards(server string, profile *pjskRemoteProfile) []pjskProfileCardView {
	cardStates := make(map[int]pjskProfileUserCard, len(profile.UserCards))
	for _, card := range profile.UserCards {
		cardStates[card.CardID] = card
	}

	memberIDs := []int{
		profile.UserDeck.Member1,
		profile.UserDeck.Member2,
		profile.UserDeck.Member3,
		profile.UserDeck.Member4,
		profile.UserDeck.Member5,
	}

	cards := make([]pjskProfileCardView, 0, len(memberIDs))
	for _, cardID := range memberIDs {
		if cardID <= 0 {
			continue
		}
		state, ok := cardStates[cardID]
		if !ok {
			state = pjskProfileUserCard{CardID: cardID}
		}
		cardView, ok := buildPJSKProfileCardView(server, state)
		if !ok {
			continue
		}
		cards = append(cards, cardView)
	}
	return cards
}

func buildPJSKProfileCardView(server string, state pjskProfileUserCard) (pjskProfileCardView, bool) {
	card, err := findCardFull(server, state.CardID)
	if err != nil {
		return pjskProfileCardView{}, false
	}

	characterName := fmt.Sprintf("角色 #%d", card.CharacterID)
	characterUnit := ""
	if ch := pjskProfileCharacterFinder(server, card.CharacterID); ch != nil {
		characterName = joinPJSKCharacterName(ch.FirstName, ch.GivenName)
		characterUnit = unitName(ch.Unit)
	}

	rarity := rarityNames[card.CardRarityType]
	if rarity == "" {
		rarity = card.CardRarityType
	}
	attr := attrNames[card.Attr]
	if attr == "" {
		attr = card.Attr
	}

	assetStatus, modeLabel := resolvePJSKProfileCardImageMode(card, state)
	starIcon := starDataURL
	if card.CardRarityType == "rarity_birthday" {
		starIcon = birthdayDataURL
	}

	return pjskProfileCardView{
		ID:            card.ID,
		Prefix:        card.Prefix,
		CharacterName: characterName,
		CharacterUnit: characterUnit,
		Rarity:        rarity,
		Attr:          attr,
		Level:         state.Level,
		MasterRank:    state.MasterRank,
		ImageMode:     modeLabel,
		Thumbnail:     pjskProfileAssetDownloader(server, "card:thumbnail:"+card.AssetbundleName+":"+assetStatus),
		Frame:         frameDataURLs[card.CardRarityType],
		AttrIcon:      attrIconDataURLs[card.Attr],
		Stars:         starPositions(card.CardRarityType),
		StarIcon:      starIcon,
	}, true
}

func resolvePJSKProfileCardImageMode(card *pjskCardFull, state pjskProfileUserCard) (string, string) {
	if hasSpecialTraining(card.CardRarityType) {
		if state.DefaultImage == "special_training" && strings.EqualFold(state.SpecialTrainingStatus, "done") {
			return "after_training", "特训后"
		}
		return "normal", "特训前"
	}
	return "normal", "通常"
}

func buildPJSKProfilePlayStats(counts []pjskProfileDifficultyCount) ([]pjskProfileDifficultyColumn, []pjskProfilePlayStatsRow) {
	if len(counts) == 0 {
		return nil, nil
	}

	type diffMeta struct {
		Key             string
		Label           string
		BackgroundStyle htmltemplate.CSS
		CellStyle       htmltemplate.CSS
	}

	metas := []diffMeta{
		{Key: "easy", Label: "EZ", BackgroundStyle: "background:#5AC06E; color:#ffffff;", CellStyle: "background:rgba(90,192,110,0.16); color:#21400d; border-color:rgba(90,192,110,0.35);"},
		{Key: "normal", Label: "NM", BackgroundStyle: "background:#56A4D4; color:#ffffff;", CellStyle: "background:rgba(86,164,212,0.16); color:#12323d; border-color:rgba(86,164,212,0.35);"},
		{Key: "hard", Label: "HD", BackgroundStyle: "background:#EFAF28; color:#ffffff;", CellStyle: "background:rgba(239,175,40,0.16); color:#4b3200; border-color:rgba(239,175,40,0.35);"},
		{Key: "expert", Label: "EX", BackgroundStyle: "background:#E84D53; color:#ffffff;", CellStyle: "background:rgba(232,77,83,0.16); color:#6a1730; border-color:rgba(232,77,83,0.35);"},
		{Key: "master", Label: "MA", BackgroundStyle: "background:#BB58B8; color:#ffffff;", CellStyle: "background:rgba(187,88,184,0.16); color:#5a1570; border-color:rgba(187,88,184,0.35);"},
		{Key: "append", Label: "APD", BackgroundStyle: "background:#EE92BC; color:#ffffff;", CellStyle: "background:rgba(238,146,188,0.16); color:#7b3357; border-color:rgba(238,146,188,0.35);"},
	}

	countMap := make(map[string]pjskProfileDifficultyCount, len(counts))
	for _, count := range counts {
		key := strings.ToLower(strings.TrimSpace(count.MusicDifficultyType))
		if key == "" {
			continue
		}
		countMap[key] = count
	}

	columns := make([]pjskProfileDifficultyColumn, 0, len(metas))
	for _, meta := range metas {
		columns = append(columns, pjskProfileDifficultyColumn{
			Label:           meta.Label,
			BackgroundStyle: meta.BackgroundStyle,
			CellStyle:       meta.CellStyle,
		})
	}

	rows := []pjskProfilePlayStatsRow{
		{Label: "CLEAR", Values: make([]pjskProfilePlayStatsCell, 0, len(metas))},
		{Label: "FC", Values: make([]pjskProfilePlayStatsCell, 0, len(metas))},
		{Label: "AP", Values: make([]pjskProfilePlayStatsCell, 0, len(metas))},
	}
	for _, meta := range metas {
		count := countMap[meta.Key]
		rows[0].Values = append(rows[0].Values, pjskProfilePlayStatsCell{Value: count.LiveClear, Style: meta.CellStyle})
		rows[1].Values = append(rows[1].Values, pjskProfilePlayStatsCell{Value: count.FullCombo, Style: meta.CellStyle})
		rows[2].Values = append(rows[2].Values, pjskProfilePlayStatsCell{Value: count.AllPerfect, Style: meta.CellStyle})
	}

	return columns, rows
}

func buildPJSKProfileCharacterRanks(server string, characters []pjskProfileUserCharacter) []pjskProfileCharacterRankView {
	if len(characters) == 0 {
		return nil
	}

	rankMap := make(map[int]int, len(characters))
	for _, character := range characters {
		rankMap[character.CharacterID] = character.CharacterRank
	}

	result := make([]pjskProfileCharacterRankView, 0, len(pjskProfileCharacterGridOrder))
	for _, characterID := range pjskProfileCharacterGridOrder {
		if characterID == 0 {
			result = append(result, pjskProfileCharacterRankView{Empty: true})
			continue
		}
		characterName := fmt.Sprintf("#%d", characterID)
		if ch := pjskProfileCharacterFinder(server, characterID); ch != nil {
			characterName = shortPJSKCharacterName(ch)
		}
		result = append(result, pjskProfileCharacterRankView{
			Name: characterName,
			Rank: rankMap[characterID],
		})
	}
	return result
}

type pjskRadarCoord struct {
	X float64
	Y float64
}

func buildPJSKProfileRadarChart(server string, characters []pjskProfileUserCharacter) htmltemplate.HTML {
	if len(characters) == 0 {
		return ""
	}

	rankMap := make(map[int]int, len(characters))
	maxValue := 0
	for _, character := range characters {
		rankMap[character.CharacterID] = character.CharacterRank
		if character.CharacterRank > maxValue {
			maxValue = character.CharacterRank
		}
	}
	if maxValue <= 0 {
		maxValue = 10
	}
	maxRank := int(math.Ceil(float64(maxValue)/10.0) * 10)
	if maxRank < 10 {
		maxRank = 10
	}

	const (
		width       = 860.0
		height      = 760.0
		centerX     = 430.0
		centerY     = 360.0
		outerRadius = 250.0
		labelRadius = 316.0
		valueGap    = 18.0
		rings       = 5
	)

	orderedIDs := pjskProfileRadarOrder
	coords := make([]pjskRadarCoord, 0, len(orderedIDs))
	valueCoords := make([]pjskRadarCoord, 0, len(orderedIDs))
	labelCoords := make([]pjskRadarCoord, 0, len(orderedIDs))
	for i, characterID := range orderedIDs {
		angle := -math.Pi/2 + (2*math.Pi*float64(i))/float64(len(orderedIDs))
		cosV := math.Cos(angle)
		sinV := math.Sin(angle)
		labelCoords = append(labelCoords, pjskRadarCoord{X: centerX + cosV*labelRadius, Y: centerY + sinV*labelRadius})
		rank := rankMap[characterID]
		ratio := float64(rank) / float64(maxRank)
		if ratio < 0 {
			ratio = 0
		}
		if ratio > 1 {
			ratio = 1
		}
		pointRadius := outerRadius * ratio
		coords = append(coords, pjskRadarCoord{X: centerX + cosV*pointRadius, Y: centerY + sinV*pointRadius})
		valueCoords = append(valueCoords, pjskRadarCoord{X: centerX + cosV*(pointRadius+valueGap), Y: centerY + sinV*(pointRadius+valueGap)})
	}

	var builder strings.Builder
	fmt.Fprintf(&builder, `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 %.0f %.0f" style="display:block;width:100%%;height:auto;">`, width, height)
	builder.WriteString(`<rect x="0" y="0" width="100%" height="100%" rx="24" fill="#ffffff" opacity="0.55"></rect>`)

	for ring := rings; ring >= 1; ring-- {
		ratio := float64(ring) / float64(rings)
		fill := "rgba(200,224,227,0.10)"
		if ring%2 == 0 {
			fill = "rgba(200,224,227,0.20)"
		}
		fmt.Fprintf(&builder, `<polygon points="%s" fill="%s" stroke="rgba(110,110,110,0.16)" stroke-width="1"/>`, buildPJSKRadarPolygonPoints(orderedIDs, centerX, centerY, outerRadius*ratio), fill)
	}

	for i := range orderedIDs {
		angle := -math.Pi/2 + (2*math.Pi*float64(i))/float64(len(orderedIDs))
		ax := centerX + math.Cos(angle)*outerRadius
		ay := centerY + math.Sin(angle)*outerRadius
		fmt.Fprintf(&builder, `<line x1="%.2f" y1="%.2f" x2="%.2f" y2="%.2f" stroke="rgba(110,110,110,0.22)" stroke-width="1"/>`, centerX, centerY, ax, ay)
	}

	for ring := 1; ring <= rings; ring++ {
		ratio := float64(ring) / float64(rings)
		value := int(math.Round(float64(maxRank) * ratio))
		y := centerY - outerRadius*ratio
		fmt.Fprintf(&builder, `<text x="%.2f" y="%.2f" fill="rgba(85,85,85,0.68)" font-size="12" text-anchor="middle" dominant-baseline="middle">%d</text>`, centerX, y-10, value)
	}

	fmt.Fprintf(&builder, `<polygon points="%s" fill="rgba(131,76,117,0.14)" stroke="#834c75" stroke-width="3"/>`, buildPJSKRadarCoordsPolygonPoints(coords))

	for i, characterID := range orderedIDs {
		name := fmt.Sprintf("#%d", characterID)
		if ch := pjskProfileCharacterFinder(server, characterID); ch != nil {
			name = shortPJSKCharacterName(ch)
		}
		name = htmltemplate.HTMLEscapeString(name)
		color := pjskProfileCharacterColor(characterID)
		label := labelCoords[i]
		point := coords[i]
		valuePoint := valueCoords[i]
		anchor := "middle"
		if label.X < centerX-18 {
			anchor = "end"
		} else if label.X > centerX+18 {
			anchor = "start"
		}
		fmt.Fprintf(&builder, `<text x="%.2f" y="%.2f" fill="%s" font-size="14" font-weight="700" text-anchor="%s" dominant-baseline="middle">%s</text>`, label.X, label.Y, color, anchor, name)
		fmt.Fprintf(&builder, `<circle cx="%.2f" cy="%.2f" r="5.5" fill="%s" stroke="#ffffff" stroke-width="2"/>`, point.X, point.Y, color)
		rank := rankMap[characterID]
		if rank > 0 {
			fmt.Fprintf(&builder, `<text x="%.2f" y="%.2f" fill="%s" font-size="12" font-weight="700" text-anchor="middle" dominant-baseline="middle">%d</text>`, valuePoint.X, valuePoint.Y, color, rank)
		}
	}

	builder.WriteString(`</svg>`)
	return htmltemplate.HTML(builder.String())
}

func buildPJSKRadarPolygonPoints(orderedIDs []int, centerX, centerY, radius float64) string {
	coords := make([]pjskRadarCoord, 0, len(orderedIDs))
	for i := range orderedIDs {
		angle := -math.Pi/2 + (2*math.Pi*float64(i))/float64(len(orderedIDs))
		coords = append(coords, pjskRadarCoord{X: centerX + math.Cos(angle)*radius, Y: centerY + math.Sin(angle)*radius})
	}
	return buildPJSKRadarCoordsPolygonPoints(coords)
}

func buildPJSKRadarCoordsPolygonPoints(coords []pjskRadarCoord) string {
	parts := make([]string, 0, len(coords))
	for _, coord := range coords {
		parts = append(parts, fmt.Sprintf("%.2f,%.2f", coord.X, coord.Y))
	}
	return strings.Join(parts, " ")
}

func pjskProfileCharacterColor(characterID int) string {
	if color, ok := pjskProfileCharacterColors[characterID]; ok && color != "" {
		return color
	}
	switch {
	case characterID >= 21:
		return pjskProfileUnitColors["piapro"]
	case characterID >= 17:
		return pjskProfileUnitColors["school_refusal"]
	case characterID >= 13:
		return pjskProfileUnitColors["theme_park"]
	case characterID >= 9:
		return pjskProfileUnitColors["street"]
	case characterID >= 5:
		return pjskProfileUnitColors["idol"]
	default:
		return pjskProfileUnitColors["light_sound"]
	}
}

func buildPJSKProfileChallengeLiveView(server string, raw json.RawMessage, stages []pjskProfileChallengeLiveStage) pjskProfileChallengeLiveView {
	best := parseBestPJSKChallengeLiveResult(raw)
	if best == nil {
		return pjskProfileChallengeLiveView{}
	}

	stageRank := 0
	for _, stage := range stages {
		if stage.CharacterID == best.CharacterID && stage.Rank > stageRank {
			stageRank = stage.Rank
		}
	}

	characterName := fmt.Sprintf("角色 #%d", best.CharacterID)
	if ch := pjskProfileCharacterFinder(server, best.CharacterID); ch != nil {
		characterName = joinPJSKCharacterName(ch.FirstName, ch.GivenName)
	}

	return pjskProfileChallengeLiveView{
		Available:     true,
		CharacterName: characterName,
		StageRank:     stageRank,
		HighScore:     best.HighScore,
	}
}

func parseBestPJSKChallengeLiveResult(raw json.RawMessage) *pjskProfileChallengeLiveResult {
	trimmed := bytes.TrimSpace(raw)
	if len(trimmed) == 0 || bytes.Equal(trimmed, []byte("null")) {
		return nil
	}

	if trimmed[0] == '[' {
		var results []pjskProfileChallengeLiveResult
		if err := json.Unmarshal(trimmed, &results); err != nil || len(results) == 0 {
			return nil
		}
		best := results[0]
		for _, result := range results[1:] {
			if result.HighScore > best.HighScore {
				best = result
			}
		}
		if best.CharacterID == 0 {
			return nil
		}
		return &best
	}

	var result pjskProfileChallengeLiveResult
	if err := json.Unmarshal(trimmed, &result); err != nil || result.CharacterID == 0 {
		return nil
	}
	return &result
}

func buildPJSKProfileHonorViews(server string, profile *pjskRemoteProfile) []pjskProfileHonorView {
	if profile == nil || len(profile.UserProfileHonors) == 0 {
		return nil
	}

	lookup, err := pjskProfileHonorLookupLoader(server)
	if err != nil {
		return nil
	}

	views := make([]pjskProfileHonorView, 0, len(profile.UserProfileHonors))
	for _, honor := range profile.UserProfileHonors {
		view := buildPJSKProfileHonorView(honor, lookup)
		if view != nil {
			views = append(views, *view)
		}
	}
	return views
}

func buildPJSKProfileHonorView(honor pjskProfileHonor, lookup *pjskHonorLookup) *pjskProfileHonorView {
	seq := honor.Seq
	if seq <= 0 {
		seq = 1
	}

	view := &pjskProfileHonorView{
		Slot:    humanizePJSKProfileHonorSlot(seq),
		Level:   honor.HonorLevel,
		IsMain:  seq == 1,
		Width:   0,
		Height:  0,
	}

	if honor.ProfileHonorType == "bonds" {
		view.Kind = humanizePJSKProfileHonorKind("bonds")
		bondsHonor, ok := lookup.Bonds[honor.HonorID]
		if !ok {
			return nil
		}

		view.Title = bondsHonor.Name
		view.Rarity = humanizePJSKHonorRarity(bondsHonor.HonorRarity)

		// 羁绊头衔可以自定义文字
		if honor.BondsHonorWordID > 0 {
			if word, ok := lookup.BondWords[honor.BondsHonorWordID]; ok {
				view.Subtitle = word.Name
			}
		}

		// 获取等级描述
		view.Description = findPJSKBondsHonorLevelDescription(bondsHonor.Levels, honor.HonorLevel)

		// 获取角色名称作为副标题（如果没有自定义文字）
		if view.Subtitle == "" {
			charNames := getPJSKBondsHonorCharacterNames(&bondsHonor, lookup)
			if len(charNames) > 0 {
				view.Subtitle = strings.Join(charNames, " & ")
			}
		}
	} else {
		// 普通头衔
		view.Kind = humanizePJSKProfileHonorKind("normal")
		normalHonor, ok := lookup.Honors[honor.HonorID]
		if !ok {
			return nil
		}

		view.Title = normalHonor.Name
		view.Rarity = humanizePJSKHonorRarity(normalHonor.HonorRarity)

		// 获取头衔组信息
		if group, ok := lookup.Groups[normalHonor.GroupID]; ok {
			view.Kind = humanizePJSKHonorGroupType(group.HonorType)
		}

		// 获取等级描述
		view.Description = findPJSKHonorLevelDescription(normalHonor.Levels, honor.HonorLevel)
	}

	return view
}

func getPJSKBondsHonorCharacterNames(bondsHonor *pjskBondsHonor, lookup *pjskHonorLookup) []string {
	var names []string

	// 获取角色1
	if unit1, ok := lookup.GameCharacterUnits[bondsHonor.GameCharacterUnitID1]; ok {
		if ch := pjskProfileCharacterFinder("", unit1.GameCharacterID); ch != nil {
			names = append(names, shortPJSKCharacterName(ch))
		}
	}

	// 获取角色2
	if unit2, ok := lookup.GameCharacterUnits[bondsHonor.GameCharacterUnitID2]; ok {
		if ch := pjskProfileCharacterFinder("", unit2.GameCharacterID); ch != nil {
			names = append(names, shortPJSKCharacterName(ch))
		}
	}

	return names
}

func loadPJSKHonorLookup(server string) (*pjskHonorLookup, error) {
	var honors []pjskHonor
	if err := readCachedJSONInto(server, "honors.json", &honors); err != nil {
		return nil, err
	}
	var groups []pjskHonorGroup
	if err := readCachedJSONInto(server, "honorGroups.json", &groups); err != nil {
		return nil, err
	}
	var bonds []pjskBondsHonor
	if err := readCachedJSONInto(server, "bondsHonors.json", &bonds); err != nil {
		return nil, err
	}
	bondWords, err := loadPJSKBondsHonorWords(server)
	if err != nil {
		return nil, err
	}
	var gameCharacterUnits []pjskGameCharacterUnit
	if err := readCachedJSONInto(server, "gameCharacterUnits.json", &gameCharacterUnits); err != nil {
		return nil, err
	}

	lookup := &pjskHonorLookup{
		Honors:            make(map[int]pjskHonor, len(honors)),
		Groups:            make(map[int]pjskHonorGroup, len(groups)),
		Bonds:             make(map[int]pjskBondsHonor, len(bonds)),
		BondWords:         make(map[int]pjskBondsHonorWord, len(bondWords)),
		GameCharacterUnits: make(map[int]pjskGameCharacterUnit, len(gameCharacterUnits)),
	}
	for _, honor := range honors {
		lookup.Honors[honor.ID] = honor
	}
	for _, group := range groups {
		lookup.Groups[group.ID] = group
	}
	for _, bond := range bonds {
		lookup.Bonds[bond.ID] = bond
	}
	for _, word := range bondWords {
		lookup.BondWords[word.ID] = word
	}
	for _, unit := range gameCharacterUnits {
		lookup.GameCharacterUnits[unit.ID] = unit
	}
	return lookup, nil
}

func loadPJSKBondsHonorWords(server string) ([]pjskBondsHonorWord, error) {
	data, err := ReadCachedJSON(server, "bondsHonorWords.json")
	if err != nil {
		return nil, err
	}

	// 尝试解析为对象数组格式 (JP/EN/KR 格式)
	var objectFormat []pjskBondsHonorWord
	if err := json.Unmarshal(data, &objectFormat); err == nil {
		return objectFormat, nil
	}

	// 尝试解析为数组数组格式 (CN 格式)
	// 格式: [id, seq, bondsGroupId, assetbundleName, name, description]
	var arrayFormat [][]interface{}
	if err := json.Unmarshal(data, &arrayFormat); err != nil {
		return nil, fmt.Errorf("解析 bondsHonorWords.json 失败: 无法识别格式")
	}

	result := make([]pjskBondsHonorWord, 0, len(arrayFormat))
	for _, arr := range arrayFormat {
		if len(arr) < 6 {
			continue
		}
		word := pjskBondsHonorWord{}
		if id, ok := arr[0].(float64); ok {
			word.ID = int(id)
		}
		if name, ok := arr[4].(string); ok {
			word.Name = name
		}
		if desc, ok := arr[5].(string); ok {
			word.Description = desc
		}
		if asset, ok := arr[3].(string); ok {
			word.AssetbundleName = asset
		}
		result = append(result, word)
	}
 return result, nil
}

func readCachedJSONInto(server, file string, target any) error {
	data, err := ReadCachedJSON(server, file)
	if err != nil {
		return err
	}
	if err := json.Unmarshal(data, target); err != nil {
		return fmt.Errorf("解析 %s 失败: %w", file, err)
	}
	return nil
}

func findPJSKHonorLevelDescription(levels []pjskHonorLevel, level int) string {
	for _, item := range levels {
		if item.Level == level {
			return strings.TrimSpace(item.Description)
		}
	}
	return ""
}

func findPJSKBondsHonorLevelDescription(levels []pjskBondsHonorLevel, level int) string {
	for _, item := range levels {
		if item.Level == level {
			return strings.TrimSpace(item.Description)
		}
	}
	return ""
}

func humanizePJSKProfileHonorSlot(seq int) string {
	switch seq {
	case 1:
		return "主头衔"
	case 2:
		return "副头衔 1"
	case 3:
		return "副头衔 2"
	default:
		return "头衔"
	}
}

func humanizePJSKProfileHonorKind(kind string) string {
	switch strings.ToLower(strings.TrimSpace(kind)) {
	case "bonds":
		return "羁绊头衔"
	default:
		return "普通头衔"
	}
}

func humanizePJSKHonorGroupType(honorType string) string {
	switch strings.ToLower(strings.TrimSpace(honorType)) {
	case "character":
		return "角色"
	case "achievement":
		return "成就"
	case "event":
		return "活动"
	case "limitevent":
		return "活动应援"
	case "rank_match":
		return "排位"
	case "birthday":
		return "生日"
	case "license":
		return "许可"
	case "unit_rank":
		return "团体等级"
	case "world_bloom":
		return "世界开花"
	case "main_story":
		return "主线剧情"
	case "challenge_live":
		return "Challenge Live"
	case "virtual_live":
		return "虚拟 Live"
	case "event_point":
		return "活动点数"
	default:
		return strings.TrimSpace(honorType)
	}
}

func humanizePJSKHonorRarity(rarity string) string {
	switch strings.ToLower(strings.TrimSpace(rarity)) {
	case "low":
		return "低"
	case "middle":
		return "中"
	case "high":
		return "高"
	case "highest":
		return "最高"
	default:
		return strings.TrimSpace(rarity)
	}
}

func shortPJSKCharacterName(ch *pjskGameCharacter) string {
	if ch == nil {
		return "未知角色"
	}
	if givenName := strings.TrimSpace(ch.GivenName); givenName != "" {
		return givenName
	}
	if firstName := strings.TrimSpace(ch.FirstName); firstName != "" {
		return firstName
	}
	return fmt.Sprintf("角色 #%d", ch.ID)
}

func joinPJSKCharacterName(firstName, givenName string) string {
	parts := make([]string, 0, 2)
	if strings.TrimSpace(firstName) != "" {
		parts = append(parts, strings.TrimSpace(firstName))
	}
	if strings.TrimSpace(givenName) != "" {
		parts = append(parts, strings.TrimSpace(givenName))
	}
	if len(parts) == 0 {
		return "未知角色"
	}
	return strings.Join(parts, " ")
}

func isDigits(s string) bool {
	if s == "" {
		return false
	}
	for _, r := range s {
		if r < '0' || r > '9' {
			return false
		}
	}
	return true
}
