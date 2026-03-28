package pixiv

import (
	"archive/zip"
	"bytes"
	"context"
	"fmt"
	"image"
	"image/color/palette"
	"image/draw"
	"image/gif"
	_ "image/jpeg"
	_ "image/png"
	"io"
	"net/http"
	"net/url"
	"path"
	"strconv"
	"strings"
	"time"

	"github.com/gin-gonic/gin"
)

const (
	pixivBinaryCacheControl = "public, max-age=86400"
	pixivUgoiraZipMaxBytes  = 64 << 20
)

type pixivMediaManifestResponse struct {
	PID       int                      `json:"pid"`
	Title     string                   `json:"title"`
	Type      string                   `json:"type"`
	PageCount int                      `json:"page_count"`
	Items     []pixivMediaItemResponse `json:"items"`
}

type pixivMediaItemResponse struct {
	Index int    `json:"index"`
	Kind  string `json:"kind"`
	Path  string `json:"path"`
}

func IllustMediaHandler(c *gin.Context) {
	pid, err := parsePixivPIDParam(c)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	illust, err := getPixivIllustDetail(c, pid)
	if err != nil {
		c.JSON(http.StatusBadGateway, gin.H{"error": err.Error()})
		return
	}

	manifest, err := buildPixivMediaManifest(illust)
	if err != nil {
		c.JSON(http.StatusBadGateway, gin.H{"error": err.Error()})
		return
	}
	c.JSON(http.StatusOK, manifest)
}

func PixivImageProxyHandler(c *gin.Context) {
	rawURL := strings.TrimSpace(c.Query("url"))
	parsedURL, err := validatePixivImageURL(rawURL)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	resp, err := openPixivBinaryResponse(c.Request.Context(), parsedURL.String())
	if err != nil {
		c.JSON(http.StatusBadGateway, gin.H{"error": err.Error()})
		return
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		_, _ = io.Copy(io.Discard, io.LimitReader(resp.Body, 64*1024))
		c.JSON(http.StatusBadGateway, gin.H{"error": fmt.Sprintf("Pixiv 图片下载失败: HTTP %d", resp.StatusCode)})
		return
	}

	contentType := strings.TrimSpace(resp.Header.Get("Content-Type"))
	if contentType == "" {
		contentType = "application/octet-stream"
	}

	extraHeaders := map[string]string{
		"Cache-Control": pixivBinaryCacheControl,
	}
	filename := path.Base(parsedURL.Path)
	if filename != "" && filename != "." && filename != "/" {
		extraHeaders["Content-Disposition"] = fmt.Sprintf("inline; filename=%q", filename)
	}

	c.DataFromReader(http.StatusOK, resp.ContentLength, contentType, resp.Body, extraHeaders)
}

func PixivUgoiraGIFHandler(c *gin.Context) {
	pid, err := parsePixivPIDParam(c)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
		return
	}

	metadata, err := getPixivUgoiraMetadata(c, pid)
	if err != nil {
		c.JSON(http.StatusBadGateway, gin.H{"error": err.Error()})
		return
	}

	zipURL := strings.TrimSpace(metadata.ZipURLs.Medium)
	if zipURL == "" {
		c.JSON(http.StatusBadGateway, gin.H{"error": "Pixiv 动图未返回可用 zip 资源"})
		return
	}

	zipData, err := downloadPixivBinary(c.Request.Context(), zipURL, pixivUgoiraZipMaxBytes)
	if err != nil {
		c.JSON(http.StatusBadGateway, gin.H{"error": err.Error()})
		return
	}

	gifData, err := convertPixivUgoiraToGIF(zipData, metadata.Frames)
	if err != nil {
		c.JSON(http.StatusBadGateway, gin.H{"error": err.Error()})
		return
	}

	c.Header("Cache-Control", pixivBinaryCacheControl)
	c.Header("Content-Disposition", fmt.Sprintf("inline; filename=%q", fmt.Sprintf("pixiv-ugoira-%d.gif", pid)))
	c.Data(http.StatusOK, "image/gif", gifData)
}

func parsePixivPIDParam(c *gin.Context) (int, error) {
	pidStr := strings.TrimSpace(c.Query("pid"))
	if pidStr == "" {
		return 0, fmt.Errorf("缺少插画 PID 参数")
	}
	pid, err := strconv.Atoi(pidStr)
	if err != nil || pid <= 0 {
		return 0, fmt.Errorf("无效的插画 PID: %s", pidStr)
	}
	return pid, nil
}

func buildPixivMediaManifest(illust *pixivIllust) (pixivMediaManifestResponse, error) {
	if illust == nil || illust.ID <= 0 {
		return pixivMediaManifestResponse{}, fmt.Errorf("Pixiv 插画信息为空")
	}

	manifest := pixivMediaManifestResponse{
		PID:       illust.ID,
		Title:     illust.Title,
		Type:      illust.Type,
		PageCount: illust.PageCount,
	}

	if strings.EqualFold(strings.TrimSpace(illust.Type), "ugoira") {
		manifest.Items = []pixivMediaItemResponse{{
			Index: 0,
			Kind:  "gif",
			Path:  buildPixivUgoiraGIFPath(illust.ID),
		}}
		if manifest.PageCount <= 0 {
			manifest.PageCount = 1
		}
		return manifest, nil
	}

	originalURLs := extractPixivOriginalImageURLs(illust)
	items := make([]pixivMediaItemResponse, 0, len(originalURLs))
	for index, rawURL := range originalURLs {
		proxyPath := buildPixivImageProxyPath(rawURL)
		if proxyPath == "" {
			continue
		}
		items = append(items, pixivMediaItemResponse{
			Index: index,
			Kind:  "image",
			Path:  proxyPath,
		})
	}
	manifest.Items = items
	if manifest.PageCount <= 0 && len(items) > 0 {
		manifest.PageCount = len(items)
	}
	return manifest, nil
}

func extractPixivOriginalImageURLs(illust *pixivIllust) []string {
	if illust == nil {
		return nil
	}

	if illust.PageCount > 1 {
		urls := make([]string, 0, len(illust.MetaPages))
		for _, page := range illust.MetaPages {
			if rawURL := strings.TrimSpace(page.ImageURLs.Original); rawURL != "" {
				urls = append(urls, rawURL)
			}
		}
		if len(urls) > 0 {
			return urls
		}
	}

	if rawURL := strings.TrimSpace(illust.MetaSinglePage.OriginalImageURL); rawURL != "" {
		return []string{rawURL}
	}

	urls := make([]string, 0, len(illust.MetaPages))
	for _, page := range illust.MetaPages {
		if rawURL := strings.TrimSpace(page.ImageURLs.Original); rawURL != "" {
			urls = append(urls, rawURL)
		}
	}
	return urls
}

func buildPixivImageProxyPath(rawURL string) string {
	rawURL = strings.TrimSpace(rawURL)
	if rawURL == "" {
		return ""
	}
	return "/pixiv/image?url=" + url.QueryEscape(rawURL)
}

func buildPixivUgoiraGIFPath(pid int) string {
	if pid <= 0 {
		return ""
	}
	return "/pixiv/ugoira/gif?pid=" + strconv.Itoa(pid)
}

func validatePixivImageURL(rawURL string) (*url.URL, error) {
	rawURL = strings.TrimSpace(rawURL)
	if rawURL == "" {
		return nil, fmt.Errorf("缺少图片 URL 参数")
	}

	parsedURL, err := url.Parse(rawURL)
	if err != nil {
		return nil, fmt.Errorf("图片 URL 无效")
	}
	if parsedURL.Scheme != "https" && parsedURL.Scheme != "http" {
		return nil, fmt.Errorf("图片 URL 协议无效")
	}

	host := strings.ToLower(strings.TrimSpace(parsedURL.Hostname()))
	if host == "" || (host != "pximg.net" && !strings.HasSuffix(host, ".pximg.net")) {
		return nil, fmt.Errorf("仅支持代理 pximg.net 图片")
	}
	return parsedURL, nil
}

func openPixivBinaryResponse(ctx context.Context, rawURL string) (*http.Response, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, rawURL, nil)
	if err != nil {
		return nil, fmt.Errorf("创建 Pixiv 图片请求失败: %w", err)
	}
	for key, value := range pixivImageHeaders() {
		req.Header.Set(key, value)
	}

	client := &http.Client{Timeout: 120 * time.Second}
	resp, err := client.Do(req)
	if err != nil {
		return nil, fmt.Errorf("请求 Pixiv 图片失败: %w", err)
	}
	return resp, nil
}

func downloadPixivBinary(ctx context.Context, rawURL string, maxBytes int64) ([]byte, error) {
	resp, err := openPixivBinaryResponse(ctx, rawURL)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		_, _ = io.Copy(io.Discard, io.LimitReader(resp.Body, 64*1024))
		return nil, fmt.Errorf("Pixiv 资源下载失败: HTTP %d", resp.StatusCode)
	}

	if maxBytes <= 0 {
		maxBytes = 32 << 20
	}
	data, err := io.ReadAll(io.LimitReader(resp.Body, maxBytes+1))
	if err != nil {
		return nil, fmt.Errorf("读取 Pixiv 资源失败: %w", err)
	}
	if int64(len(data)) > maxBytes {
		return nil, fmt.Errorf("Pixiv 资源体积超过限制")
	}
	return data, nil
}

func convertPixivUgoiraToGIF(zipData []byte, frames []pixivUgoiraFrame) ([]byte, error) {
	if len(zipData) == 0 {
		return nil, fmt.Errorf("ugoira zip 数据为空")
	}
	if len(frames) == 0 {
		return nil, fmt.Errorf("ugoira 帧信息为空")
	}

	reader, err := zip.NewReader(bytes.NewReader(zipData), int64(len(zipData)))
	if err != nil {
		return nil, fmt.Errorf("解析 ugoira zip 失败: %w", err)
	}

	zipFiles := make(map[string]*zip.File, len(reader.File))
	for _, file := range reader.File {
		zipFiles[file.Name] = file
		zipFiles[path.Base(file.Name)] = file
	}

	gifFrames := &gif.GIF{}
	for _, frame := range frames {
		frameName := strings.TrimSpace(frame.File)
		if frameName == "" {
			return nil, fmt.Errorf("ugoira 帧文件名为空")
		}

		zipFile := zipFiles[frameName]
		if zipFile == nil {
			zipFile = zipFiles[path.Base(frameName)]
		}
		if zipFile == nil {
			return nil, fmt.Errorf("ugoira zip 缺少帧文件: %s", frameName)
		}

		rc, err := zipFile.Open()
		if err != nil {
			return nil, fmt.Errorf("打开 ugoira 帧失败: %w", err)
		}
		img, _, err := image.Decode(rc)
		rc.Close()
		if err != nil {
			return nil, fmt.Errorf("解析 ugoira 帧失败: %w", err)
		}

		bounds := img.Bounds()
		paletted := image.NewPaletted(bounds, palette.Plan9)
		draw.FloydSteinberg.Draw(paletted, bounds, img, bounds.Min)

		gifFrames.Image = append(gifFrames.Image, paletted)
		gifFrames.Delay = append(gifFrames.Delay, frameDelayToGIF(frame.Delay))
	}

	var buffer bytes.Buffer
	if err := gif.EncodeAll(&buffer, gifFrames); err != nil {
		return nil, fmt.Errorf("编码 ugoira GIF 失败: %w", err)
	}
	return buffer.Bytes(), nil
}

func frameDelayToGIF(delayMS int) int {
	if delayMS <= 0 {
		return 1
	}
	delay := delayMS / 10
	if delay <= 0 {
		delay = 1
	}
	return delay
}
