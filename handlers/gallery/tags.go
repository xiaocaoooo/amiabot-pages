package gallery

import (
	"context"
	"fmt"
	htmltemplate "html/template"
	"net/http"
	"strings"
	"sync"

	"github.com/gin-gonic/gin"
)

type galleryTagCard struct {
	Name         string
	Count        int64
	FirstImageID int64
	Preview      htmltemplate.URL
	HasPreview   bool
}

type galleryTagsPageData struct {
	TotalTags int
	Items     []galleryTagCard
}

func TagsHandler(c *gin.Context) {
	tags, err := fetchGalleryTags(c.Request.Context(), "", galleryTagListLimit)
	if err != nil {
		renderGalleryError(c, "gallery/tags", err.Error())
		return
	}

	items, err := buildGalleryTagCards(c.Request.Context(), tags)
	if err != nil {
		renderGalleryError(c, "gallery/tags", err.Error())
		return
	}

	c.HTML(http.StatusOK, "gallery/tags", gin.H{
		"TagsPage": galleryTagsPageData{
			TotalTags: len(tags),
			Items:     items,
		},
	})
}

func buildGalleryTagCards(ctx context.Context, tags []galleryTag) ([]galleryTagCard, error) {
	items := make([]galleryTagCard, len(tags))
	if len(tags) == 0 {
		return items, nil
	}

	workerLimit := galleryTagPreviewWorkers
	if workerLimit <= 0 {
		workerLimit = 1
	}

	sem := make(chan struct{}, workerLimit)
	var wg sync.WaitGroup
	var errMu sync.Mutex
	var firstErr error

	for i, tag := range tags {
		wg.Add(1)
		go func(index int, tag galleryTag) {
			defer wg.Done()
			sem <- struct{}{}
			defer func() { <-sem }()

			payload, err := fetchGalleryImagesPage(ctx, []string{tag.Name}, 1, 1)
			if err != nil {
				errMu.Lock()
				if firstErr == nil {
					firstErr = fmt.Errorf("加载标签 #%s 首图失败: %w", tag.Name, err)
				}
				errMu.Unlock()
				return
			}

			card := galleryTagCard{
				Name:  tag.Name,
				Count: payload.Total,
			}
			if len(payload.Items) > 0 {
				card.FirstImageID = payload.Items[0].ID
				previewURL := buildGalleryPreviewURL(&payload.Items[0], galleryTagPreviewWidth, galleryTagPreviewHeight)
				card.Preview = galleryImageDownloader(previewURL, -1, nil)
				card.HasPreview = strings.TrimSpace(string(card.Preview)) != ""
			}
			items[index] = card
		}(i, tag)
	}

	wg.Wait()
	if firstErr != nil {
		return nil, firstErr
	}
	return items, nil
}
