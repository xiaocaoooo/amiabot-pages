package main

import (
	"log"
	"net/http"
	"os"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/xiaocaoooo/amiabot-pages/handlers/bilibili"
	"github.com/xiaocaoooo/amiabot-pages/handlers/gallery"
	"github.com/xiaocaoooo/amiabot-pages/handlers/pixiv"
	"github.com/xiaocaoooo/amiabot-pages/handlers/pjsk"
	"github.com/xiaocaoooo/amiabot-pages/handlers/query"
	"github.com/xiaocaoooo/amiabot-pages/handlers/status"
	"github.com/xiaocaoooo/amiabot-pages/pkg/imgcache"
	"github.com/xiaocaoooo/amiabot-pages/pkg/paramid"
)

func main() {
	r := gin.Default()
	paramIDHandler, paramIDEnabled, err := paramid.NewFromEnv()
	if err != nil {
		log.Fatalf("初始化 param_id 中间件失败: %v", err)
	}
	groupMiddlewares := make([]gin.HandlerFunc, 0, 1)
	if paramIDEnabled {
		log.Printf("[paramid] 已启用 param_id 参数注入中间件")
		groupMiddlewares = append(groupMiddlewares, paramIDHandler)
	}

	r.GET("/health", func(c *gin.Context) {
		c.JSON(http.StatusOK, gin.H{"status": "ok"})
	})
	statusGroup := r.Group("/status", groupMiddlewares...)
	{
		statusGroup.GET("/zeabur", status.ZeaburPageHandler)
	}

	r.LoadHTMLFiles(
		"templates/layout.html",
		"templates/logo.html",
		"templates/bilibili/video.html",
		"templates/gallery/duplicate.html",
		"templates/gallery/tags.html",
		"templates/gallery/images.html",
		"templates/pixiv/illust.html",
		"templates/pjsk/event.html",
		"templates/pjsk/card.html",
		"templates/pjsk/music.html",
		"templates/pjsk/profile.html",
		"templates/pjsk/b30.html",
		"templates/query/user.html",
		"templates/query/group.html",
		"templates/status/zeabur.html",
	)
	bilibiliGroup := r.Group("/bilibili", groupMiddlewares...)
	{
		bilibiliGroup.GET("/video", bilibili.VideoHandler)
	}

	galleryGroup := r.Group("/gallery", groupMiddlewares...)
	{
		galleryGroup.GET("/duplicate", gallery.DuplicateHandler)
		galleryGroup.GET("/tags", gallery.TagsHandler)
		galleryGroup.GET("/images", gallery.ImagesHandler)
	}

	pixivGroup := r.Group("/pixiv", groupMiddlewares...)
	{
		pixivGroup.GET("/illust/info", pixiv.IllustInfoHandler)
		pixivGroup.GET("/illust/media", pixiv.IllustMediaHandler)
		pixivGroup.GET("/image", pixiv.PixivImageProxyHandler)
		pixivGroup.GET("/ugoira/gif", pixiv.PixivUgoiraGIFHandler)
	}

	pjskGroup := r.Group("/pjsk", groupMiddlewares...)
	{
		pjskGroup.GET("/event", pjsk.EventHandler)
		pjskGroup.GET("/event/current", pjsk.CurrentEventHandler)
		pjskGroup.GET("/card", pjsk.CardHandler)
		pjskGroup.GET("/music", pjsk.MusicHandler)
		pjskGroup.GET("/profile", pjsk.ProfileHandler)
		pjskGroup.GET("/profile/raw", pjsk.ProfileRawHandler)
		pjskGroup.GET("/b30", pjsk.B30Handler)
		pjskGroup.GET("/masterdata/*path", pjsk.MasterDataHandler)
		pjskGroup.GET("/assets/:label", pjsk.AssetBinaryHandler)
	}

	queryGroup := r.Group("/query", groupMiddlewares...)
	{
		queryGroup.GET("/user", query.UserHandler)
		queryGroup.GET("/group", query.GroupHandler)
	}

	pjsk.InitMasterData()

	imgcache.Default.LoadIndex()
	imgcache.Default.StartCleanupTicker(10 * time.Minute)

	port := os.Getenv("PORT")
	if port == "" {
		port = "8080"
	}

	r.Run(":" + port)
}
