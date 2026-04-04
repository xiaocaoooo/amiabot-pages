package pjsk

import (
	"encoding/json"
	"fmt"
	htmltemplate "html/template"
	"io"
	"net/http"
	"os"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/xiaocaoooo/amiabot-pages/pkg/imgcache"
)

var (
	pjskB30HTTPClient = &http.Client{Timeout: 30 * time.Second}
	b30ChartCache     = struct {
		mu      sync.RWMutex
		data    string
		expires time.Time
	}{}
)

// suiteMusicResult 来自 suite-api userMusicResults 的单条记录
type suiteMusicResult struct {
	MusicID        int    `json:"musicId"`
	MusicDifficulty string `json:"musicDifficultyType"`
	HighScore      int64  `json:"highScore"`
	PlayResult     string `json:"playResult"`
	FullComboFlg   bool   `json:"fullComboFlg"`
	FullPerfectFlg bool   `json:"fullPerfectFlg"`
	PlayType       string `json:"playType"`
}

// B30ChartEntry 歌曲难度 Constant 数据
type B30ChartEntry struct {
	SongID   int
	Diff     string
	Level    float64
	Constant float64
}

// B30ScoreEntry 单个成绩（模板渲染）
type B30ScoreEntry struct {
	Order      int
	Cover      htmltemplate.URL
	Name       string
	SongID     string
	Diff       string // MA/APD/...
	Level      string
	ResultType string // ap / fc / clear
	ResultIcon htmltemplate.URL
	Constant   string
	DiffStyle  htmltemplate.CSS
}

// B30PageData 传递给模板的数据
type B30PageData struct {
	Name           string
	UserID         string
	Server         string
	ServerKey      string
	UserRating     string
	Count          int
	Scores         []B30ScoreEntry
	ChartUpdatedAt string
	UpdatedTime    string
}

const b30ChartURL = "https://raw.githubusercontent.com/moe-sekai/MoeSekai-Hub/main/data/pjskb30/merged_chart.csv"

// ===== helpers =====

func diffNameToNum(diff string) int {
	switch strings.ToLower(diff) {
	case "easy":
		return 0
	case "normal":
		return 1
	case "hard":
		return 2
	case "expert":
		return 3
	case "master":
		return 4
	case "append":
		return 5
	default:
		return -1
	}
}

func diffNumToLabel(n int) string {
	switch n {
	case 0:
		return "EZ"
	case 1:
		return "NM"
	case 2:
		return "HD"
	case 3:
		return "EX"
	case 4:
		return "MA"
	case 5:
		return "APD"
	default:
		return "?"
	}
}

func diffNumToBGStyleHTML(n int) htmltemplate.CSS {
	switch n {
	case 0:
		return "background:#5AC06E;color:#fff;"
	case 1:
		return "background:#56A4D4;color:#fff;"
	case 2:
		return "background:#EFAF28;color:#fff;"
	case 3:
		return "background:#E84D53;color:#fff;"
	case 4:
		return "background:#BB58B8;color:#fff;"
	case 5:
		return "background:#EE92BC;color:#fff;"
	default:
		return "background:#888;color:#fff;"
	}
}

func formatB30(v float64) string { return fmt.Sprintf("%.2f", v) }

func truncateStr(s string, maxLen int) string {
	runes := []rune(s)
	if len(runes) > maxLen {
		return string(runes[:maxLen])
	}
	return s
}

func formatPJSKTimestamp(ts int64) string {
	if ts <= 0 {
		return ""
	}
	if ts < 100000000000 {
		return time.Unix(ts, 0).Local().Format("2006-01-02 15:04:05")
	}
	return time.UnixMilli(ts).Local().Format("2006-01-02 15:04:05")
}

func shortenErrBody(body []byte) string {
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

// ===== chart =====

func getB30ChartCSV() (string, error) {
	b30ChartCache.mu.RLock()
	if time.Now().Before(b30ChartCache.expires) && b30ChartCache.data != "" {
		data := b30ChartCache.data
		b30ChartCache.mu.RUnlock()
		return data, nil
	}
	b30ChartCache.mu.RUnlock()

	b30ChartCache.mu.Lock()
	defer b30ChartCache.mu.Unlock()

	if time.Now().Before(b30ChartCache.expires) && b30ChartCache.data != "" {
		return b30ChartCache.data, nil
	}

	resp, err := pjskB30HTTPClient.Get(b30ChartURL)
	if err != nil {
		if b30ChartCache.data != "" {
			return b30ChartCache.data, nil
		}
		return "", fmt.Errorf("获取难度表失败: %w", err)
	}
	defer resp.Body.Close()

	body, err := io.ReadAll(resp.Body)
	if err != nil {
		if b30ChartCache.data != "" {
			return b30ChartCache.data, nil
		}
		return "", fmt.Errorf("读取难度表失败: %w", err)
	}

	b30ChartCache.data = string(body)
	b30ChartCache.expires = time.Now().Add(1 * time.Hour)
	return b30ChartCache.data, nil
}

func parseB30Chart(csv string) map[int]*B30ChartEntry {
	result := make(map[int]*B30ChartEntry)
	lines := strings.Split(csv, "\n")
	for i, line := range lines {
		if i == 0 || strings.TrimSpace(line) == "" {
			continue
		}
		fields := strings.SplitN(line, ",", 8)
		if len(fields) < 7 {
			continue
		}

		constant, err := strconv.ParseFloat(strings.TrimSpace(fields[2]), 64)
		if err != nil || constant <= 0 {
			continue
		}
		songID, err := strconv.Atoi(strings.TrimSpace(fields[6]))
		if err != nil || songID <= 0 {
			continue
		}

		diff := strings.TrimSpace(fields[5])
		diffNum := diffNameToNum(diff)
		if diffNum < 0 {
			continue
		}

		levelStr := strings.TrimSpace(fields[3])
		level := parseLevel(levelStr)
		if level <= 0 {
			continue
		}

		key := songID*10 + diffNum
		if existing, ok := result[key]; !ok || constant > existing.Constant {
			result[key] = &B30ChartEntry{SongID: songID, Diff: diff, Level: level, Constant: constant}
		}
	}
	return result
}

func parseLevel(s string) float64 {
	s = strings.TrimSpace(s)
	if s == "" {
		return 0
	}
	s = strings.TrimRight(s, "+-")
	parts := strings.Fields(s)
	if len(parts) > 1 {
		s = parts[len(parts)-1]
	}
	level, err := strconv.ParseFloat(s, 64)
	if err != nil {
		return 0
	}
	return level
}

// ===== rating calculation (lunabot style) =====

func resultTypeRank(playResult string, fullCombo, fullPerfect bool) int {
	if fullPerfect {
		return 3 // AP
	}
	if fullCombo {
		return 2 // FC
	}
	if playResult != "not_clear" {
		return 1 // Clear
	}
	return 0 // Not Clear
}

func calcRatingForResult(resultType int, constant float64, level float64) float64 {
	switch resultType {
	case 3: // AP
		return constant
	case 2: // FC
		if level >= 33 {
			return constant - 1.0
		}
		return constant - 1.5
	default:
		return 0
	}
}

// ===== fetch data =====

func loadSuiteBaseURL() string {
	return strings.TrimRight(strings.TrimSpace(os.Getenv("PJSK_SUITE_BASEURL")), "/")
}

func fetchSuiteMusicResults(suiteBaseURL, server, userID string) ([]suiteMusicResult, string, string, error) {
	endpoint := suiteBaseURL + "/public/" + server + "/suite/" + userID
	req, err := http.NewRequest(http.MethodGet, endpoint, nil)
	if err != nil {
		return nil, "", "", fmt.Errorf("创建 suite-api 请求失败: %w", err)
	}
	req.Header.Set("User-Agent", "amiabot-pages/1.0")
	req.Header.Set("Accept", "application/json")

	resp, err := pjskB30HTTPClient.Do(req)
	if err != nil {
		return nil, "", "", fmt.Errorf("请求 suite-api 失败: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		body, _ := io.ReadAll(resp.Body)
		return nil, "", "", fmt.Errorf("suite-api 返回 %d: %s", resp.StatusCode, shortenErrBody(body))
	}

	var suiteData struct {
		UserMusicResults []suiteMusicResult `json:"userMusicResults"`
		UploadTime       int64              `json:"upload_time"`
		UserProfile      struct {
			UserId json.Number `json:"userId"`
			Name   string      `json:"name"`
		} `json:"userProfile"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&suiteData); err != nil {
		return nil, "", "", fmt.Errorf("解析 suite-api 响应失败: %w", err)
	}

	if len(suiteData.UserMusicResults) == 0 {
		return nil, "", "", fmt.Errorf("suite-api 未返回 userMusicResults 数据")
	}

	var name string
	if suiteData.UserProfile.Name != "" {
		name = strings.TrimSpace(suiteData.UserProfile.Name)
	} else {
		if profileCfg, err := loadPJSKProfileAPIConfigFromEnv(); err == nil {
			name = fetchB30PlayerName(profileCfg, server, userID)
		}
	}
	if name == "" {
		userIDStr := suiteData.UserProfile.UserId.String()
		if userIDStr != "" {
			name = "玩家 " + userIDStr
		} else {
			name = "玩家 " + userID
		}
	}

	var uploadTime string
	if suiteData.UploadTime > 0 {
		uploadTime = formatPJSKTimestamp(suiteData.UploadTime)
	}

	return suiteData.UserMusicResults, name, uploadTime, nil
}

func fetchB30PlayerName(cfg *pjskProfileAPIConfig, server, userID string) string {
	if cfg == nil || cfg.BaseURL == "" {
		return ""
	}
	endpoint := cfg.BaseURL + "/" + server + "/" + userID + "/profile"
	req, err := http.NewRequest(http.MethodGet, endpoint, nil)
	if err != nil {
		return ""
	}
	req.Header.Set("User-Agent", "amiabot-pages/1.0")
	req.Header.Set("Accept", "application/json")
	for k, v := range cfg.Headers {
		req.Header.Set(k, v)
	}
	resp, err := pjskB30HTTPClient.Do(req)
	if err != nil {
		return ""
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return ""
	}
	var result struct {
		User struct {
			Name string `json:"name"`
		} `json:"user"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return ""
	}
	return strings.TrimSpace(result.User.Name)
}

// ===== music index (read musics.json once) =====

func buildMusicIndex(server string) map[int]*pjskMusic {
	data, err := ReadCachedJSON(server, "musics.json")
	if err != nil {
		return nil
	}
	var musics []pjskMusic
	if err := json.Unmarshal(data, &musics); err != nil {
		return nil
	}
	idx := make(map[int]*pjskMusic, len(musics))
	for i := range musics {
		idx[musics[i].ID] = &musics[i]
	}
	return idx
}

// ===== handler =====

func B30Handler(c *gin.Context) {
	server := c.DefaultQuery("server", "jp")
	if !validServers[server] {
		renderB30Err(c, "无效的服务器参数，支持: jp, cn, en, tw, kr")
		return
	}

	userID := strings.TrimSpace(c.Query("id"))
	if userID == "" {
		renderB30Err(c, "缺少玩家 ID 参数")
		return
	}
	if !isDigits(userID) {
		renderB30Err(c, "无效的玩家 ID: "+userID)
		return
	}

	suiteBaseURL := loadSuiteBaseURL()
	if suiteBaseURL == "" {
		renderB30Err(c, "未配置 PJSK_SUITE_BASEURL 环境变量")
		return
	}

	// 1. 获取难度表
	chartCSV, err := getB30ChartCSV()
	if err != nil {
		renderB30Err(c, "加载难度表失败: "+err.Error())
		return
	}
	chartMap := parseB30Chart(chartCSV)

	// 2. 获取音乐成绩
	musicResults, name, uploadTime, err := fetchSuiteMusicResults(suiteBaseURL, server, userID)
	if err != nil {
		renderB30Err(c, err.Error())
		return
	}

	// 3. 按 (musicId, difficulty) 取最佳成绩 (AP > FC > Clear > NotClear)
	type bestEntry struct {
		songID     int
		diffNum    int
		diffLabel  string
		level      float64
		constant   float64
		resultType int
		resultStr  string
	}

	bestMap := make(map[int]bestEntry)

	for _, r := range musicResults {
		songID := r.MusicID
		diffNum := diffNameToNum(strings.ToLower(r.MusicDifficulty))
		if songID <= 0 || diffNum < 0 {
			continue
		}

		chart, ok := chartMap[songID*10+diffNum]
		if !ok {
			continue
		}

		rt := resultTypeRank(r.PlayResult, r.FullComboFlg, r.FullPerfectFlg)
		if rt == 0 {
			continue
		}

		key := songID*10 + diffNum
		if existing, exists := bestMap[key]; !exists || rt > existing.resultType {
			var resultStr string
			switch rt {
			case 3:
				resultStr = "ap"
			case 2:
				resultStr = "fc"
			default:
				resultStr = "clear"
			}

			bestMap[key] = bestEntry{
				songID:     songID,
				diffNum:    diffNum,
				diffLabel:  diffNumToLabel(diffNum),
				level:      chart.Level,
				constant:   chart.Constant,
				resultType: rt,
				resultStr:  resultStr,
			}
		}
	}

	// 4. 计算每条 rating 并排序
	type scoredEntry struct {
		bestEntry
		rating float64
	}

	var scored []scoredEntry
	for _, entry := range bestMap {
		rating := calcRatingForResult(entry.resultType, entry.constant, entry.level)
		if rating <= 0 {
			continue
		}
		scored = append(scored, scoredEntry{bestEntry: entry, rating: rating})
	}

	sort.Slice(scored, func(i, j int) bool {
		return scored[i].rating > scored[j].rating
	})
	if len(scored) > 30 {
		scored = scored[:30]
	}

	sumRating := 0.0
	for _, s := range scored {
		sumRating += s.rating
	}

	serverName := serverNames[server]
	if serverName == "" {
		serverName = strings.ToUpper(server)
	}

	// 5. 一次性读取 musics.json，构建 title + cover 索引
	musicIdx := buildMusicIndex(server)

	// 6. 转换为模板视图
	views := make([]B30ScoreEntry, len(scored))
	for i, s := range scored {
		resultIcon := pjskB30ResultIcon(s.resultStr)

		constantText := strconv.FormatFloat(s.constant, 'f', -1, 64)

		var cover htmltemplate.URL
		var songName string
		if m := musicIdx[s.songID]; m != nil {
			songName = m.Title
			cover = downloadAssetByLabel(server, "music:jacket:"+m.AssetbundleName)
		}

		views[i] = B30ScoreEntry{
			Order:      i + 1,
			Cover:      cover,
			Name:       truncateStr(songName, 20),
			SongID:     "#" + strconv.Itoa(s.songID),
			Diff:       s.diffLabel,
			Level:      levelLabel(s.level),
			ResultType: s.resultStr,
			ResultIcon: resultIcon,
			Constant:   constantText,
			DiffStyle:  diffNumToBGStyleHTML(s.diffNum),
		}
	}

	page := B30PageData{
		Name:           name,
		UserID:         userID,
		Server:         serverName,
		ServerKey:      server,
		UserRating:     formatB30(sumRating / 30.0),
		Count:          len(views),
		Scores:         views,
		ChartUpdatedAt: b30ChartUpdateText(),
		UpdatedTime:    uploadTime,
	}

	c.HTML(http.StatusOK, "pjsk/b30", gin.H{"B30": page})
}

func renderB30Err(c *gin.Context, errMsg string) {
	c.HTML(http.StatusOK, "pjsk/b30", gin.H{"Error": errMsg})
}

func levelLabel(level float64) string {
	return strings.TrimRight(strings.TrimRight(fmt.Sprintf("%.0f", level), "0"), ".")
}

func b30ChartUpdateText() string {
	b30ChartCache.mu.RLock()
	defer b30ChartCache.mu.RUnlock()
	if b30ChartCache.data == "" {
		return ""
	}
	return ""
}

// ===== result icons =====

const (
	pjskB30APIconURL = "https://raw.githubusercontent.com/watagashi-uni/Unibot/refs/heads/main/pics/AllPerfect.png"
	pjskB30FCIconURL = "https://raw.githubusercontent.com/watagashi-uni/Unibot/refs/heads/main/pics/FullCombo.png"
)

func pjskB30ResultIcon(resultType string) htmltemplate.URL {
	switch resultType {
	case "ap":
		return imgcache.Default.Download(pjskB30APIconURL, -1, nil)
	case "fc":
		return imgcache.Default.Download(pjskB30FCIconURL, -1, nil)
	default:
		return ""
	}
}
