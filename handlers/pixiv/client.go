package pixiv

import (
	"crypto/md5"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/gin-gonic/gin"
)

const (
	pixivHashSalt      = "28c1fdd170a5204386cb1313c7077b34f83e4aaf4aa829ce78c231e05b0bae2c"
	pixivUserAgent     = "PixivAndroidApp/5.0.166 (Android 10.0; Pixel C)"
	pixivImageAgent    = "PixivIOSApp/5.8.0"
	pixivAcceptLang    = "zh-CN"
	pixivAppOS         = "Android"
	pixivAppOSVersion  = "Android 10.0"
	pixivAppVersion    = "5.0.166"
	pixivClientID      = "MOBrBDS8blbauoSck0ZfDbtuzpyT"
	pixivClientSecret  = "lsACyCD94FhDUtGTXi3QzcFE2uU1hqtDaKeqrdwj"
	pixivTokenErrorMsg = "缺少 Pixiv token，请在请求头传 Authorization: Bearer <access_token>，或配置 PIXIV_ACCESS_TOKEN / PIXIV_REFRESH_TOKEN"
)

var (
	pixivHTTPClient    = &http.Client{Timeout: 15 * time.Second}
	pixivAppAPIBaseURL = "https://app-api.pixiv.net"
	pixivOAuthBaseURL  = "https://oauth.secure.pixiv.net"
)

type pixivTokenSource string

const (
	pixivTokenSourceNone         pixivTokenSource = "none"
	pixivTokenSourceHeader       pixivTokenSource = "header"
	pixivTokenSourceEnvAccess    pixivTokenSource = "env_access"
	pixivTokenSourceRefreshCache pixivTokenSource = "refresh_cache"
	pixivTokenSourceRefresh      pixivTokenSource = "refresh"
)

type pixivTokenCache struct {
	mu          sync.Mutex
	accessToken string
	expiresAt   time.Time
}

var cachedPixivToken pixivTokenCache

type pixivErrorResponse struct {
	Error pixivErrorDetail `json:"error"`
}

type pixivErrorDetail struct {
	UserMessage string `json:"user_message"`
	Message     string `json:"message"`
	Reason      string `json:"reason"`
}

type pixivAPIError struct {
	StatusCode int
	Message    string
	OAuth      bool
}

func (e *pixivAPIError) Error() string {
	if e.Message != "" {
		return e.Message
	}
	return fmt.Sprintf("Pixiv API 返回异常状态码: %d", e.StatusCode)
}

type pixivOAuthResponse struct {
	Response pixivOAuthToken `json:"response"`
}

type pixivOAuthToken struct {
	AccessToken  string `json:"access_token"`
	ExpiresIn    int    `json:"expires_in"`
	RefreshToken string `json:"refresh_token"`
}

type pixivIllustDetailResponse struct {
	Illust pixivIllust `json:"illust"`
}

type pixivIllust struct {
	ID             int                   `json:"id"`
	Title          string                `json:"title"`
	Type           string                `json:"type"`
	ImageURLs      pixivImageURLs        `json:"image_urls"`
	Caption        string                `json:"caption"`
	CreateDate     string                `json:"create_date"`
	PageCount      int                   `json:"page_count"`
	Width          int                   `json:"width"`
	Height         int                   `json:"height"`
	User           pixivUser             `json:"user"`
	Tags           []pixivTag            `json:"tags"`
	MetaSinglePage pixivMetaSinglePage   `json:"meta_single_page"`
	MetaPages      []pixivMetaPage       `json:"meta_pages"`
	TotalView      int                   `json:"total_view"`
	TotalBookmarks int                   `json:"total_bookmarks"`
	TotalComments  int                   `json:"total_comments"`
	IllustAIType   int                   `json:"illust_ai_type"`
	Series         *pixivIllustSeriesRef `json:"series"`
}

type pixivIllustSeriesRef struct {
	ID    int    `json:"id"`
	Title string `json:"title"`
}

type pixivImageURLs struct {
	SquareMedium string `json:"square_medium"`
	Medium       string `json:"medium"`
	Large        string `json:"large"`
}

type pixivMetaSinglePage struct {
	OriginalImageURL string `json:"original_image_url"`
}

type pixivMetaPage struct {
	ImageURLs pixivMetaPageImageURLs `json:"image_urls"`
}

type pixivMetaPageImageURLs struct {
	SquareMedium string `json:"square_medium"`
	Medium       string `json:"medium"`
	Large        string `json:"large"`
	Original     string `json:"original"`
}

type pixivUser struct {
	ID               int                   `json:"id"`
	Name             string                `json:"name"`
	Account          string                `json:"account"`
	ProfileImageURLs pixivProfileImageURLs `json:"profile_image_urls"`
}

type pixivProfileImageURLs struct {
	Medium string `json:"medium"`
}

type pixivTag struct {
	Name           string `json:"name"`
	TranslatedName string `json:"translated_name"`
}

func md5Hex(s string) string {
	sum := md5.Sum([]byte(s))
	return hex.EncodeToString(sum[:])
}

func pixivClientTime() string {
	return time.Now().UTC().Format("2006-01-02T15:04:05+00:00")
}

func applyPixivCommonHeaders(req *http.Request) {
	clientTime := pixivClientTime()
	req.Header.Set("X-Client-Time", clientTime)
	req.Header.Set("X-Client-Hash", md5Hex(clientTime+pixivHashSalt))
	req.Header.Set("User-Agent", pixivUserAgent)
	req.Header.Set("Accept-Language", pixivAcceptLang)
	req.Header.Set("App-OS", pixivAppOS)
	req.Header.Set("App-OS-Version", pixivAppOSVersion)
	req.Header.Set("App-Version", pixivAppVersion)
}

func pixivImageHeaders() map[string]string {
	return map[string]string{
		"Referer":    "https://app-api.pixiv.net/",
		"User-Agent": pixivImageAgent,
	}
}

func resolvePixivHeaderAccessToken(c *gin.Context) string {
	authHeader := strings.TrimSpace(c.GetHeader("Authorization"))
	if authHeader == "" {
		return ""
	}

	const bearerPrefix = "Bearer "
	if len(authHeader) >= len(bearerPrefix) && strings.EqualFold(authHeader[:len(bearerPrefix)], bearerPrefix) {
		return strings.TrimSpace(authHeader[len(bearerPrefix):])
	}

	if !strings.ContainsRune(authHeader, ' ') {
		return authHeader
	}

	return ""
}

func loadCachedPixivAccessToken() string {
	cachedPixivToken.mu.Lock()
	defer cachedPixivToken.mu.Unlock()

	if cachedPixivToken.accessToken == "" {
		return ""
	}
	if !cachedPixivToken.expiresAt.IsZero() && time.Now().After(cachedPixivToken.expiresAt) {
		cachedPixivToken.accessToken = ""
		cachedPixivToken.expiresAt = time.Time{}
		return ""
	}
	return cachedPixivToken.accessToken
}

func cachePixivAccessToken(token string, expiresIn int) {
	cachedPixivToken.mu.Lock()
	defer cachedPixivToken.mu.Unlock()

	expiresAfter := time.Duration(expiresIn) * time.Second
	if expiresAfter <= 0 {
		expiresAfter = time.Hour
	}
	skew := time.Minute
	if expiresAfter <= 2*time.Minute {
		skew = 10 * time.Second
	}
	if expiresAfter <= skew {
		skew = 0
	}

	cachedPixivToken.accessToken = strings.TrimSpace(token)
	cachedPixivToken.expiresAt = time.Now().Add(expiresAfter - skew)
}

func clearPixivAccessTokenCache() {
	cachedPixivToken.mu.Lock()
	defer cachedPixivToken.mu.Unlock()
	cachedPixivToken.accessToken = ""
	cachedPixivToken.expiresAt = time.Time{}
}

func resolvePixivAccessToken(c *gin.Context) (string, pixivTokenSource, error) {
	if token := resolvePixivHeaderAccessToken(c); token != "" {
		return token, pixivTokenSourceHeader, nil
	}

	if token := loadCachedPixivAccessToken(); token != "" {
		return token, pixivTokenSourceRefreshCache, nil
	}

	if token := strings.TrimSpace(os.Getenv("PIXIV_ACCESS_TOKEN")); token != "" {
		return token, pixivTokenSourceEnvAccess, nil
	}

	refreshToken := strings.TrimSpace(os.Getenv("PIXIV_REFRESH_TOKEN"))
	if refreshToken == "" {
		return "", pixivTokenSourceNone, fmt.Errorf(pixivTokenErrorMsg)
	}

	refreshed, err := refreshPixivAccessToken(refreshToken)
	if err != nil {
		return "", pixivTokenSourceNone, err
	}
	return refreshed.AccessToken, pixivTokenSourceRefresh, nil
}

func refreshPixivAccessToken(refreshToken string) (*pixivOAuthToken, error) {
	refreshToken = strings.TrimSpace(refreshToken)
	if refreshToken == "" {
		return nil, fmt.Errorf(pixivTokenErrorMsg)
	}

	form := url.Values{}
	form.Set("client_id", pixivClientID)
	form.Set("client_secret", pixivClientSecret)
	form.Set("grant_type", "refresh_token")
	form.Set("refresh_token", refreshToken)
	form.Set("include_policy", "true")

	req, err := http.NewRequest(http.MethodPost, pixivOAuthBaseURL+"/auth/token", strings.NewReader(form.Encode()))
	if err != nil {
		return nil, fmt.Errorf("创建 Pixiv OAuth 请求失败: %w", err)
	}
	applyPixivCommonHeaders(req)
	req.Header.Set("Content-Type", "application/x-www-form-urlencoded")

	resp, err := pixivHTTPClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("刷新 Pixiv access token 失败: %w", err)
	}
	defer resp.Body.Close()

	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, fmt.Errorf("读取 Pixiv OAuth 响应失败: %w", err)
	}

	if resp.StatusCode != http.StatusOK {
		return nil, parsePixivAPIError(resp.StatusCode, body)
	}

	var payload pixivOAuthResponse
	if err := json.Unmarshal(body, &payload); err != nil {
		return nil, fmt.Errorf("解析 Pixiv OAuth 响应失败: %w", err)
	}
	if strings.TrimSpace(payload.Response.AccessToken) == "" {
		return nil, fmt.Errorf("Pixiv OAuth 未返回 access token")
	}

	cachePixivAccessToken(payload.Response.AccessToken, payload.Response.ExpiresIn)
	return &payload.Response, nil
}

func parsePixivAPIError(statusCode int, body []byte) error {
	var payload pixivErrorResponse
	if err := json.Unmarshal(body, &payload); err == nil {
		msg := strings.TrimSpace(payload.Error.UserMessage)
		if msg == "" {
			msg = strings.TrimSpace(payload.Error.Message)
		}
		if msg == "" {
			msg = strings.TrimSpace(payload.Error.Reason)
		}
		if msg != "" {
			return &pixivAPIError{
				StatusCode: statusCode,
				Message:    "Pixiv API 错误: " + msg,
				OAuth:      strings.Contains(strings.ToLower(payload.Error.Message), "oauth"),
			}
		}
	}

	trimmedBody := strings.TrimSpace(string(body))
	if len(trimmedBody) > 200 {
		trimmedBody = trimmedBody[:200] + "..."
	}
	msg := fmt.Sprintf("Pixiv API 返回异常状态码: %d", statusCode)
	if trimmedBody != "" {
		msg += " " + trimmedBody
	}
	return &pixivAPIError{StatusCode: statusCode, Message: msg}
}

func fetchPixivIllustDetail(accessToken string, pid int) (*pixivIllust, error) {
	if strings.TrimSpace(accessToken) == "" {
		return nil, fmt.Errorf(pixivTokenErrorMsg)
	}

	detailURL := pixivAppAPIBaseURL + "/v1/illust/detail?filter=for_android&illust_id=" + strconv.Itoa(pid)
	req, err := http.NewRequest(http.MethodGet, detailURL, nil)
	if err != nil {
		return nil, fmt.Errorf("创建 Pixiv 详情请求失败: %w", err)
	}
	applyPixivCommonHeaders(req)
	req.Header.Set("Authorization", "Bearer "+accessToken)

	resp, err := pixivHTTPClient.Do(req)
	if err != nil {
		return nil, fmt.Errorf("请求 Pixiv 插画详情失败: %w", err)
	}
	defer resp.Body.Close()

	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, fmt.Errorf("读取 Pixiv 插画详情失败: %w", err)
	}

	if resp.StatusCode != http.StatusOK {
		return nil, parsePixivAPIError(resp.StatusCode, body)
	}

	var payload pixivIllustDetailResponse
	if err := json.Unmarshal(body, &payload); err != nil {
		return nil, fmt.Errorf("解析 Pixiv 插画详情失败: %w", err)
	}
	if payload.Illust.ID <= 0 {
		return nil, fmt.Errorf("未找到插画 PID: %d", pid)
	}
	return &payload.Illust, nil
}

func getPixivIllustDetail(c *gin.Context, pid int) (*pixivIllust, error) {
	accessToken, source, err := resolvePixivAccessToken(c)
	if err != nil {
		return nil, err
	}

	illust, err := fetchPixivIllustDetail(accessToken, pid)
	if err == nil {
		return illust, nil
	}

	apiErr, ok := err.(*pixivAPIError)
	if !ok || !apiErr.OAuth || source == pixivTokenSourceHeader {
		return nil, err
	}

	refreshToken := strings.TrimSpace(os.Getenv("PIXIV_REFRESH_TOKEN"))
	if refreshToken == "" {
		return nil, err
	}

	clearPixivAccessTokenCache()
	refreshed, refreshErr := refreshPixivAccessToken(refreshToken)
	if refreshErr != nil {
		return nil, refreshErr
	}

	return fetchPixivIllustDetail(refreshed.AccessToken, pid)
}
