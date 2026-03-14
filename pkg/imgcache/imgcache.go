package imgcache

import (
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"fmt"
	htmltemplate "html/template"
	"io"
	"log"
	"net/http"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"
)

const defaultCacheDir = "cache/images"
const defaultMaxSizeMB = 512

// cacheMeta 内存中只保存元数据，不保存文件内容
type cacheMeta struct {
	CreatedAt time.Time
	TTL       time.Duration
	Size      int // 文件大小（字节）
}

// fileCacheEntry 文件缓存 JSON 结构
type fileCacheEntry struct {
	URL       string `json:"url"`
	DataURL   string `json:"dataURL"`
	CreatedAt int64  `json:"createdAt"`
	TTLMs     int64  `json:"ttl"`
}

// ImageCache 图片下载缓存管理器
type ImageCache struct {
	mu        sync.RWMutex
	items     map[string]*cacheMeta // key -> 元数据（不含内容）
	totalSize int
	maxSize   int
	cacheDir  string
}

// Default 全局默认实例
var Default = New()

// New 创建缓存实例
func New() *ImageCache {
	maxMB := defaultMaxSizeMB
	if v := os.Getenv("IMAGE_CACHE_MAX_SIZE"); v != "" {
		if n, err := strconv.Atoi(v); err == nil && n > 0 {
			maxMB = n
		}
	}
	return &ImageCache{
		items:    make(map[string]*cacheMeta),
		maxSize:  maxMB * 1024 * 1024,
		cacheDir: defaultCacheDir,
	}
}

func cacheKey(imageURL string) string {
	h := sha256.Sum256([]byte(imageURL))
	return fmt.Sprintf("%x", h)
}

func (c *ImageCache) filePath(key string) string {
	return filepath.Join(c.cacheDir, key+".json")
}

func isExpired(m *cacheMeta) bool {
	if m.TTL < 0 {
		return false
	}
	return time.Since(m.CreatedAt) > m.TTL
}

// loadMetaFromFile 从文件读取元数据（不保留 DataURL 在内存）
func (c *ImageCache) loadMetaFromFile(path string) (*cacheMeta, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	var fe fileCacheEntry
	if err := json.Unmarshal(data, &fe); err != nil {
		return nil, err
	}
	m := &cacheMeta{
		CreatedAt: time.UnixMilli(fe.CreatedAt),
		TTL:       time.Duration(fe.TTLMs) * time.Millisecond,
		Size:      len(fe.DataURL),
	}
	return m, nil
}

// readDataURLFromFile 从文件读取 DataURL 内容
func (c *ImageCache) readDataURLFromFile(key string) (string, error) {
	data, err := os.ReadFile(c.filePath(key))
	if err != nil {
		return "", err
	}
	var fe fileCacheEntry
	if err := json.Unmarshal(data, &fe); err != nil {
		return "", err
	}
	return fe.DataURL, nil
}

// saveToFile 写入文件缓存
func (c *ImageCache) saveToFile(key, imageURL, dataURL string, m *cacheMeta) {
	_ = os.MkdirAll(c.cacheDir, 0o755)
	fe := fileCacheEntry{
		URL:       imageURL,
		DataURL:   dataURL,
		CreatedAt: m.CreatedAt.UnixMilli(),
		TTLMs:     m.TTL.Milliseconds(),
	}
	data, err := json.Marshal(fe)
	if err != nil {
		return
	}
	_ = os.WriteFile(c.filePath(key), data, 0o644)
}

// LoadIndex 启动时扫描缓存目录，将元数据加载到内存索引
func (c *ImageCache) LoadIndex() {
	entries, err := os.ReadDir(c.cacheDir)
	if err != nil {
		return
	}

	c.mu.Lock()
	defer c.mu.Unlock()

	loaded := 0
	for _, e := range entries {
		if e.IsDir() || !strings.HasSuffix(e.Name(), ".json") {
			continue
		}
		key := strings.TrimSuffix(e.Name(), ".json")
		path := filepath.Join(c.cacheDir, e.Name())

		m, err := c.loadMetaFromFile(path)
		if err != nil {
			_ = os.Remove(path)
			continue
		}
		if isExpired(m) {
			_ = os.Remove(path)
			continue
		}

		c.items[key] = m
		c.totalSize += m.Size
		loaded++
	}
	log.Printf("[imgcache] 已加载 %d 条缓存索引, 占用 %d MB", loaded, c.totalSize/1024/1024)
}

// download 执行实际的 HTTP 下载
func download(imageURL string, headers map[string]string) (string, error) {
	req, err := http.NewRequest(http.MethodGet, imageURL, nil)
	if err != nil {
		return "", err
	}
	req.Header.Set("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36")
	for k, v := range headers {
		req.Header.Set(k, v)
	}

	client := &http.Client{Timeout: 15 * time.Second}
	resp, err := client.Do(req)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return "", fmt.Errorf("HTTP %d for %s", resp.StatusCode, imageURL)
	}

	data, err := io.ReadAll(io.LimitReader(resp.Body, 10*1024*1024))
	if err != nil || len(data) == 0 {
		return "", fmt.Errorf("read body failed")
	}

	contentType := resp.Header.Get("Content-Type")
	if contentType == "" {
		contentType = http.DetectContentType(data)
	}

	return "data:" + contentType + ";base64," + base64.StdEncoding.EncodeToString(data), nil
}

// Download 下载图片并缓存，ttl < 0 表示永不过期
func (c *ImageCache) Download(imageURL string, ttl time.Duration, headers map[string]string) htmltemplate.URL {
	if imageURL == "" {
		return htmltemplate.URL("")
	}

	key := cacheKey(imageURL)

	// 1. 内存索引命中 → 从文件读取内容
	c.mu.RLock()
	if m, ok := c.items[key]; ok && !isExpired(m) {
		c.mu.RUnlock()
		if dataURL, err := c.readDataURLFromFile(key); err == nil {
			return htmltemplate.URL(dataURL)
		}
		// 文件丢失，清理索引
		c.mu.Lock()
		c.totalSize -= m.Size
		delete(c.items, key)
		c.mu.Unlock()
	} else {
		c.mu.RUnlock()
	}

	// 2. 远程下载
	dataURL, err := download(imageURL, headers)
	if err != nil {
		log.Printf("[imgcache] 下载失败: %s 原因: %v", imageURL, err)
		return htmltemplate.URL("")
	}

	m := &cacheMeta{
		CreatedAt: time.Now(),
		TTL:       ttl,
		Size:      len(dataURL),
	}

	c.mu.Lock()
	c.items[key] = m
	c.totalSize += m.Size
	c.mu.Unlock()

	c.saveToFile(key, imageURL, dataURL, m)
	log.Printf("[imgcache] 已缓存: %s (%d bytes)", imageURL, m.Size)

	return htmltemplate.URL(dataURL)
}

// Cleanup 清理过期条目，超出 maxSize 时按最早创建时间淘汰
func (c *ImageCache) Cleanup() {
	c.mu.Lock()
	defer c.mu.Unlock()

	expired := 0
	for key, m := range c.items {
		if isExpired(m) {
			c.totalSize -= m.Size
			delete(c.items, key)
			_ = os.Remove(c.filePath(key))
			expired++
		}
	}

	if c.totalSize <= c.maxSize {
		if expired > 0 {
			log.Printf("[imgcache] 清理完成: 过期 %d 条, 剩余 %d 条, 占用 %d MB", expired, len(c.items), c.totalSize/1024/1024)
		}
		return
	}

	type kv struct {
		key       string
		createdAt time.Time
		size      int
	}
	sorted := make([]kv, 0, len(c.items))
	for k, m := range c.items {
		sorted = append(sorted, kv{key: k, createdAt: m.CreatedAt, size: m.Size})
	}
	sort.Slice(sorted, func(i, j int) bool {
		return sorted[i].createdAt.Before(sorted[j].createdAt)
	})

	evicted := 0
	for _, item := range sorted {
		if c.totalSize <= c.maxSize {
			break
		}
		c.totalSize -= item.size
		delete(c.items, item.key)
		_ = os.Remove(c.filePath(item.key))
		evicted++
	}
	log.Printf("[imgcache] 清理完成: 过期 %d 条, 淘汰 %d 条, 剩余 %d 条, 占用 %d MB", expired, evicted, len(c.items), c.totalSize/1024/1024)
}

// StartCleanupTicker 启动定时清理 goroutine
func (c *ImageCache) StartCleanupTicker(interval time.Duration) {
	go func() {
		ticker := time.NewTicker(interval)
		defer ticker.Stop()
		for range ticker.C {
			c.Cleanup()
		}
	}()
}
