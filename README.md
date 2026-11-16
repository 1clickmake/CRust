# CRust

Modern Gnuboard-style web community CMS built with **Rust**, **Axum**, **MiniJinja**, and **SQLite** — featuring dynamic themes, real-time SSE notifications, OAuth2, i18n, and a snapshot-safe AI builder.

## ✨ Key Features

- **Boards & Skins**: `basic`, `gallery` (thumbnail cards), and `qna` (answer accept + point rewards) skins, nested comments (`depth`), secret comments, and guest comment passwords.
- **Real-time Notifications**: `EventBus` + Server-Sent Events (SSE) bell badge with instant read/read-all support.
- **Community & Moderation**: 1:1 Memo with configurable point cost, post/comment reporting with duplicate blocking, trash bin & restore, member levels & point logs.
- **Media & Storage**: Automatic image resizing & WebP conversion (preserving animated GIFs), pluggable `local` or S3-compatible (AWS S3 / Cloudflare R2 / MinIO) storage.
- **OAuth2 Social Login**: Admin-configurable Kakao, Naver, Google, GitHub, and Apple login + account linking/unlinking.
- **i18n & Theme Mode**: Korean (`ko`), English (`en`), Japanese (`ja`), and Chinese (`zh`) UI with Dark / Light mode toggle.
- **SEO**: `/robots.txt`, `/sitemap.xml`, `/rss`, and OpenGraph meta tags.
- **Safe AI Builder**: Separate `/admin/ai-settings` (Gemini, OpenAI, Claude, OpenAI-compatible, model listing, reasoning effort `minimal`–`high`) and `/admin/ai-builder` with path whitelist, pre-change snapshots, and one-click rollback.

---

## 🚀 Quick Start

### 1. Run from Source (Rust)

```bash
# 1. Copy example configuration
cp config.example.toml config.toml

# 2. Build & run (migrations and default seed run automatically on startup)
cargo run --release
```

Open `http://127.0.0.1:3000` in your browser.

- **Default Admin Login**: `admin` / `admin1234`
  > ⚠️ **Security Notice:** Change the default admin password immediately after first login on any public server.

---

### 2. Run with Docker

```bash
cp config.example.toml config.toml
# Edit config.toml: set host = "0.0.0.0", dev_mode = false

docker build -t crust .
docker run -d \
  -p 3000:3000 \
  -v $(pwd)/config.toml:/app/config.toml:ro \
  -v $(pwd)/data:/app/data \
  -v $(pwd)/uploads:/app/uploads \
  --name crust \
  crust
```

*(If mounting `/app/data`, set `url = "sqlite:///app/data/rust_board.db?mode=rwc"` in `config.toml` so your SQLite database persists across container restarts.)*

---

## 🔒 Production Checklist

1. Change the default `admin` password (`admin1234`).
2. Set `dev_mode = false` in `config.toml` so templates and translations are cached in memory.
3. Put CRust behind a reverse proxy (Nginx / Caddy / Cloudflare) with HTTPS and set `cookie_secure = true` in `config.toml`.
4. Back up `rust_board.db` and `uploads/` regularly (or use `[storage.s3]` for media).

---

## 📄 License

This project is licensed under the [MIT License](LICENSE).
