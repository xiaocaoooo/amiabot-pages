package handlers_test

import (
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/gin-gonic/gin"
	"github.com/stretchr/testify/assert"
	"github.com/xiaocaoooo/amiabot-pages/handlers/pjsk"
	"github.com/xiaocaoooo/amiabot-pages/handlers/pixiv"
)

func TestFooterRendering(t *testing.T) {
	gin.SetMode(gin.TestMode)
	r := gin.New()

	// Load actual templates
	r.LoadHTMLFiles(
		"templates/layout.html",
		"templates/logo.html",
		"templates/bilibili/video.html",
		"templates/gallery/duplicate.html",
		"templates/gallery/tags.html",
		"templates/gallery/images.html",
		"templates/pixiv/illust.html",
		"templates/pjsk/event.html",
		"templates/pjsk/card.html",
		"templates/pjsk/music.html",
		"templates/pjsk/profile.html",
		"templates/pjsk/b30.html",
		"templates/query/user.html",
		"templates/query/group.html",
		"templates/status/zeabur.html",
	)

	// Define routes
	r.GET("/pjsk/b30", pjsk.B30Handler)
	r.GET("/pixiv/illust", pixiv.IllustInfoHandler)

	t.Run("PJSK page should have moesekai footer", func(t *testing.T) {
		// Mock necessary environment variables for B30Handler
		// Actually, B30Handler calls fetchSuiteMusicResults which needs PJSK_SUITE_BASEURL
		// We can just mock the handler's behavior or provide dummy envs
		// But since we want to test the TEMPLATE rendering, we can just call the handler
		// with a valid ID and dummy env.
		
		// To avoid network calls in test, we might need to mock the API, 
		// but let's see if we can just check the result of a failing request 
		// (as error pages might also use the layout).
		
		w := httptest.NewRecorder()
		req, _ := http.NewRequest("GET", "/pjsk/b30?id=123", nil)
		r.ServeHTTP(w, req)

		body := w.Body.String()
		// Even if it's an error page, it should use layout.html. 
		// Wait, B30Handler calls renderB30Err which uses c.HTML(..., "pjsk/b30", gin.H{"Error": ...})
		// Let's check if "Powered by Moesekai" is there.
		// Note: the error page still uses the PJSK template, which now should rely on FooterExtra.
		// In renderB30Err, FooterExtra is NOT passed. 
		// Ah! I missed the error pages in my fix!
		
		// Let's check the current result first.
		assert.Contains(t, body, "Powered by Moesekai", "PJSK page should contain moesekai footer")
	})

	t.Run("Pixiv page should NOT have moesekai footer", func(t *testing.T) {
		w := httptest.NewRecorder()
		req, _ := http.NewRequest("GET", "/pixiv/illust?id=123", nil)
		r.ServeHTTP(w, req)

		body := w.Body.String()
		assert.NotContains(t, body, "Powered by Moesekai", "Pixiv page should not contain moesekai footer")
	})
}
