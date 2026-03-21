package pjsk

import (
	"encoding/base64"
	"fmt"
	"net/http"
	"strings"

	"github.com/gin-gonic/gin"
)

const assetBinaryCacheControl = "public, max-age=31536000, immutable"

func decodeDataURL(dataURL string) (string, []byte, error) {
	raw := strings.TrimSpace(dataURL)
	if !strings.HasPrefix(raw, "data:") {
		return "", nil, fmt.Errorf("invalid data URL prefix")
	}

	metaAndPayload := strings.TrimPrefix(raw, "data:")
	meta, payload, ok := strings.Cut(metaAndPayload, ",")
	if !ok || payload == "" {
		return "", nil, fmt.Errorf("invalid data URL payload")
	}
	if !strings.HasSuffix(meta, ";base64") {
		return "", nil, fmt.Errorf("unsupported data URL encoding")
	}

	contentType := strings.TrimSuffix(meta, ";base64")
	if contentType == "" {
		contentType = "application/octet-stream"
	}

	data, err := base64.StdEncoding.DecodeString(payload)
	if err != nil {
		return "", nil, fmt.Errorf("decode base64 failed: %w", err)
	}
	return contentType, data, nil
}

// AssetBinaryHandler GET /pjsk/assets/:label
// 示例: /pjsk/assets/event:background:event_marathon_001?server=jp
func AssetBinaryHandler(c *gin.Context) {
	server := c.DefaultQuery("server", "jp")
	if !validServers[server] {
		c.JSON(http.StatusBadRequest, gin.H{"error": "无效的服务器参数，支持: jp, cn, en, tw, kr"})
		return
	}

	label := strings.TrimSpace(c.Param("label"))
	if label == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "缺少资源 label"})
		return
	}

	if normalizedLabel, _ := buildRelativePathsByLabel(label); normalizedLabel == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "不支持的资源 label: " + label})
		return
	}

	dataURL := downloadAssetByLabel(server, label)
	if dataURL == "" {
		c.JSON(http.StatusNotFound, gin.H{"error": "资源下载失败: " + label})
		return
	}

	contentType, payload, err := decodeDataURL(string(dataURL))
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "解析资源失败: " + err.Error()})
		return
	}

	c.Header("Cache-Control", assetBinaryCacheControl)
	c.Data(http.StatusOK, contentType, payload)
}
