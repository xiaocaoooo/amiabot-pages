package pixiv

import (
	"fmt"
	stdhtml "html"
	htmltemplate "html/template"
	"net/http"
	"os"
	"regexp"
	"strconv"
	"strings"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/xiaocaoooo/amiabot-pages/pkg/imgcache"
)

const pixivPreviewLimit = 6

var (
	pixivBreakTagPattern  = regexp.MustCompile(`(?i)<br\s*/?>`)
	pixivParagraphPattern = regexp.MustCompile(`(?i)</?p[^>]*>`)
	pixivListItemStart    = regexp.MustCompile(`(?i)<li[^>]*>`)
	pixivListItemEnd      = regexp.MustCompile(`(?i)</li>`)
	pixivAnyTagPattern    = regexp.MustCompile(`(?is)<[^>]+>`)
)

type pixivIllustPageData struct {
	ID                int
	Title             string
	Type              string
	TypeRaw           string
	AuthorName        string
	AuthorAccount     string
	AuthorAvatar      htmltemplate.URL
	MainImage         htmltemplate.URL
	Previews          []pixivPreviewView
	PageCount         int
	DisplayedPreviews int
	HasMorePreviews   bool
	RemainingPreviews int
	Dimensions        string
	Width             int
	Height            int
	TotalView         string
	TotalBookmarks    string
	TotalComments     string
	CreateAt          string
	Caption           string
	Tags              []pixivTagView
	SeriesID          int
	SeriesTitle       string
	HasSeries         bool
	IsAIGenerated     bool
	AIGeneratedLabel  string
}

type pixivPreviewView struct {
	Index int
	Image htmltemplate.URL
}

type pixivTagView struct {
	Name           string
	TranslatedName string
}

func loadPixivTagBlacklist() map[string]struct{} {
	raw := strings.TrimSpace(os.Getenv("PIXIV_TAG_BLACKLIST"))
	if raw == "" {
		return nil
	}

	blacklist := make(map[string]struct{})
	for _, token := range strings.FieldsFunc(raw, func(r rune) bool {
		switch r {
		case ',', '，', ';', '；', '\n', '\r', '\t':
			return true
		default:
			return false
		}
	}) {
		normalized := normalizePixivTagValue(token)
		if normalized == "" {
			continue
		}
		blacklist[normalized] = struct{}{}
	}
	if len(blacklist) == 0 {
		return nil
	}
	return blacklist
}

func normalizePixivTagValue(s string) string {
	return strings.ToLower(strings.TrimSpace(s))
}

func isPixivTagBlacklisted(tag pixivTag, blacklist map[string]struct{}) bool {
	if len(blacklist) == 0 {
		return false
	}
	if _, ok := blacklist[normalizePixivTagValue(tag.Name)]; ok {
		return true
	}
	if _, ok := blacklist[normalizePixivTagValue(tag.TranslatedName)]; ok {
		return true
	}
	return false
}

func renderIllustError(c *gin.Context, errMsg string) {
	c.HTML(http.StatusOK, "pixiv/illust", gin.H{"Error": errMsg})
}

func IllustInfoHandler(c *gin.Context) {
	pidStr := strings.TrimSpace(c.Query("pid"))
	if pidStr == "" {
		renderIllustError(c, "缺少插画 PID 参数")
		return
	}

	pid, err := strconv.Atoi(pidStr)
	if err != nil || pid <= 0 {
		renderIllustError(c, "无效的插画 PID: "+pidStr)
		return
	}

	illust, err := getPixivIllustDetail(c, pid)
	if err != nil {
		renderIllustError(c, err.Error())
		return
	}

	c.HTML(http.StatusOK, "pixiv/illust", gin.H{
		"Illust": buildPixivIllustPageData(illust),
	})
}

func buildPixivIllustPageData(illust *pixivIllust) pixivIllustPageData {
	previewURLs := extractPixivPreviewURLs(illust)
	pageCount := illust.PageCount
	if pageCount <= 0 {
		switch {
		case len(previewURLs) > 0:
			pageCount = len(previewURLs)
		default:
			pageCount = 1
		}
	}

	mainImageURL := pickPixivMainImageURL(illust)
	if mainImageURL == "" && len(previewURLs) > 0 {
		mainImageURL = previewURLs[0]
	}

	displayedPreviews := len(previewURLs)
	if displayedPreviews > pixivPreviewLimit {
		displayedPreviews = pixivPreviewLimit
	}

	previews := make([]pixivPreviewView, 0, displayedPreviews)
	for i := 0; i < displayedPreviews; i++ {
		previews = append(previews, pixivPreviewView{
			Index: i + 1,
			Image: downloadPixivImage(previewURLs[i]),
		})
	}

	blacklist := loadPixivTagBlacklist()
	tags := make([]pixivTagView, 0, len(illust.Tags))
	for _, tag := range illust.Tags {
		if isPixivTagBlacklisted(tag, blacklist) {
			continue
		}
		tags = append(tags, pixivTagView{
			Name:           tag.Name,
			TranslatedName: tag.TranslatedName,
		})
	}

	remainingPreviews := pageCount - displayedPreviews
	if remainingPreviews < 0 {
		remainingPreviews = 0
	}

	data := pixivIllustPageData{
		ID:                illust.ID,
		Title:             illust.Title,
		Type:              pixivWorkTypeLabel(illust.Type),
		TypeRaw:           illust.Type,
		AuthorName:        illust.User.Name,
		AuthorAccount:     illust.User.Account,
		AuthorAvatar:      downloadPixivImage(illust.User.ProfileImageURLs.Medium),
		MainImage:         downloadPixivImage(mainImageURL),
		Previews:          previews,
		PageCount:         pageCount,
		DisplayedPreviews: displayedPreviews,
		HasMorePreviews:   remainingPreviews > 0,
		RemainingPreviews: remainingPreviews,
		Dimensions:        formatPixivDimensions(illust.Width, illust.Height),
		Width:             illust.Width,
		Height:            illust.Height,
		TotalView:         formatPixivCount(illust.TotalView),
		TotalBookmarks:    formatPixivCount(illust.TotalBookmarks),
		TotalComments:     formatPixivCount(illust.TotalComments),
		CreateAt:          formatPixivCreateDate(illust.CreateDate),
		Caption:           normalizePixivCaption(illust.Caption),
		Tags:              tags,
		IsAIGenerated:     illust.IllustAIType == 2,
		AIGeneratedLabel:  "AI生成作品",
	}

	if illust.Series != nil && illust.Series.ID > 0 {
		data.HasSeries = true
		data.SeriesID = illust.Series.ID
		data.SeriesTitle = illust.Series.Title
	}

	return data
}

func downloadPixivImage(imageURL string) htmltemplate.URL {
	return imgcache.Default.Download(strings.TrimSpace(imageURL), -1, pixivImageHeaders())
}

func pickPixivMainImageURL(illust *pixivIllust) string {
	if len(illust.MetaPages) > 0 {
		return firstNonEmpty(
			illust.MetaPages[0].ImageURLs.Large,
			illust.MetaPages[0].ImageURLs.Original,
			illust.MetaPages[0].ImageURLs.Medium,
		)
	}

	return firstNonEmpty(
		illust.MetaSinglePage.OriginalImageURL,
		illust.ImageURLs.Large,
		illust.ImageURLs.Medium,
	)
}

func extractPixivPreviewURLs(illust *pixivIllust) []string {
	urls := make([]string, 0, len(illust.MetaPages))
	for _, page := range illust.MetaPages {
		if imageURL := firstNonEmpty(page.ImageURLs.Large, page.ImageURLs.Original, page.ImageURLs.Medium); imageURL != "" {
			urls = append(urls, imageURL)
		}
	}
	if len(urls) > 0 {
		return urls
	}

	if mainImageURL := pickPixivMainImageURL(illust); mainImageURL != "" {
		return []string{mainImageURL}
	}
	return nil
}

func normalizePixivCaption(raw string) string {
	text := strings.TrimSpace(raw)
	if text == "" {
		return ""
	}

	text = pixivBreakTagPattern.ReplaceAllString(text, "\n")
	text = pixivParagraphPattern.ReplaceAllString(text, "\n")
	text = pixivListItemStart.ReplaceAllString(text, "• ")
	text = pixivListItemEnd.ReplaceAllString(text, "\n")
	text = pixivAnyTagPattern.ReplaceAllString(text, "")
	text = stdhtml.UnescapeString(text)
	text = strings.ReplaceAll(text, "\r\n", "\n")
	text = strings.ReplaceAll(text, "\r", "\n")

	lines := strings.Split(text, "\n")
	normalized := make([]string, 0, len(lines))
	lastBlank := false
	for _, line := range lines {
		trimmed := strings.TrimSpace(line)
		if trimmed == "" {
			if lastBlank {
				continue
			}
			lastBlank = true
			normalized = append(normalized, "")
			continue
		}
		lastBlank = false
		normalized = append(normalized, trimmed)
	}

	return strings.TrimSpace(strings.Join(normalized, "\n"))
}

func formatPixivCount(v int) string {
	value := int64(v)
	if value >= 100000000 {
		return fmt.Sprintf("%.1f亿", float64(value)/100000000)
	}
	if value >= 10000 {
		return fmt.Sprintf("%.1f万", float64(value)/10000)
	}
	return strconv.FormatInt(value, 10)
}

func formatPixivDimensions(width, height int) string {
	if width <= 0 || height <= 0 {
		return "-"
	}
	return fmt.Sprintf("%dx%d", width, height)
}

func formatPixivCreateDate(raw string) string {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return ""
	}

	parsed, err := time.Parse(time.RFC3339, raw)
	if err != nil {
		return raw
	}
	return parsed.Local().Format("2006-01-02 15:04:05")
}

func pixivWorkTypeLabel(kind string) string {
	switch strings.ToLower(strings.TrimSpace(kind)) {
	case "illust":
		return "插画"
	case "manga":
		return "漫画"
	case "ugoira":
		return "动图"
	default:
		if kind == "" {
			return "未知"
		}
		return kind
	}
}

func firstNonEmpty(values ...string) string {
	for _, value := range values {
		if trimmed := strings.TrimSpace(value); trimmed != "" {
			return trimmed
		}
	}
	return ""
}
