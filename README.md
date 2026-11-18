<p align="center">
  <h1 align="center">🦀 CRust</h1>
  <p align="center">
    <strong>Blazing-Fast, All-in-One Modern Web Framework & Community CMS in Pure Rust</strong><br>
    초고속 단일 바이너리 올인원 웹 프레임워크 & 커뮤니티 CMS
  </p>
  <p align="center">
    <a href="https://crust.cmake.work"><img src="https://img.shields.io/badge/Live_Demo-crust.cmake.work-4f46e5?style=for-the-badge&logo=firefox" alt="Live Demo"></a>
    <a href="https://crust.cmake.work/page/getting-started"><img src="https://img.shields.io/badge/Docs-Getting_Started-10b981?style=for-the-badge&logo=gitbook" alt="Docs"></a>
    <a href="https://github.com/1clickmake/CRust/blob/main/LICENSE"><img src="https://img.shields.io/badge/License-MIT-blue?style=for-the-badge" alt="MIT License"></a>
    <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Rust-2021_Edition-f97316?style=for-the-badge&logo=rust" alt="Rust"></a>
  </p>
</p>

---

## 🌟 Overview

**CRust** is a high-performance, single-binary web framework and CMS crafted in **Rust (Axum)**. It combines the familiar usability of traditional community boards with the uncompromising speed, memory safety, and fearless concurrency of modern Rust.

No PHP-FPM, no complex runtime configurations, and no required external database setup. Run a single executable and deploy a complete, production-ready community and web portal in milliseconds.

- 🌐 **Live Showcase & Forum:** [https://crust.cmake.work](https://crust.cmake.work)
- 📖 **Getting Started Guide:** [https://crust.cmake.work/page/getting-started](https://crust.cmake.work/page/getting-started)

---

## ✨ Key Features

- ⚡ **Single Binary & Ultra-Low Memory**: Zero-dependency deployment. Runs on ~30MB RAM with blazing sub-millisecond response times even on budget VPS servers.
- 🔍 **SQLite WAL + FTS5 Full-Text Search**: Zero-config auto-seeding on first run. Lightning-fast trigram full-text search across hundreds of thousands of posts.
- 🎨 **Dynamic Theme Engine with Fallback**: Plug-and-play themes (`templates/themes/`). Modify HTML/CSS via FTP or aaPanel without recompiling. Missing files automatically fall back to `default`.
- 📂 **File-Based Page Routing**: Drop `page/about.html` into your theme folder to immediately serve `https://domain.com/page/about` with full theme layout and dark-mode inheritance.
- 🔔 **Real-Time SSE Notifications**: Asynchronous `EventBus` pushing comments, mentions, points, and reports directly to the UI via Server-Sent Events (SSE).
- 🛡️ **Enterprise-Grade Security**: Argon2id password hashing, CSRF token middleware, Ammonia-powered HTML XSS sanitizer, session TTL, and role-based permissions (levels 1–10).
- 🌐 **Built-in i18n & Dynamic Themes**: One-click switching for Korean (KO), English (EN), Japanese (JA), and Chinese (ZH) with automatic system Dark/Light mode detection.
- 📦 **Pluggable Storage**: Local filesystem or S3-compatible cloud storage (AWS S3, Cloudflare R2, MinIO) with automatic WebP conversion and thumbnail caching.
- 🤖 **Snapshot-Safe AI Builder**: Separate `/admin/ai-settings` and `/admin/ai-builder` with directory whitelisting, pre-change atomic snapshots, and one-click instant rollbacks.

---

## 🚀 Quick Start

### 1. Run from Source (Rust)

```bash
# 1. Clone repository
git clone https://github.com/1clickmake/CRust.git
cd CRust

# 2. Copy default configuration
cp config.example.toml config.toml

# 3. Build & launch release binary (automatic DB initialization)
cargo run --release
```

Open `http://127.0.0.1:3000` in your browser.

- **Default Admin Account**: `admin` / `admin1234`
  > ⚠️ *Security Notice: Change the default administrator password immediately after first launch.*

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

---

## 🛠️ Production Deployment (aaPanel / Nginx)

CRust works out of the box behind Nginx, Caddy, or **aaPanel**:

1. **Add Website**: In aaPanel ➔ `[Website]` ➔ `[Add site]`, enter domain (e.g. `crust.cmake.work`), set PHP to `Pure`, Database to `None`.
2. **Reverse Proxy**: Site Settings ➔ `[Reverse Proxy]`, add target `http://127.0.0.1:3000` with proxy directory `/` and sent domain `$host`.
3. **One-Click SSL**: Site Settings ➔ `[SSL]` ➔ select `[Let's Encrypt]` and enable `[Force HTTPS]`.
4. **Daemon Supervisor**: Install `Supervisor` from aaPanel App Store, set command to `/www/wwwroot/crust/target/release/rust_board`.

For a full step-by-step walkthrough with screenshots and anchor navigation, visit our **[CRust Getting Started Guide](https://crust.cmake.work/page/getting-started)**.

---

## 📂 Project Structure

```text
CRust/
├── Cargo.toml            # Rust dependencies (Axum, SQLx, Tokio, MiniJinja)
├── config.example.toml   # Server, database, storage & security settings
├── src/                  # Core Rust source code
│   ├── handlers/         # Web views & API controllers
│   ├── services/         # Business logic (auth, theme, file, SEO, AI)
│   ├── models/           # Data models & SQLx queries
│   └── middlewares/      # CSRF, auth guard, rate limiters
├── templates/            # MiniJinja template files
│   └── themes/           # Pluggable themes (default, success, forum)
│       └── forum/        # Modern forum theme with file-based routing
├── static/               # Global CSS, JS, fonts, and assets
└── uploads/              # Local storage upload directory
```

---

## 📄 License

This project is open-sourced under the **[MIT License](LICENSE)**.

Built with passion by **[Boss & Asamiya 💕](https://crust.cmake.work)**.
