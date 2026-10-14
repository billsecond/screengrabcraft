# Hosting ScreenGrabCraft Editor for the web

`screengrabcraft-web-<version>.zip` (from the GitHub release, or `packaging/web/package.sh`) holds
the editor as a static site in `screengrabcraft-web-<version>/`:

| File | What it is |
|---|---|
| `index.html` | The page. It loads everything through relative URLs. |
| `screengrabcraft-web-<hash>.js` | wasm-bindgen glue (generated, ES module) |
| `screengrabcraft-web-<hash>_bg.wasm` | The editor, compiled to WebAssembly |
| `_headers`, `.htaccess` | Sample header rules for Netlify/Cloudflare Pages and Apache |

There is no server-side code. Upload the folder's contents anywhere that serves static files.

The browser build is the editor alone. It marks up pictures you open, drop or paste, and saves
through the browser's downloads; capturing the screen needs the desktop programs.

## Any path works

All URLs in `index.html` are relative (`public_url = "./"` in `apps/screengrabcraft-web/Trunk.toml`),
so the site works at a domain root (`https://example.com/`), under a prefix
(`https://example.com/tools/screengrabcraft/`) and from a CDN bucket. The asset names carry a
content hash, so they can be cached forever. Only `index.html` needs revalidation.

## Required server settings

- **MIME type:** serve `.wasm` as `application/wasm`. Browsers refuse to stream-compile it under
  any other type, and the editor then loads slowly or not at all. Serve `.js` as
  `text/javascript`. Most hosts already do both. For nginx, check that `mime.types` has
  `application/wasm wasm;`.
- **Compression:** turn on gzip or Brotli for `.wasm`, `.js` and `.html`; WebAssembly compresses
  to about a third of its size. You can also precompress (`brotli -k *.wasm`) and let the server
  send `Content-Encoding: br`.
- **Caching:** `Cache-Control: public, max-age=31536000, immutable` on the hashed `.wasm` and
  `.js` files, and `no-cache` on `index.html`.
- **HTTPS:** the clipboard (pasting a picture, copying the result) only works in a secure
  context, which means `https://` or `http://localhost`.
- **No special isolation headers:** the editor doesn't use `SharedArrayBuffer`, so it doesn't
  need `Cross-Origin-Opener-Policy` or `Cross-Origin-Embedder-Policy`. If your site already sends
  COEP `require-corp`, also send `Cross-Origin-Resource-Policy: same-origin` (or `cross-origin`
  when the files live on a CDN) on the editor's files.

nginx example:

```nginx
location /screengrabcraft/ {
    types { application/wasm wasm; text/javascript js; text/html html; }
    gzip on;
    gzip_types application/wasm text/javascript text/html;
    location ~* \.(wasm|js)$ { add_header Cache-Control "public, max-age=31536000, immutable"; }
    location ~* index\.html$ { add_header Cache-Control "no-cache"; }
}
```

Local test: `python3 -m http.server 8765` inside the folder, then open http://localhost:8765/.

## Embedding in a page (iframe)

```html
<iframe
  src="https://example.com/screengrabcraft/"
  title="ScreenGrabCraft Editor"
  style="width: 100%; height: 720px; border: 0;"
  allow="fullscreen; clipboard-read; clipboard-write"
  allowfullscreen>
</iframe>
```

- The editor fills the iframe and follows its size, so size the iframe and not the editor.
- Keyboard shortcuts go to the iframe after the user clicks into it, as with any embedded app.
- **Cross-origin embeds** work. Preferences are kept in the iframe's `localStorage`. Browsers
  that partition or block third-party storage may forget them between visits, and the editor
  then starts with defaults.
- **Sandboxed iframes** need at least
  `sandbox="allow-scripts allow-same-origin allow-downloads allow-popups"`. Without
  `allow-same-origin` there's no storage. Without `allow-downloads`, Save and Export (browser
  downloads) are blocked.
- Don't send `X-Frame-Options: DENY` or a `frame-ancestors` CSP that excludes the embedding page.
