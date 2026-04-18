package gallery

import (
	htmltemplate "html/template"
	"net/http"
	"strings"

	"github.com/gin-gonic/gin"
)

type galleryImageCard struct {
	ID         int64
	Preview    htmltemplate.URL
	HasPreview bool
	Tags       []string
	Dimensions string
	CreatedAt  string
}

type galleryImagesPageData struct {
	QueryTags []string
	Total     int64
	Items     []galleryImageCard
}

func ImagesHandler(c *gin.Context) {
	tags := splitGalleryTags(c.Query("tags"))
	if len(tags) == 0 {
		renderGalleryError(c, "gallery/images", "缺少标签参数，无法生成图片列表页")
		return
	}

	images, err := fetchAllGalleryImages(c.Request.Context(), tags)
	if err != nil {
		renderGalleryError(c, "gallery/images", err.Error())
		return
	}

	items := make([]galleryImageCard, 0, len(images))
	for _, image := range images {
		previewURL := buildGalleryPreviewURL(&image, galleryListPreviewWidth, galleryListPreviewHeight)
		preview := galleryImageDownloader(previewURL, -1, nil)
		items = append(items, galleryImageCard{
			ID:         image.ID,
			Preview:    preview,
			HasPreview: strings.TrimSpace(string(preview)) != "",
			Tags:       galleryTagNames(image.Tags),
			Dimensions: formatGalleryDimensions(image),
			CreatedAt:  formatGalleryCreatedAt(image),
		})
	}

	c.HTML(http.StatusOK, "gallery/images", gin.H{
		"ImagesPage": galleryImagesPageData{
			QueryTags: tags,
			Total:     int64(len(items)),
			Items:     items,
		},
	})
}
