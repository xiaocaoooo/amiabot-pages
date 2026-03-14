package pjsk

import (
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/gin-gonic/gin"
)

const assetsCacheDir = "cache/pjsk"

var allServers = []string{"jp", "cn", "en", "tw", "kr"}

// commitSHAs 内存中保存的各服务器最新 commit SHA
var commitSHAs sync.Map // server -> string

func repoOwner() string { return "Sekai-World" }

// serverToRepoCode 服务器代码到仓库代码的映射（tw -> tc）
var serverToRepoCode = map[string]string{
	"tw": "tc",
}

func dbDiffName(server string) string {
	if server == "jp" {
		return "sekai-master-db-diff"
	}
	code := server
	if mapped, ok := serverToRepoCode[server]; ok {
		code = mapped
	}
	return "sekai-master-db-" + code + "-diff"
}

func serverCacheDir(server string) string {
	return filepath.Join(assetsCacheDir, dbDiffName(server))
}

func remoteURL(server, file string) string {
	return "https://sekai-world.github.io/" + dbDiffName(server) + "/" + file
}


// ghContentsEntry GitHub Contents API 返回的单个条目
type ghContentsEntry struct {
	Name string `json:"name"`
	Type string `json:"type"`
}

// ghCommit GitHub Commits API 返回的条目
type ghCommit struct {
	SHA string `json:"sha"`
}

const commitSHAFile = ".commit_sha"

// githubToken 从环境变量读取 GitHub API Token
var githubToken = os.Getenv("GITHUB_TOKEN")

// newGitHubRequest 创建带认证的 GitHub API 请求
func newGitHubRequest(url string) (*http.Request, error) {
	req, err := http.NewRequest(http.MethodGet, url, nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("User-Agent", "amiabot-pages/1.0")
	req.Header.Set("Accept", "application/vnd.github.v3+json")
	if githubToken != "" {
		req.Header.Set("Authorization", "Bearer "+githubToken)
	}
	return req, nil
}

// fetchLatestCommitSHA 通过 GitHub API 获取仓库 main 分支最新 commit SHA
func fetchLatestCommitSHA(server string) (string, error) {
	repo := dbDiffName(server)
	url := fmt.Sprintf("https://api.github.com/repos/%s/%s/commits?sha=main&per_page=1", repoOwner(), repo)

	req, err := newGitHubRequest(url)
	if err != nil {
		return "", err
	}

	client := &http.Client{Timeout: 15 * time.Second}
	resp, err := client.Do(req)
	if err != nil {
		return "", fmt.Errorf("请求 GitHub API 失败 (%s): %w", server, err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return "", fmt.Errorf("GitHub API 返回 %d (%s)", resp.StatusCode, server)
	}

	var commits []ghCommit
	if err := json.NewDecoder(resp.Body).Decode(&commits); err != nil {
		return "", fmt.Errorf("解析 commit 响应失败 (%s): %w", server, err)
	}
	if len(commits) == 0 {
		return "", fmt.Errorf("未获取到 commit (%s)", server)
	}
	return commits[0].SHA, nil
}

// loadSavedSHA 从文件加载已保存的 commit SHA
func loadSavedSHA(server string) string {
	path := filepath.Join(serverCacheDir(server), commitSHAFile)
	data, err := os.ReadFile(path)
	if err != nil {
		return ""
	}
	return strings.TrimSpace(string(data))
}

// saveSHA 保存 commit SHA 到文件和内存
func saveSHA(server, sha string) {
	commitSHAs.Store(server, sha)
	dir := serverCacheDir(server)
	_ = os.MkdirAll(dir, 0o755)
	_ = os.WriteFile(filepath.Join(dir, commitSHAFile), []byte(sha), 0o644)
}

// fetchFileList 通过 GitHub Contents API 获取仓库根目录所有 .json 文件名
func fetchFileList(server string) ([]string, error) {
	repo := dbDiffName(server)
	url := fmt.Sprintf("https://api.github.com/repos/%s/%s/contents/", repoOwner(), repo)

	req, err := newGitHubRequest(url)
	if err != nil {
		return nil, err
	}

	client := &http.Client{Timeout: 30 * time.Second}
	resp, err := client.Do(req)
	if err != nil {
		return nil, fmt.Errorf("请求 GitHub API 失败 (%s): %w", server, err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("GitHub API 返回 %d (%s)", resp.StatusCode, server)
	}

	var entries []ghContentsEntry
	if err := json.NewDecoder(resp.Body).Decode(&entries); err != nil {
		return nil, fmt.Errorf("解析 GitHub API 响应失败 (%s): %w", server, err)
	}

	var files []string
	for _, e := range entries {
		if e.Type == "file" && strings.HasSuffix(e.Name, ".json") {
			files = append(files, e.Name)
		}
	}
	return files, nil
}

func downloadFile(server, file string) error {
	url := remoteURL(server, file)
	req, err := http.NewRequest(http.MethodGet, url, nil)
	if err != nil {
		return err
	}
	req.Header.Set("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")

	client := &http.Client{Timeout: 60 * time.Second}
	resp, err := client.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("HTTP %d for %s", resp.StatusCode, url)
	}

	data, err := io.ReadAll(resp.Body)
	if err != nil {
		return err
	}

	dir := serverCacheDir(server)
	_ = os.MkdirAll(dir, 0o755)
	return os.WriteFile(filepath.Join(dir, file), data, 0o644)
}

// refreshServer 刷新单个服务器：检查 commit SHA 和文件完整性，无变化且完整则跳过
func refreshServer(server string, maxConcurrency int, force bool) map[string]string {
	// 获取远程最新 commit SHA
	remoteSHA, err := fetchLatestCommitSHA(server)
	if err != nil {
		return map[string]string{"_error": err.Error()}
	}

	files, err := fetchFileList(server)
	if err != nil {
		return map[string]string{"_error": err.Error()}
	}

	if !force {
		// 对比本地 SHA
		localSHA := ""
		if v, ok := commitSHAs.Load(server); ok {
			localSHA = v.(string)
		} else {
			localSHA = loadSavedSHA(server)
		}

		if localSHA == remoteSHA {
			// SHA 相同，检查本地文件完整性
			missing := 0
			dir := serverCacheDir(server)
			for _, f := range files {
				if _, err := os.Stat(filepath.Join(dir, f)); err != nil {
					missing++
				}
			}
			if missing == 0 {
				return map[string]string{"_skipped": "commit SHA 未变化且文件完整: " + remoteSHA[:12]}
			}
			log.Printf("[pjsk] %s: SHA 未变化但缺少 %d 个文件，继续下载", server, missing)
		}
	}

	type result struct {
		file string
		err  error
	}

	ch := make(chan result, len(files))
	sem := make(chan struct{}, maxConcurrency)
	var wg sync.WaitGroup

	for _, f := range files {
		wg.Add(1)
		go func(file string) {
			defer wg.Done()
			sem <- struct{}{}
			defer func() { <-sem }()
			var dlErr error
			for attempt := 0; attempt < 3; attempt++ {
				if dlErr = downloadFile(server, file); dlErr == nil {
					break
				}
				time.Sleep(time.Duration(attempt+1) * time.Second)
			}
			ch <- result{file: file, err: dlErr}
		}(f)
	}

	wg.Wait()
	close(ch)

	results := make(map[string]string, len(files))
	for r := range ch {
		if r.err != nil {
			results[r.file] = r.err.Error()
		} else {
			results[r.file] = "ok"
		}
	}

	// 下载完成后保存新的 commit SHA
	saveSHA(server, remoteSHA)
	results["_commit"] = remoteSHA

	return results
}

// RefreshAll 并发刷新所有服务器（每个服务器内部也并发下载）
func RefreshAll(force bool) map[string]map[string]string {
	var mu sync.Mutex
	allResults := make(map[string]map[string]string, len(allServers))
	var wg sync.WaitGroup

	for _, s := range allServers {
		wg.Add(1)
		go func(server string) {
			defer wg.Done()
			r := refreshServer(server, 20, force)
			mu.Lock()
			allResults[server] = r
			mu.Unlock()
		}(s)
	}

	wg.Wait()
	return allResults
}

// ReadCachedJSON 读取缓存的 JSON 文件
func ReadCachedJSON(server, file string) ([]byte, error) {
	path := filepath.Join(serverCacheDir(server), file)
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("缓存文件不存在: %s/%s，请先调用 /pjsk/assets/refresh", server, file)
	}
	return data, nil
}

// InitAssets 启动时加载 commit SHA，后台全量刷新
func InitAssets() {
	for _, s := range allServers {
		if sha := loadSavedSHA(s); sha != "" {
			commitSHAs.Store(s, sha)
		}
	}

	log.Println("[pjsk] 后台启动全量下载...")
	go func() {
		results := RefreshAll(false)
		total, failed := 0, 0
		for server, serverResults := range results {
			for key, status := range serverResults {
				if strings.HasPrefix(key, "_") {
					log.Printf("[pjsk] %s: %s = %s", server, key, status)
					continue
				}
				total++
				if status != "ok" {
					failed++
					log.Printf("[pjsk] 下载失败: %s 原因: %s", remoteURL(server, key), status)
				}
			}
		}
		log.Printf("[pjsk] 全量下载完成: %d 个文件, %d 个失败", total, failed)
	}()
}

// AssetHandler GET /pjsk/assets/*path
// /pjsk/assets/refresh → 刷新缓存
// /pjsk/assets/sekai-master-db-diff/xxx.json → 返回缓存 JSON
func AssetHandler(c *gin.Context) {
	raw := strings.TrimPrefix(c.Param("path"), "/")

	if raw == "refresh" {
		force := c.Query("force") == "true"
		results := RefreshAll(force)
		c.JSON(http.StatusOK, gin.H{"results": results})
		return
	}

	parts := strings.SplitN(raw, "/", 2)
	if len(parts) != 2 || parts[1] == "" {
		c.JSON(http.StatusBadRequest, gin.H{"error": "无效路径"})
		return
	}

	dirName, file := parts[0], parts[1]

	var server string
	if dirName == "sekai-master-db-diff" {
		server = "jp"
	} else if strings.HasPrefix(dirName, "sekai-master-db-") && strings.HasSuffix(dirName, "-diff") {
		server = strings.TrimSuffix(strings.TrimPrefix(dirName, "sekai-master-db-"), "-diff")
	} else {
		c.JSON(http.StatusBadRequest, gin.H{"error": "无效的仓库名: " + dirName})
		return
	}

	if !validServers[server] {
		c.JSON(http.StatusBadRequest, gin.H{"error": "无效的服务器: " + server})
		return
	}

	data, err := ReadCachedJSON(server, file)
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"error": err.Error()})
		return
	}

	c.Data(http.StatusOK, "application/json; charset=utf-8", data)
}
