package gallery

import (
	htmltemplate "html/template"
	"net/http"
	"strconv"
	"strings"

	"github.com/gin-gonic/gin"
)

type duplicatePageData struct {
	CurrentImage  htmltemplate.URL
	ExistingImage htmltemplate.URL
	DuplicateID   string
	CurrentTags   []string
	ExistingTags  []string
}

func DuplicateHandler(c *gin.Context) {
	currentURL := strings.TrimSpace(c.Query("current_image_url"))
	duplicateID := strings.TrimSpace(c.Query("duplicate_id"))
	if currentURL == "" || duplicateID == "" {
		renderGalleryError(c, "gallery/duplicate", "缺少必要参数，无法生成重复图片对比页")
		return
	}

	existingURL := buildGalleryRenderURLFromEnv(duplicateID)
	if existingURL == "" {
		renderGalleryError(c, "gallery/duplicate", "重复图片地址无效，无法生成对比页")
		return
	}

	data := duplicatePageData{
		CurrentImage:  galleryImageDownloader(currentURL, -1, nil),
		ExistingImage: galleryImageDownloader(existingURL, -1, nil),
		DuplicateID:   duplicateID,
		CurrentTags:   splitGalleryTags(c.Query("current_tags")),
		ExistingTags:  splitGalleryTags(c.Query("existing_tags")),
	}
	if data.CurrentImage == "" || data.ExistingImage == "" {
		renderGalleryError(c, "gallery/duplicate", "图片加载失败，请检查图片地址是否可访问")
		return
	}

	c.HTML(http.StatusOK, "gallery/duplicate", gin.H{"Duplicate": data})
}

func buildGalleryRenderURLFromEnv(duplicateID string) string {
	parsedID, err := strconv.ParseInt(strings.TrimSpace(duplicateID), 10, 64)
	if err != nil || parsedID <= 0 {
		return ""
	}
	return buildGalleryRenderURL(parsedID)
}
