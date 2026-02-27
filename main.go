package main

import (
	"net/http"
	"os"

	"github.com/gin-gonic/gin"
	"github.com/xiaocaoooo/amiabot-pages/handlers/bilibili"
)

func main() {
	r := gin.Default()
	r.GET("/health", func(c *gin.Context) {
		c.JSON(http.StatusOK, gin.H{"status": "ok"})
	})

	r.LoadHTMLFiles(
		"templates/layout.html",
		"templates/logo.html",
		"templates/bilibili/video.html",
	)
	bilibiliGroup := r.Group("/bilibili")
	{
		bilibiliGroup.GET("/video", bilibili.VideoHandler)
	}

	port := os.Getenv("PORT")
	if port == "" {
		port = "8080"
	}

	r.Run(":" + port)
}
