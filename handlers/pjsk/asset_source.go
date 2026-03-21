package pjsk

import (
	htmltemplate "html/template"
	"log"
	"os"
	"strings"
	"sync"
	"time"

	"github.com/xiaocaoooo/amiabot-pages/pkg/imgcache"
)

type sekaiAssetSource string

const (
	sekaiAssetSnowy  sekaiAssetSource = "snowy"
	sekaiAssetUni    sekaiAssetSource = "uni"
	sekaiAssetHaruki sekaiAssetSource = "haruki"

	assetRetryRounds = 2
)

var (
	defaultSekaiAssetSources = []sekaiAssetSource{
		sekaiAssetSnowy,
		sekaiAssetUni,
		sekaiAssetHaruki,
	}

	assetLabelPathTemplates = map[string][]string{
		"event:background": {
			"ondemand/event/{assetbundle}/screen/bg.png",
			"ondemand/event/{assetbundle}/screen/bg.webp",
			"event/{assetbundle}/screen/bg.png",
			"event/{assetbundle}/screen/bg.webp",
		},
		"event:logo": {
			"ondemand/event/{assetbundle}/logo/logo.png",
			"ondemand/event/{assetbundle}/logo/logo.webp",
			"event/{assetbundle}/logo/logo.png",
			"event/{assetbundle}/logo/logo.webp",
		},
		"event:banner": {
			"ondemand/event_story/{assetbundle}/screen_image/banner_event_story.png",
			"ondemand/event_story/{assetbundle}/screen_image/banner_event_story.webp",
			"event_story/{assetbundle}/screen_image/banner_event_story.png",
			"event_story/{assetbundle}/screen_image/banner_event_story.webp",
			"ondemand/event/{assetbundle}/logo/logo.png",
			"ondemand/event/{assetbundle}/logo/logo.webp",
			"event/{assetbundle}/logo/logo.png",
			"event/{assetbundle}/logo/logo.webp",
			"ondemand/event/{assetbundle}/screen/bg.png",
			"ondemand/event/{assetbundle}/screen/bg.webp",
			"event/{assetbundle}/screen/bg.png",
			"event/{assetbundle}/screen/bg.webp",
			"ondemand/home/banner/{assetbundle}/{assetbundle}.png",
			"ondemand/home/banner/{assetbundle}/{assetbundle}.webp",
			"home/banner/{assetbundle}/{assetbundle}.png",
			"home/banner/{assetbundle}/{assetbundle}.webp",
		},
		"card:thumbnail": {
			"startapp/thumbnail/chara/{assetbundle}_{status}.png",
			"startapp/thumbnail/chara/{assetbundle}_{status}.webp",
			"thumbnail/chara/{assetbundle}_{status}.png",
			"thumbnail/chara/{assetbundle}_{status}.webp",
		},
		"card:image": {
			"startapp/character/member/{assetbundle}/{card_file}.png",
			"startapp/character/member/{assetbundle}/{card_file}.webp",
			"character/member/{assetbundle}/{card_file}.png",
			"character/member/{assetbundle}/{card_file}.webp",
		},
		"music:jacket": {
			"startapp/music/jacket/{assetbundle}/{assetbundle}.png",
			"startapp/music/jacket/{assetbundle}/{assetbundle}.webp",
			"music/jacket/{assetbundle}/{assetbundle}.png",
			"music/jacket/{assetbundle}/{assetbundle}.webp",
		},
	}

	sekaiAssetsOnce sync.Once
	sekaiAssetsList []sekaiAssetSource

	labelURLCacheMu sync.RWMutex
	labelURLCache   = make(map[string]string)
)

func configuredSekaiAssetSources() []sekaiAssetSource {
	sekaiAssetsOnce.Do(func() {
		sekaiAssetsList = parseSekaiAssetSources(os.Getenv("SEKAI_ASSET"))
	})
	return sekaiAssetsList
}

func parseSekaiAssetSources(raw string) []sekaiAssetSource {
	raw = strings.TrimSpace(raw)
	if raw == "" {
		return append([]sekaiAssetSource(nil), defaultSekaiAssetSources...)
	}

	var sources []sekaiAssetSource
	seen := make(map[sekaiAssetSource]struct{})
	for _, token := range strings.Split(raw, ",") {
		token = strings.TrimSpace(strings.ToLower(token))
		if token == "" {
			continue
		}

		var source sekaiAssetSource
		switch token {
		case string(sekaiAssetSnowy), "snowyassets":
			source = sekaiAssetSnowy
		case string(sekaiAssetUni):
			source = sekaiAssetUni
		case string(sekaiAssetHaruki):
			source = sekaiAssetHaruki
		default:
			log.Printf("[pjsk] 忽略无效 SEKAI_ASSET 配置项: %q (支持: snowy, uni, haruki)", token)
			continue
		}

		if _, ok := seen[source]; ok {
			continue
		}
		seen[source] = struct{}{}
		sources = append(sources, source)
	}

	if len(sources) == 0 {
		log.Printf("[pjsk] SEKAI_ASSET=%q 未解析到有效项，回退默认值: snowy,uni,haruki", raw)
		return append([]sekaiAssetSource(nil), defaultSekaiAssetSources...)
	}
	return sources
}

func assetBaseCandidates(source sekaiAssetSource, server string) []string {
	switch source {
	case sekaiAssetSnowy:
		if server == "cn" {
			return []string{
				"https://snowyassets.exmeaning.com/cn",
				"https://snowyassets.exmeaning.com",
			}
		}
		return []string{"https://snowyassets.exmeaning.com"}
	case sekaiAssetUni:
		return []string{"https://assets.unipjsk.com"}
	case sekaiAssetHaruki:
		if server == "cn" {
			return []string{
				"https://sekai-assets-bdf29c81.seiunx.net/cn-assets",
				"https://sekai-assets-bdf29c81.seiunx.net/jp-assets",
			}
		}
		return []string{"https://sekai-assets-bdf29c81.seiunx.net/jp-assets"}
	default:
		return nil
	}
}

func joinAssetURL(base, path string) string {
	return strings.TrimRight(base, "/") + "/" + strings.TrimLeft(path, "/")
}

func buildAssetCandidates(server string, relativePaths []string) []string {
	var urls []string
	seen := make(map[string]struct{})

	for _, source := range configuredSekaiAssetSources() {
		bases := assetBaseCandidates(source, server)
		for _, base := range bases {
			for _, relativePath := range relativePaths {
				url := joinAssetURL(base, relativePath)
				if _, ok := seen[url]; ok {
					continue
				}
				seen[url] = struct{}{}
				urls = append(urls, url)
			}
		}
	}

	return urls
}

func labelCacheKey(server, label string) string {
	return server + ":" + label
}

func cachedAssetURL(server, label string) string {
	labelURLCacheMu.RLock()
	defer labelURLCacheMu.RUnlock()
	return labelURLCache[labelCacheKey(server, label)]
}

func updateCachedAssetURL(server, label, url string) {
	if label == "" || url == "" {
		return
	}
	labelURLCacheMu.Lock()
	labelURLCache[labelCacheKey(server, label)] = url
	labelURLCacheMu.Unlock()
}

func prioritizeAssetCandidates(candidates []string, preferred string) []string {
	if preferred == "" {
		return candidates
	}

	ordered := make([]string, 0, len(candidates)+1)
	seen := make(map[string]struct{}, len(candidates)+1)
	ordered = append(ordered, preferred)
	seen[preferred] = struct{}{}

	for _, candidate := range candidates {
		if _, ok := seen[candidate]; ok {
			continue
		}
		seen[candidate] = struct{}{}
		ordered = append(ordered, candidate)
	}

	return ordered
}

func downloadAssetWithFallback(server, label string, relativePaths []string) htmltemplate.URL {
	candidates := buildAssetCandidates(server, relativePaths)
	if len(candidates) == 0 {
		log.Printf("[pjsk] 资源候选地址为空: %s", label)
		return htmltemplate.URL("")
	}

	candidates = prioritizeAssetCandidates(candidates, cachedAssetURL(server, label))

	for round := 0; round < assetRetryRounds; round++ {
		for _, candidate := range candidates {
			if dataURL := imgcache.Default.Download(candidate, -1, nil); dataURL != "" {
				updateCachedAssetURL(server, label, candidate)
				return dataURL
			}
		}

		if round+1 < assetRetryRounds {
			time.Sleep(time.Duration(round+1) * 300 * time.Millisecond)
		}
	}

	log.Printf("[pjsk] 资源下载失败: %s (server=%s, 尝试地址数=%d)", label, server, len(candidates))
	return htmltemplate.URL("")
}

func buildRelativePathsByLabel(label string) (string, []string) {
	parts := strings.SplitN(strings.TrimSpace(label), ":", 4)
	if len(parts) < 3 {
		return "", nil
	}

	category := strings.TrimSpace(parts[0])
	kind := strings.TrimSpace(parts[1])
	assetbundleName := strings.TrimSpace(parts[2])
	if category == "" || kind == "" || assetbundleName == "" {
		return "", nil
	}

	labelType := category + ":" + kind
	templates, ok := assetLabelPathTemplates[labelType]
	if !ok {
		return "", nil
	}

	status := ""
	if len(parts) == 4 {
		status = strings.TrimSpace(parts[3])
	}
	switch labelType {
	case "card:thumbnail":
		if status == "" {
			status = "normal"
		}
	case "card:image":
		status = strings.TrimPrefix(status, "card_")
		if status == "" {
			status = "normal"
		}
	}

	cardFile := ""
	if labelType == "card:image" {
		cardFile = "card_" + status
	}

	replacer := strings.NewReplacer(
		"{assetbundle}", assetbundleName,
		"{status}", status,
		"{card_file}", cardFile,
	)

	relativePaths := make([]string, 0, len(templates))
	for _, tmpl := range templates {
		relativePaths = append(relativePaths, replacer.Replace(tmpl))
	}

	normalizedLabel := labelType + ":" + assetbundleName
	if labelType == "card:thumbnail" || labelType == "card:image" {
		normalizedLabel += ":" + status
	}
	return normalizedLabel, relativePaths
}

func downloadAssetByLabel(server, label string) htmltemplate.URL {
	normalizedLabel, relativePaths := buildRelativePathsByLabel(label)
	if normalizedLabel == "" || len(relativePaths) == 0 {
		log.Printf("[pjsk] 不支持的资源 label: %q", label)
		return htmltemplate.URL("")
	}
	return downloadAssetWithFallback(server, normalizedLabel, relativePaths)
}
