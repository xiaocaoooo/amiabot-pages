package pjsk

import (
	"encoding/json"
	"fmt"
	htmltemplate "html/template"
	"net/http"
	"strconv"

	"github.com/gin-gonic/gin"
)

// pjskGameCharacter 角色基本信息
type pjskGameCharacter struct {
	ID        int    `json:"id"`
	FirstName string `json:"firstName"`
	GivenName string `json:"givenName"`
	Unit      string `json:"unit"`
}

// cardDetail 传给模板的卡面详情
type cardDetail struct {
	ID            int
	CharacterID   int
	CharacterName string
	CharacterUnit string
	Prefix        string
	Rarity        string
	RarityRaw     string
	Attr          string
	AttrRaw       string
	SkillName     string
	FlavorText    string
	ReleaseAt     string
	Server        string
	ServerKey     string

	Thumbnail htmltemplate.URL
	Frame     htmltemplate.URL
	AttrIcon  htmltemplate.URL
	Stars     []int
	StarIcon  htmltemplate.URL

	CardImage      htmltemplate.URL
	CardImageAfter htmltemplate.URL

	HasAfter       bool
	ThumbnailAfter htmltemplate.URL
	FrameAfter     htmltemplate.URL
	StarsAfter     []int
	StarIconAfter  htmltemplate.URL
}

// pjskCardFull 完整卡面 JSON 结构
type pjskCardFull struct {
	ID              int    `json:"id"`
	CharacterID     int    `json:"characterId"`
	CardRarityType  string `json:"cardRarityType"`
	Attr            string `json:"attr"`
	Prefix          string `json:"prefix"`
	AssetbundleName string `json:"assetbundleName"`
	CardSkillName   string `json:"cardSkillName"`
	FlavorText      string `json:"flavorText"`
	ReleaseAt       int64  `json:"releaseAt"`
}

func findCharacter(server string, charID int) *pjskGameCharacter {
	data, err := ReadCachedJSON(server, "gameCharacters.json")
	if err != nil {
		return nil
	}
	var chars []pjskGameCharacter
	if err := json.Unmarshal(data, &chars); err != nil {
		return nil
	}
	for i := range chars {
		if chars[i].ID == charID {
			return &chars[i]
		}
	}
	return nil
}

func findCardFull(server string, cardID int) (*pjskCardFull, error) {
	data, err := ReadCachedJSON(server, "cards.json")
	if err != nil {
		return nil, err
	}
	var cards []pjskCardFull
	if err := json.Unmarshal(data, &cards); err != nil {
		return nil, fmt.Errorf("解析卡面数据失败: %w", err)
	}
	for i := range cards {
		if cards[i].ID == cardID {
			return &cards[i], nil
		}
	}
	return nil, fmt.Errorf("未找到卡面 ID: %d (服务器: %s)", cardID, server)
}

func hasSpecialTraining(rarity string) bool {
	return rarity == "rarity_3" || rarity == "rarity_4" || rarity == "rarity_birthday"
}

func renderCardError(c *gin.Context, errMsg string) {
	c.HTML(http.StatusOK, "pjsk/card", gin.H{"Error": errMsg})
}

func CardHandler(c *gin.Context) {
	server := c.DefaultQuery("server", "jp")
	if !validServers[server] {
		renderCardError(c, "无效的服务器参数，支持: jp, cn, en, tw, kr")
		return
	}

	idStr := c.Query("id")
	if idStr == "" {
		renderCardError(c, "缺少卡面 ID 参数")
		return
	}
	cardID, err := strconv.Atoi(idStr)
	if err != nil {
		renderCardError(c, "无效的卡面 ID: "+idStr)
		return
	}

	card, err := findCardFull(server, cardID)
	if err != nil {
		renderCardError(c, err.Error())
		return
	}

	// 角色信息
	charName := fmt.Sprintf("角色 #%d", card.CharacterID)
	charUnit := ""
	if ch := findCharacter(server, card.CharacterID); ch != nil {
		charName = ch.FirstName + " " + ch.GivenName
		charUnit = ch.Unit
	}

	// 缩略图
	thumb := downloadCardThumbnail(server, card.AssetbundleName, "normal")

	// 大图
	cardImage := downloadCardImage(server, card.AssetbundleName, "normal")

	rarity := card.CardRarityType
	sIcon := starDataURL
	if rarity == "rarity_birthday" {
		sIcon = birthdayDataURL
	}

	d := cardDetail{
		ID:            card.ID,
		CharacterID:   card.CharacterID,
		CharacterName: charName,
		CharacterUnit: unitName(charUnit),
		Prefix:        card.Prefix,
		RarityRaw:     rarity,
		AttrRaw:       card.Attr,
		SkillName:     card.CardSkillName,
		FlavorText:    card.FlavorText,
		ReleaseAt:     formatMillisTime(card.ReleaseAt),
		Server:        serverNames[server],
		ServerKey:     server,
		Thumbnail:     thumb,
		CardImage:     cardImage,
		Frame:         frameDataURLs[rarity],
		AttrIcon:      attrIconDataURLs[card.Attr],
		Stars:         starPositions(rarity),
		StarIcon:      sIcon,
	}

	if name, ok := rarityNames[rarity]; ok {
		d.Rarity = name
	} else {
		d.Rarity = rarity
	}
	if name, ok := attrNames[card.Attr]; ok {
		d.Attr = name
	} else {
		d.Attr = card.Attr
	}

	if hasSpecialTraining(rarity) {
		d.HasAfter = true
		d.ThumbnailAfter = downloadCardThumbnail(server, card.AssetbundleName, "after_training")
		d.CardImageAfter = downloadCardImage(server, card.AssetbundleName, "after_training")
		d.FrameAfter = frameDataURLs[rarity]
		d.StarsAfter = starPositions(rarity)
		d.StarIconAfter = sIcon
	}

	c.HTML(http.StatusOK, "pjsk/card", gin.H{"Card": d})
}
