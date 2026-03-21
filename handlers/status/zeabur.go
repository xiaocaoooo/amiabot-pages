package status

import (
	"bytes"
	"encoding/json"
	"fmt"
	htmltemplate "html/template"
	"io"
	"math"
	"net/http"
	"os"
	"strings"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/xiaocaoooo/amiabot-pages/pkg/imgcache"
)

const zeaburGraphQLEndpoint = "https://api.zeabur.com/graphql"

const zeaburStatusQuery = `
query Query {
  servers {
    _id
    city
    country
    name
    status {
      isOnline
      totalCPU
      usedCPU
      totalMemory
      usedMemory
      totalDisk
      usedDisk
      latency
    }
  }
  projects {
    edges {
      node {
        _id
        iconURL
        name
        services {
          _id
          name
          status
        }
      }
    }
  }
}
`

type zeaburGraphQLRequest struct {
	Query string `json:"query"`
}

type zeaburGraphQLResponse struct {
	Data    zeaburStatusData   `json:"data"`
	Errors  []zeaburGraphQLErr `json:"errors"`
	Message string             `json:"message"`
}

type zeaburGraphQLErr struct {
	Message string         `json:"message"`
	Code    map[string]any `json:"extensions"`
}

type zeaburStatusData struct {
	Servers  []zeaburServer          `json:"servers"`
	Projects zeaburProjectConnection `json:"projects"`
}

type zeaburServer struct {
	ID      string            `json:"_id"`
	City    string            `json:"city"`
	Country string            `json:"country"`
	Name    string            `json:"name"`
	Status  zeaburServerState `json:"status"`
}

type zeaburServerState struct {
	IsOnline    bool    `json:"isOnline"`
	TotalCPU    float64 `json:"totalCPU"`
	UsedCPU     float64 `json:"usedCPU"`
	TotalMemory float64 `json:"totalMemory"`
	UsedMemory  float64 `json:"usedMemory"`
	TotalDisk   float64 `json:"totalDisk"`
	UsedDisk    float64 `json:"usedDisk"`
	Latency     float64 `json:"latency"`
}

type zeaburProjectConnection struct {
	Edges []zeaburProjectEdge `json:"edges"`
}

type zeaburProjectEdge struct {
	Node zeaburProject `json:"node"`
}

type zeaburProject struct {
	ID       string          `json:"_id"`
	IconURL  string          `json:"iconURL"`
	Name     string          `json:"name"`
	Services []zeaburService `json:"services"`
}

type zeaburService struct {
	ID     string `json:"_id"`
	Name   string `json:"name"`
	Status string `json:"status"`
}

type zeaburStatusPageData struct {
	FetchedAt    string
	ServerCount  int
	OnlineCount  int
	ProjectCount int
	ServiceCount int
	Servers      []zeaburServerView
	Projects     []zeaburProjectView
}

type zeaburServerView struct {
	Name     string
	Location string
	Online   bool
	Latency  string
	CPU      string
	Memory   string
	Disk     string
}

type zeaburProjectView struct {
	Name         string
	IconURL      htmltemplate.URL
	ServiceCount int
	RunningCount int
	Services     []zeaburServiceView
}

type zeaburServiceView struct {
	Name   string
	Status string
}

func resolveZeaburToken(c *gin.Context) string {
	authHeader := strings.TrimSpace(c.GetHeader("Authorization"))
	if authHeader != "" {
		const bearerPrefix = "Bearer "
		if len(authHeader) >= len(bearerPrefix) && strings.EqualFold(authHeader[:len(bearerPrefix)], bearerPrefix) {
			return strings.TrimSpace(authHeader[len(bearerPrefix):])
		}
		return authHeader
	}
	return strings.TrimSpace(os.Getenv("ZEABUR_TOKEN"))
}

func callZeaburGraphQL(token string) ([]byte, int, error) {
	payload, err := json.Marshal(zeaburGraphQLRequest{Query: zeaburStatusQuery})
	if err != nil {
		return nil, 0, fmt.Errorf("序列化 Zeabur 请求失败: %w", err)
	}

	req, err := http.NewRequest(http.MethodPost, zeaburGraphQLEndpoint, bytes.NewReader(payload))
	if err != nil {
		return nil, 0, fmt.Errorf("创建 Zeabur 请求失败: %w", err)
	}
	req.Header.Set("Authorization", "Bearer "+token)
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("X-Request-Type", "GraphQL")

	client := &http.Client{Timeout: 15 * time.Second}
	resp, err := client.Do(req)
	if err != nil {
		return nil, 0, fmt.Errorf("请求 Zeabur 失败: %w", err)
	}
	defer resp.Body.Close()

	body, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, 0, fmt.Errorf("读取 Zeabur 响应失败: %w", err)
	}

	return body, resp.StatusCode, nil
}

func formatRatio(used, total float64) string {
	if total <= 0 {
		return fmt.Sprintf("%.2f / %.2f", used, total)
	}
	percent := used / total * 100
	return fmt.Sprintf("%.2f / %.2f (%.1f%%)", used, total, percent)
}

func formatLatency(v float64) string {
	if v <= 0 || math.IsNaN(v) || math.IsInf(v, 0) {
		return "-"
	}
	return fmt.Sprintf("%.0fms", v)
}

func buildZeaburStatusPage(data zeaburStatusData) zeaburStatusPageData {
	page := zeaburStatusPageData{
		FetchedAt: time.Now().Format("2006-01-02 15:04:05"),
	}

	page.ServerCount = len(data.Servers)
	for _, s := range data.Servers {
		if s.Status.IsOnline {
			page.OnlineCount++
		}

		location := strings.TrimSpace(strings.Trim(strings.Join([]string{s.Country, s.City}, " "), " "))
		if location == "" {
			location = "-"
		}

		page.Servers = append(page.Servers, zeaburServerView{
			Name:     s.Name,
			Location: location,
			Online:   s.Status.IsOnline,
			Latency:  formatLatency(s.Status.Latency),
			CPU:      formatRatio(s.Status.UsedCPU, s.Status.TotalCPU),
			Memory:   formatRatio(s.Status.UsedMemory, s.Status.TotalMemory),
			Disk:     formatRatio(s.Status.UsedDisk, s.Status.TotalDisk),
		})
	}

	page.ProjectCount = len(data.Projects.Edges)
	for _, edge := range data.Projects.Edges {
		p := zeaburProjectView{
			Name:         edge.Node.Name,
			IconURL:      imgcache.Default.Download(edge.Node.IconURL, -1, nil),
			ServiceCount: len(edge.Node.Services),
		}

		for _, svc := range edge.Node.Services {
			status := strings.ToUpper(strings.TrimSpace(svc.Status))
			if status == "" {
				status = "UNKNOWN"
			}
			if status == "RUNNING" {
				p.RunningCount++
			}
			p.Services = append(p.Services, zeaburServiceView{
				Name:   svc.Name,
				Status: status,
			})
		}

		page.ServiceCount += p.ServiceCount
		page.Projects = append(page.Projects, p)
	}

	return page
}

func renderZeaburError(c *gin.Context, errMsg string) {
	c.HTML(http.StatusOK, "status/zeabur", gin.H{"Error": errMsg})
}

func ZeaburPageHandler(c *gin.Context) {
	token := resolveZeaburToken(c)
	if token == "" {
		renderZeaburError(c, "缺少 Zeabur token，请在环境变量 ZEABUR_TOKEN 配置，或在请求头传 Authorization: Bearer <token>")
		return
	}

	body, statusCode, err := callZeaburGraphQL(token)
	if err != nil {
		renderZeaburError(c, err.Error())
		return
	}

	if statusCode != http.StatusOK {
		renderZeaburError(c, fmt.Sprintf("Zeabur API 返回异常状态码: %d\n%s", statusCode, strings.TrimSpace(string(body))))
		return
	}

	var payload zeaburGraphQLResponse
	if err := json.Unmarshal(body, &payload); err != nil {
		renderZeaburError(c, "解析 Zeabur 响应失败: "+err.Error())
		return
	}

	if len(payload.Errors) > 0 && len(payload.Data.Servers) == 0 && len(payload.Data.Projects.Edges) == 0 {
		renderZeaburError(c, "Zeabur GraphQL 返回错误: "+payload.Errors[0].Message)
		return
	}

	c.HTML(http.StatusOK, "status/zeabur", gin.H{
		"Status": buildZeaburStatusPage(payload.Data),
	})
}
