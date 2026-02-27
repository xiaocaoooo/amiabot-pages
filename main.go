package main

import (
	"github.com/gin-gonic/gin"
	"github.com/xiaocaoooo/amiabot-pages/handlers/bilibili" // 替换为你的 go mod 名字
)

func main() {
	r := gin.Default()
	r.LoadHTMLFiles(
		"templates/layout.html",
		"templates/logo.html",
		"templates/bilibili/video.html",
	)
	bilibiliGroup := r.Group("/bilibili")
	{
		bilibiliGroup.GET("/video", bilibili.VideoHandler)
	}

	r.Run(":8080")
}
