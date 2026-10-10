// Package web is the web client (docs/plans/030_web_client.md): plain HTML,
// CSS and JavaScript modules with no build step, embedded in the server and
// served from the origin the API is on, so the login cookie reaches <audio>
// and <img>.
package web

import (
	"bytes"
	"compress/gzip"
	"crypto/sha256"
	"embed"
	"encoding/hex"
	"io/fs"
	"mime"
	"net/http"
	"os"
	"path"
	"strings"
)

//go:embed index.html app.css icon.svg js
var files embed.FS

type file struct {
	body, gz []byte
	etag     string
	ctype    string
}

// Handler serves the client. Every file is read, hashed and gzipped once, at
// start: they are a few hundred kilobytes, and the hash is the ETag that
// lets a reload revalidate in one round trip rather than fetch again.
//
// With DHUN_WEB_DIR set (server/web in a checkout), the files are read from
// there on every request instead, so an edit shows on a reload.
func Handler() http.Handler {
	if dir := os.Getenv("DHUN_WEB_DIR"); dir != "" {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			load(os.DirFS(dir)).ServeHTTP(w, r)
		})
	}
	return load(files)
}

func load(files fs.FS) http.Handler {
	byPath := map[string]*file{}
	err := fs.WalkDir(files, ".", func(p string, d fs.DirEntry, err error) error {
		if err != nil || d.IsDir() {
			return err
		}
		body, err := fs.ReadFile(files, p)
		if err != nil {
			return err
		}
		sum := sha256.Sum256(body)
		var gz bytes.Buffer
		w, _ := gzip.NewWriterLevel(&gz, gzip.BestCompression)
		w.Write(body)
		w.Close()
		ctype := mime.TypeByExtension(path.Ext(p))
		if path.Ext(p) == ".js" {
			// Module scripts need a JavaScript type, whatever the system's MIME table says.
			ctype = "text/javascript; charset=utf-8"
		}
		byPath["/"+p] = &file{body, gz.Bytes(), `"` + hex.EncodeToString(sum[:8]) + `"`, ctype}
		return nil
	})
	if err != nil {
		panic(err) // embedded at build time, or a developer's folder
	}
	byPath["/"] = byPath["/index.html"]

	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		f := byPath[r.URL.Path]
		if f == nil {
			// Pages are addressed by the fragment (#/album/…), so nothing else is a page.
			http.NotFound(w, r)
			return
		}
		h := w.Header()
		// Scripts only from here, so an injected one cannot read the token. Thumbnails are data: URLs.
		h.Set("Content-Security-Policy", "default-src 'self'; img-src 'self' data: blob:; media-src 'self'; "+
			"object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'")
		// Always revalidated: an update to the server is the update to the page.
		h.Set("Cache-Control", "no-cache")
		h.Set("ETag", f.etag)
		h.Set("Content-Type", f.ctype)
		h.Add("Vary", "Accept-Encoding")
		if r.Header.Get("If-None-Match") == f.etag {
			w.WriteHeader(http.StatusNotModified)
			return
		}
		body := f.body
		if strings.Contains(r.Header.Get("Accept-Encoding"), "gzip") {
			h.Set("Content-Encoding", "gzip")
			body = f.gz
		}
		if r.Method == http.MethodHead {
			return
		}
		w.Write(body)
	})
}
