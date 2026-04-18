package gallery

import (
	"context"
	"encoding/json"
	"fmt"
	htmltemplate "html/template"
	"io"
	"net/http"
	"net/url"
	"os"
	"strconv"
	"strings"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/xiaocaoooo/amiabot-pages/pkg/imgcache"
)

var (
	galleryImageDownloader = func(imageURL string, ttl time.Duration, headers map[string]string) htmltemplate.URL {
		return imgcache.Default.Download(strings.TrimSpace(imageURL), ttl, headers)
	}
	galleryHTTPClient          = &http.Client{Timeout: 30 * time.Second}
	galleryTagListLimit        = 500
	galleryImageListPageSize   = 100
	galleryTagPreviewWorkers   = 8
	galleryListPreviewWidth    = 320
	galleryListPreviewHeight   = 320
	galleryTagPreviewWidth     = 360
	galleryTagPreviewHeight    = 240
	galleryDuplicateImageWidth = 720
)

type galleryTag struct {
	ID        int64     `json:"id"`
	Name      string    `json:"name"`
	CreatedAt time.Time `json:"created_at"`
}

type galleryImage struct {
	ID          int64     `json:"id"`
	Filename    string    `json:"filename"`
	FID         string    `json:"fid"`
	FileSize    int64     `json:"file_size"`
	Width       int       `json:"width"`
	Height      int       `json:"height"`
	MimeType    string    `json:"mime_type"`
	PHash       int64     `json:"phash"`
	IsAnimated  bool      `json:"is_animated"`
	Description string    `json:"description"`
	CreatedAt   time.Time `json:"created_at"`
}

type galleryImageWithTags struct {
	galleryImage
	Tags []galleryTag `json:"tags"`
}

type galleryTagsResponse struct {
	Items []galleryTag `json:"items"`
}

type galleryImageListResponse struct {
	Items    []galleryImageWithTags `json:"items"`
	Page     int                    `json:"page"`
	PageSize int                    `json:"page_size"`
	Total    int64                  `json:"total"`
}

func fetchGalleryTags(ctx context.Context, q string, limit int) ([]galleryTag, error) {
	if limit <= 0 {
		limit = galleryTagListLimit
	}
	payload, err := callGalleryJSON[galleryTagsResponse](ctx, "/v1/tags", map[string]string{
		"q":     strings.TrimSpace(q),
		"limit": strconv.Itoa(limit),
	})
	if err != nil {
		return nil, err
	}
	return payload.Items, nil
}

func fetchGalleryImagesPage(ctx context.Context, tags []string, page int, pageSize int) (galleryImageListResponse, error) {
	if page <= 0 {
		page = 1
	}
	if pageSize <= 0 {
		pageSize = galleryImageListPageSize
	}
	params := map[string]string{
		"page":      strconv.Itoa(page),
		"page_size": strconv.Itoa(pageSize),
	}
	if len(tags) > 0 {
		params["tags"] = strings.Join(tags, ",")
	}
	return callGalleryJSON[galleryImageListResponse](ctx, "/v1/images", params)
}

func fetchAllGalleryImages(ctx context.Context, tags []string) ([]galleryImageWithTags, error) {
	page := 1
	items := make([]galleryImageWithTags, 0)
	for {
		payload, err := fetchGalleryImagesPage(ctx, tags, page, galleryImageListPageSize)
		if err != nil {
			return nil, err
		}
		items = append(items, payload.Items...)
		if payload.Total == 0 || len(payload.Items) == 0 {
			break
		}
		if int64(len(items)) >= payload.Total {
			break
		}
		page++
	}
	return items, nil
}

func buildGalleryPreviewURL(image *galleryImageWithTags, width int, height int) string {
	if image == nil {
		return ""
	}
	return buildGalleryRenderURL(image.ID)
}

func buildGalleryRenderURL(imageID int64) string {
	base := normalizeHTTPBase(firstNonEmptyEnv("GALLERY_PAGES_RENDER_BASE", "GALLERY_SERVER"))
	if base == "" {
		return ""
	}
	parsed, err := url.Parse(base)
	if err != nil {
		return ""
	}
	parsed.Path = strings.TrimRight(parsed.Path, "/") + "/v1/images/" + url.PathEscape(strconv.FormatInt(imageID, 10)) + "/render"
	parsed.RawQuery = ""
	return parsed.String()
}

func callGalleryJSON[T any](ctx context.Context, path string, params map[string]string) (T, error) {
	var zero T
	req, err := newGalleryAPIRequest(ctx, path, params)
	if err != nil {
		return zero, err
	}
	resp, err := galleryHTTPClient.Do(req)
	if err != nil {
		return zero, fmt.Errorf("请求 gallery 服务失败: %w", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		return zero, decodeGalleryAPIError(resp)
	}
	var payload T
	if err := json.NewDecoder(resp.Body).Decode(&payload); err != nil {
		return zero, fmt.Errorf("解析 gallery 响应失败: %w", err)
	}
	return payload, nil
}

func newGalleryAPIRequest(ctx context.Context, path string, params map[string]string) (*http.Request, error) {
	base := normalizeHTTPBase(os.Getenv("GALLERY_SERVER"))
	if base == "" {
		return nil, fmt.Errorf("未配置 GALLERY_SERVER")
	}
	parsed, err := url.Parse(base)
	if err != nil {
		return nil, fmt.Errorf("GALLERY_SERVER 无效: %w", err)
	}
	parsed.Path = strings.TrimRight(parsed.Path, "/") + path
	query := parsed.Query()
	for key, value := range params {
		trimmed := strings.TrimSpace(value)
		if trimmed == "" {
			continue
		}
		query.Set(key, trimmed)
	}
	parsed.RawQuery = query.Encode()
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, parsed.String(), nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("Accept", "application/json")
	if token := strings.TrimSpace(firstNonEmptyEnv("GALLERY_READ_TOKEN", "GALLERY_WRITE_TOKEN")); token != "" {
		req.Header.Set("Authorization", "Bearer "+token)
	}
	return req, nil
}

func decodeGalleryAPIError(resp *http.Response) error {
	if resp == nil {
		return fmt.Errorf("gallery 服务响应为空")
	}
	body, _ := io.ReadAll(io.LimitReader(resp.Body, 64*1024))
	var payload struct {
		Error string `json:"error"`
	}
	_ = json.Unmarshal(body, &payload)
	message := strings.TrimSpace(payload.Error)
	if message == "" {
		message = strings.TrimSpace(string(body))
	}
	if message == "" {
		message = fmt.Sprintf("gallery 服务返回 HTTP %d", resp.StatusCode)
	}
	return fmt.Errorf("%s", message)
}

func splitGalleryTags(raw string) []string {
	parts := strings.Split(strings.TrimSpace(raw), ",")
	result := make([]string, 0, len(parts))
	seen := make(map[string]struct{}, len(parts))
	for _, part := range parts {
		trimmed := strings.TrimSpace(part)
		if trimmed == "" {
			continue
		}
		lowered := strings.ToLower(trimmed)
		if _, ok := seen[lowered]; ok {
			continue
		}
		seen[lowered] = struct{}{}
		result = append(result, trimmed)
	}
	return result
}

func galleryTagNames(tags []galleryTag) []string {
	result := make([]string, 0, len(tags))
	for _, tag := range tags {
		if trimmed := strings.TrimSpace(tag.Name); trimmed != "" {
			result = append(result, trimmed)
		}
	}
	return result
}

func formatGalleryDimensions(image galleryImageWithTags) string {
	if image.Width <= 0 || image.Height <= 0 {
		return "未知尺寸"
	}
	return fmt.Sprintf("%d×%d", image.Width, image.Height)
}

func formatGalleryCreatedAt(image galleryImageWithTags) string {
	if image.CreatedAt.IsZero() {
		return ""
	}
	return image.CreatedAt.Local().Format("2006-01-02 15:04")
}

func firstNonEmptyEnv(keys ...string) string {
	for _, key := range keys {
		if value := strings.TrimSpace(os.Getenv(key)); value != "" {
			return value
		}
	}
	return ""
}

func normalizeHTTPBase(hostOrURL string) string {
	hostOrURL = strings.TrimSpace(hostOrURL)
	if hostOrURL == "" {
		return ""
	}
	if strings.HasPrefix(hostOrURL, "http://") || strings.HasPrefix(hostOrURL, "https://") {
		return strings.TrimRight(hostOrURL, "/")
	}
	return "http://" + strings.TrimRight(hostOrURL, "/")
}

func renderGalleryError(c *gin.Context, templateName string, errMsg string) {
	c.HTML(http.StatusOK, templateName, gin.H{"Error": strings.TrimSpace(errMsg)})
}
