package pjsk

import (
	"encoding/json"
	"fmt"
	htmltemplate "html/template"
	"net/http"
	"strconv"
	"strings"

	"github.com/gin-gonic/gin"
)

// pjskMusic 音乐基础信息
type pjskMusic struct {
	ID              int    `json:"id"`
	Title           string `json:"title"`
	Lyricist        string `json:"lyricist"`
	Composer        string `json:"composer"`
	Arranger        string `json:"arranger"`
	AssetbundleName string `json:"assetbundleName"`
	PublishedAt     int64  `json:"publishedAt"`
}

// pjskMusicVocal 歌词信息
type pjskMusicVocal struct {
	ID           int    `json:"id"`
	MusicID      int    `json:"musicId"`
	VocalType    string `json:"vocalType"`
	Caption      string `json:"caption"`
	Arranger     string `json:"arranger"`
}

// pjskMusicDifficulty 难度信息
type pjskMusicDifficulty struct {
	ID              int    `json:"id"`
	MusicID         int    `json:"musicId"`
	MusicDifficulty string `json:"musicDifficulty"`
	PlayLevel       int    `json:"playLevel"`
	TotalNoteCount  int    `json:"totalNoteCount"`
}

// pjskEventMusic 活动-音乐关联
type pjskEventMusic struct {
	EventID int `json:"eventId"`
	MusicID int `json:"musicId"`
	Seq     int `json:"seq"`
}

// pjskEventBasic 活动基础信息
type pjskEventBasic struct {
	ID   int    `json:"id"`
	Name string `json:"name"`
}

// musicDetail 模板数据
type musicDetail struct {
	ID           int
	Title        string
	Lyricist     string
	Composer     string
	Arranger     string
	Server       string
	ServerKey    string
	PublishedAt  string
	Jacket       htmltemplate.URL
	Vocalists    []vocalInfo
	Difficulties []difficultyInfo
	Events       []eventInfo
}

type vocalInfo struct {
	VocalType string
	Caption   string
	Arranger  string
}

type difficultyInfo struct {
	Difficulty     string
	PlayLevel      int
	TotalNoteCount int
}

type eventInfo struct {
	ID   int
	Name string
}

func renderMusicError(c *gin.Context, errMsg string) {
	c.HTML(http.StatusOK, "pjsk/music", gin.H{"Error": errMsg})
}

func MusicHandler(c *gin.Context) {
	server := c.DefaultQuery("server", "jp")
	if !validServers[server] {
		renderMusicError(c, "无效的服务器参数，支持: jp, cn, en, tw, kr")
		return
	}

	idStr := c.Query("id")
	if idStr == "" {
		renderMusicError(c, "缺少音乐 ID 参数")
		return
	}
	musicID, err := strconv.Atoi(idStr)
	if err != nil {
		renderMusicError(c, "无效的音乐 ID: "+idStr)
		return
	}

	music, err := findMusic(server, musicID)
	if err != nil {
		renderMusicError(c, err.Error())
		return
	}

	// 封面图
	jacket := downloadMusicJacket(server, music.AssetbundleName)

	// 演唱版本
	vocalists := findMusicVocals(server, musicID)

	// 难度信息
	difficulties := findMusicDifficulties(server, musicID)

	// 关联活动
	events := findMusicEvents(server, musicID)

	d := musicDetail{
		ID:           music.ID,
		Title:        music.Title,
		Lyricist:     music.Lyricist,
		Composer:     music.Composer,
		Arranger:     music.Arranger,
		Server:       serverNames[server],
		ServerKey:    server,
		PublishedAt:  formatMillisTime(music.PublishedAt),
		Jacket:       jacket,
		Vocalists:    vocalists,
		Difficulties: difficulties,
		Events:       events,
	}

	c.HTML(http.StatusOK, "pjsk/music", gin.H{"Music": d})
}

// findMusic 从缓存的 musics.json 中查找音乐
func findMusic(server string, musicID int) (*pjskMusic, error) {
	data, err := ReadCachedJSON(server, "musics.json")
	if err != nil {
		return nil, err
	}

	var musics []pjskMusic
	if err := json.Unmarshal(data, &musics); err != nil {
		return nil, fmt.Errorf("解析音乐数据失败: %w", err)
	}

	for i := range musics {
		if musics[i].ID == musicID {
			return &musics[i], nil
		}
	}
	return nil, fmt.Errorf("未找到音乐 ID: %d (服务器: %s)", musicID, server)
}

// findMusicVocals 从 musicVocals.json 查找演唱版本
func findMusicVocals(server string, musicID int) []vocalInfo {
	data, err := ReadCachedJSON(server, "musicVocals.json")
	if err != nil {
		return nil
	}

	var vocals []pjskMusicVocal
	if err := json.Unmarshal(data, &vocals); err != nil {
		return nil
	}

	var result []vocalInfo
	for i := range vocals {
		if vocals[i].MusicID == musicID {
			result = append(result, vocalInfo{
				VocalType: vocals[i].VocalType,
				Caption:   vocals[i].Caption,
				Arranger:  vocals[i].Arranger,
			})
		}
	}
	return result
}

// findMusicDifficulties 从 musicDifficulties.json 查找难度信息
func findMusicDifficulties(server string, musicID int) []difficultyInfo {
	data, err := ReadCachedJSON(server, "musicDifficulties.json")
	if err != nil {
		return nil
	}

	var diffs []pjskMusicDifficulty
	if err := json.Unmarshal(data, &diffs); err != nil {
		return nil
	}

	var result []difficultyInfo
	for i := range diffs {
		if diffs[i].MusicID == musicID {
			result = append(result, difficultyInfo{
				Difficulty:     strings.ToUpper(diffs[i].MusicDifficulty),
				PlayLevel:      diffs[i].PlayLevel,
				TotalNoteCount: diffs[i].TotalNoteCount,
			})
		}
	}
	return result
}

// findMusicEvents 从 eventMusics.json 和 events.json 查找关联活动
func findMusicEvents(server string, musicID int) []eventInfo {
	// 先获取 eventMusics 关联
	emData, err := ReadCachedJSON(server, "eventMusics.json")
	if err != nil {
		return nil
	}

	var eventMusics []pjskEventMusic
	if err := json.Unmarshal(emData, &eventMusics); err != nil {
		return nil
	}

	var eventIDs []int
	for i := range eventMusics {
		if eventMusics[i].MusicID == musicID {
			eventIDs = append(eventIDs, eventMusics[i].EventID)
		}
	}

	if len(eventIDs) == 0 {
		return nil
	}

	// 获取活动名称
	eventData, err := ReadCachedJSON(server, "events.json")
	if err != nil {
		return nil
	}

	var events []pjskEventBasic
	if err := json.Unmarshal(eventData, &events); err != nil {
		return nil
	}

	eventMap := make(map[int]string)
	for i := range events {
		eventMap[events[i].ID] = events[i].Name
	}

	var result []eventInfo
	for _, eid := range eventIDs {
		if name, ok := eventMap[eid]; ok {
			result = append(result, eventInfo{
				ID:   eid,
				Name: name,
			})
		}
	}
	return result
}
