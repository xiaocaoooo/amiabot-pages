package paramid

import (
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"net/url"
	"os"
	"strconv"
	"strings"

	"github.com/gin-gonic/gin"
	"github.com/redis/go-redis/v9"
)

const (
	// QueryParamName 是用于传递参数存储 ID 的 query 参数名。
	QueryParamName = "param_id"
	// DefaultKeyTemplate 是 Valkey key 的默认模板。
	DefaultKeyTemplate = "amiabot-pages:params:{id}"
)

// Middleware 基于 Valkey 读取 param_id 对应参数并注入 query。
type Middleware struct {
	client      redis.UniversalClient
	keyTemplate string
}

// New 创建参数注入中间件实例。
func New(client redis.UniversalClient, keyTemplate string) (*Middleware, error) {
	if client == nil {
		return nil, errors.New("valkey client 不能为空")
	}
	resolvedTemplate := strings.TrimSpace(keyTemplate)
	if resolvedTemplate == "" {
		resolvedTemplate = DefaultKeyTemplate
	}
	if !strings.Contains(resolvedTemplate, "{id}") {
		return nil, fmt.Errorf("VALKEY_KEY_TEMPLATE 必须包含 {id}: %q", resolvedTemplate)
	}

	return &Middleware{client: client, keyTemplate: resolvedTemplate}, nil
}

// NewFromEnv 从环境变量读取 Valkey 配置并返回中间件。
// 返回 enabled=false 表示未配置 VALKEY_ADDR，功能未启用。
func NewFromEnv() (handler gin.HandlerFunc, enabled bool, err error) {
	addr := strings.TrimSpace(os.Getenv("VALKEY_ADDR"))
	if addr == "" {
		return nil, false, nil
	}

	db := 0
	if rawDB := strings.TrimSpace(os.Getenv("VALKEY_DB")); rawDB != "" {
		parsedDB, parseErr := strconv.Atoi(rawDB)
		if parseErr != nil || parsedDB < 0 {
			return nil, false, fmt.Errorf("VALKEY_DB 非法: %q", rawDB)
		}
		db = parsedDB
	}

	client := redis.NewClient(&redis.Options{
		Addr:     addr,
		Password: os.Getenv("VALKEY_PASSWORD"),
		DB:       db,
	})

	mw, newErr := New(client, os.Getenv("VALKEY_KEY_TEMPLATE"))
	if newErr != nil {
		_ = client.Close()
		return nil, false, newErr
	}

	return mw.Handler(), true, nil
}

// Handler 返回 Gin 中间件处理器。
func (m *Middleware) Handler() gin.HandlerFunc {
	return func(c *gin.Context) {
		query := c.Request.URL.Query()
		if _, ok := query[QueryParamName]; !ok {
			c.Next()
			return
		}

		paramID := strings.TrimSpace(query.Get(QueryParamName))
		if paramID == "" {
			abortBadRequest(c, "param_id 不能为空")
			return
		}

		key := strings.ReplaceAll(m.keyTemplate, "{id}", paramID)
		raw, err := m.client.Get(c.Request.Context(), key).Result()
		if errors.Is(err, redis.Nil) {
			abortBadRequest(c, "param_id 无效或已过期")
			return
		}
		if err != nil {
			c.AbortWithStatusJSON(http.StatusBadGateway, gin.H{"error": "Valkey 不可用: " + err.Error()})
			return
		}

		injected, err := decodeStoredQuery(raw)
		if err != nil {
			abortBadRequest(c, "Valkey 参数格式错误: "+err.Error())
			return
		}

		merged := mergeQueryValues(query, injected)
		c.Request.URL.RawQuery = merged.Encode()
		c.Next()
	}
}

func abortBadRequest(c *gin.Context, msg string) {
	c.AbortWithStatusJSON(http.StatusBadRequest, gin.H{"error": msg})
}

func decodeStoredQuery(raw string) (url.Values, error) {
	dec := json.NewDecoder(strings.NewReader(raw))
	dec.UseNumber()

	payload := make(map[string]any)
	if err := dec.Decode(&payload); err != nil {
		return nil, err
	}

	result := make(url.Values, len(payload))
	for key, value := range payload {
		trimmedKey := strings.TrimSpace(key)
		if trimmedKey == "" {
			return nil, errors.New("参数 key 不能为空")
		}
		if trimmedKey == QueryParamName {
			continue
		}

		values, err := normalizeValue(value)
		if err != nil {
			return nil, fmt.Errorf("参数 %q: %w", trimmedKey, err)
		}
		for _, item := range values {
			result.Add(trimmedKey, item)
		}
	}
	return result, nil
}

func normalizeValue(v any) ([]string, error) {
	if v == nil {
		return nil, nil
	}

	if scalar, ok := normalizeScalar(v); ok {
		return []string{scalar}, nil
	}

	items, ok := v.([]any)
	if !ok {
		if _, isObject := v.(map[string]any); isObject {
			return nil, errors.New("不支持嵌套对象")
		}
		return nil, fmt.Errorf("不支持的参数类型: %T", v)
	}

	result := make([]string, 0, len(items))
	for _, item := range items {
		if item == nil {
			continue
		}
		scalar, ok := normalizeScalar(item)
		if ok {
			result = append(result, scalar)
			continue
		}
		return nil, fmt.Errorf("数组中包含不支持的类型: %T", item)
	}
	return result, nil
}

func normalizeScalar(v any) (string, bool) {
	switch x := v.(type) {
	case string:
		return x, true
	case bool:
		return strconv.FormatBool(x), true
	case json.Number:
		return x.String(), true
	case float64:
		return strconv.FormatFloat(x, 'f', -1, 64), true
	case float32:
		return strconv.FormatFloat(float64(x), 'f', -1, 32), true
	case int:
		return strconv.Itoa(x), true
	case int8:
		return strconv.FormatInt(int64(x), 10), true
	case int16:
		return strconv.FormatInt(int64(x), 10), true
	case int32:
		return strconv.FormatInt(int64(x), 10), true
	case int64:
		return strconv.FormatInt(x, 10), true
	case uint:
		return strconv.FormatUint(uint64(x), 10), true
	case uint8:
		return strconv.FormatUint(uint64(x), 10), true
	case uint16:
		return strconv.FormatUint(uint64(x), 10), true
	case uint32:
		return strconv.FormatUint(uint64(x), 10), true
	case uint64:
		return strconv.FormatUint(x, 10), true
	default:
		return "", false
	}
}

func mergeQueryValues(requestQuery url.Values, injectedQuery url.Values) url.Values {
	merged := make(url.Values, len(requestQuery)+len(injectedQuery))

	for key, values := range requestQuery {
		if key == QueryParamName {
			continue
		}
		merged[key] = append([]string(nil), values...)
	}

	for key, values := range injectedQuery {
		if _, exists := merged[key]; exists {
			continue
		}
		if len(values) == 0 {
			continue
		}
		merged[key] = append([]string(nil), values...)
	}

	return merged
}
