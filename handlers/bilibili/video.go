package bilibili

import (
	"encoding/json"
	"fmt"
	"net/http"
	"net/url"
	"strconv"
	"time"

	"github.com/gin-gonic/gin"
	"github.com/xiaocaoooo/amiabot-pages/pkg/imgcache"
)

type bilibiliViewResp struct {
	Code    int    `json:"code"`
	Message string `json:"message"`
	Data    struct {
		Aid      int64  `json:"aid"`
		Bvid     string `json:"bvid"`
		Title    string `json:"title"`
		Pic      string `json:"pic"`
		Desc     string `json:"desc"`
		Pubdate  int64  `json:"pubdate"`
		Ctime    int64  `json:"ctime"`
		Duration int    `json:"duration"`
		Owner    struct {
			Name string `json:"name"`
			Face string `json:"face"`
		} `json:"owner"`
		Stat struct {
			View     int64 `json:"view"`
			Danmaku  int64 `json:"danmaku"`
			Reply    int64 `json:"reply"`
			Favorite int64 `json:"favorite"`
			Coin     int64 `json:"coin"`
			Share    int64 `json:"share"`
			Like     int64 `json:"like"`
		} `json:"stat"`
		Pages []struct {
			Page     int    `json:"page"`
			Part     string `json:"part"`
			Duration int    `json:"duration"`
		} `json:"pages"`
	} `json:"data"`
}

type viewPage struct {
	Index       int
	Title       string
	DurationStr string
}

func renderWithError(c *gin.Context, errMsg string) {
	c.HTML(http.StatusOK, "bilibili/video", gin.H{
		"Error": errMsg,
	})
}

func renderDefaultVideo(c *gin.Context) {
	c.HTML(http.StatusOK, "bilibili/video", gin.H{
		"Title":              "Bilibili 视频信息示例",
		"BVID":               "BV1XY411A7c2",
		"AID":                0,
		"Cover":              "",
		"UpperName":          "AmiaBot",
		"UpperFace":          "",
		"Play":               "--",
		"Like":               "--",
		"Coin":               "--",
		"Favorite":           "--",
		"Danmaku":            "--",
		"Reply":              "--",
		"Share":              "--",
		"Duration":           "--:--",
		"Intro":              "未提供 av/bv 参数，当前展示为默认示例数据。可使用 /bilibili/video?bv=BV号 或 /bilibili/video?av=AV号 进行查询。",
		"Pages":              []viewPage{},
		"TotalPages":         0,
		"HasMorePages":       false,
		"RemainingPageCount": 0,
	})
}

func formatCount(v int64) string {
	if v >= 100000000 {
		return fmt.Sprintf("%.1f亿", float64(v)/100000000)
	}
	if v >= 10000 {
		return fmt.Sprintf("%.1f万", float64(v)/10000)
	}
	return strconv.FormatInt(v, 10)
}

func formatDuration(sec int) string {
	if sec < 0 {
		sec = 0
	}
	h := sec / 3600
	m := (sec % 3600) / 60
	s := sec % 60
	if h > 0 {
		return fmt.Sprintf("%d:%02d:%02d", h, m, s)
	}
	return fmt.Sprintf("%02d:%02d", m, s)
}

func formatUnixTime(ts int64) string {
	if ts <= 0 {
		return ""
	}
	return time.Unix(ts, 0).Local().Format("2006-01-02 15:04:05")
}

func VideoHandler(c *gin.Context) {
	bv := c.Query("bv")
	if bv == "" {
		bv = c.Query("bvid")
	}
	av := c.Query("av")
	if av == "" {
		av = c.Query("aid")
	}
	if av == "" {
		av = c.Query("avid")
	}
	isDefault := bv == "" && av == ""

	if isDefault {
		// 无参数时直接返回可展示的默认卡片，避免外部接口异常导致非预期显示
		renderDefaultVideo(c)
		return
	}

	apiURL := "https://api.bilibili.com/x/web-interface/view"
	q := url.Values{}
	if bv != "" {
		q.Set("bvid", bv)
	} else {
		q.Set("aid", av)
	}

	req, err := http.NewRequest(http.MethodGet, apiURL+"?"+q.Encode(), nil)
	if err != nil {
		renderWithError(c, "创建请求失败: "+err.Error())
		return
	}
	req.Header.Set("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36")

	client := &http.Client{Timeout: 8 * time.Second}
	resp, err := client.Do(req)
	if err != nil {
		renderWithError(c, "请求 Bilibili 接口失败: "+err.Error())
		return
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		renderWithError(c, fmt.Sprintf("Bilibili 接口返回异常状态码: %d", resp.StatusCode))
		return
	}

	var apiResp bilibiliViewResp
	if err := json.NewDecoder(resp.Body).Decode(&apiResp); err != nil {
		renderWithError(c, "解析 Bilibili 返回数据失败: "+err.Error())
		return
	}
	if apiResp.Code != 0 {
		renderWithError(c, fmt.Sprintf("Bilibili 接口错误: %s (code=%d)", apiResp.Message, apiResp.Code))
		return
	}

	pages := make([]viewPage, 0, 8)
	for i, p := range apiResp.Data.Pages {
		if i >= 8 {
			break
		}
		pages = append(pages, viewPage{
			Index:       p.Page,
			Title:       p.Part,
			DurationStr: formatDuration(p.Duration),
		})
	}

	biliHeaders := map[string]string{"Referer": "https://www.bilibili.com/"}
	coverDataURL := imgcache.Default.Download(apiResp.Data.Pic, -1, biliHeaders)
	upperFaceDataURL := imgcache.Default.Download(apiResp.Data.Owner.Face, -1, biliHeaders)
	publishTime := formatUnixTime(apiResp.Data.Pubdate)
	uploadTime := formatUnixTime(apiResp.Data.Ctime)
	showUploadTime := false

	if apiResp.Data.Pubdate > 0 && apiResp.Data.Ctime > 0 {
		delta := apiResp.Data.Pubdate - apiResp.Data.Ctime
		if delta < 0 {
			delta = -delta
		}
		showUploadTime = delta >= 30*60
	}

	c.HTML(http.StatusOK, "bilibili/video", gin.H{
		"Title":              apiResp.Data.Title,
		"BVID":               apiResp.Data.Bvid,
		"AID":                apiResp.Data.Aid,
		"Cover":              coverDataURL,
		"UpperName":          apiResp.Data.Owner.Name,
		"UpperFace":          upperFaceDataURL,
		"Play":               formatCount(apiResp.Data.Stat.View),
		"Like":               formatCount(apiResp.Data.Stat.Like),
		"Coin":               formatCount(apiResp.Data.Stat.Coin),
		"Favorite":           formatCount(apiResp.Data.Stat.Favorite),
		"Danmaku":            formatCount(apiResp.Data.Stat.Danmaku),
		"Reply":              formatCount(apiResp.Data.Stat.Reply),
		"Share":              formatCount(apiResp.Data.Stat.Share),
		"Duration":           formatDuration(apiResp.Data.Duration),
		"PublishTime":        publishTime,
		"UploadTime":         uploadTime,
		"ShowUploadTime":     showUploadTime,
		"Intro":              apiResp.Data.Desc,
		"Pages":              pages,
		"TotalPages":         len(apiResp.Data.Pages),
		"HasMorePages":       len(apiResp.Data.Pages) > len(pages),
		"RemainingPageCount": len(apiResp.Data.Pages) - len(pages),
	})
}
