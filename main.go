package main

import (
	"net/http"
	"os"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/xiaocaoooo/amiabot-pages/handlers/bilibili"
	"github.com/xiaocaoooo/amiabot-pages/handlers/pjsk"
	"github.com/xiaocaoooo/amiabot-pages/handlers/status"
	"github.com/xiaocaoooo/amiabot-pages/pkg/imgcache"
)

func main() {
	r := gin.Default()
	r.GET("/health", func(c *gin.Context) {
		c.JSON(http.StatusOK, gin.H{"status": "ok"})
	})
	statusGroup := r.Group("/status")
	{
		statusGroup.GET("/zeabur", status.ZeaburPageHandler)
	}

	r.LoadHTMLFiles(
		"templates/layout.html",
		"templates/logo.html",
		"templates/bilibili/video.html",
		"templates/pjsk/event.html",
		"templates/pjsk/card.html",
		"templates/pjsk/music.html",
		"templates/status/zeabur.html",
	)
	bilibiliGroup := r.Group("/bilibili")
	{
		bilibiliGroup.GET("/video", bilibili.VideoHandler)
	}

	pjskGroup := r.Group("/pjsk")
	{
		pjskGroup.GET("/event", pjsk.EventHandler)
		pjskGroup.GET("/event/current", pjsk.CurrentEventHandler)
		pjskGroup.GET("/card", pjsk.CardHandler)
		pjskGroup.GET("/music", pjsk.MusicHandler)
		pjskGroup.GET("/masterdata/*path", pjsk.MasterDataHandler)
		pjskGroup.GET("/assets/:label", pjsk.AssetBinaryHandler)
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
