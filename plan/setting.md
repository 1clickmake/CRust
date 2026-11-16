# CRust (Rust Board) 확장성 중심 업그레이드 로드맵 & 진행 현황

> **최종 업데이트:** 2026-11-16  
> **핵심 아키텍처 원칙:** **확장성 최우선 (Extensibility First)** — Trait 추상화, `EventBus` 이벤트 구동, `extra_data` JSON 스키마 확장, 설정/실행 페이지 분리  
> **진행 현황:** **Phase 1 · Phase 3 · UI/다국어 코어 고도화 (완료 ✅)** → **Track A ~ Track E (확장성 중심 로드맵 ⏭️)**

---

## 📌 현재 진행 현황 & 완료 내역

### ✅ 완료된 단계
- **Phase 1 (핵심 프레임워크 1차 구축 완료 & 검증 완료):**
  - Axum + SQLite(WAL + FTS5 Trigram) + MiniJinja 동적 테마 + `CRust_Editor` 통합 파일/이미지 업로드 + `Ammonia` XSS 필터 + CSRF + 영속 세션 + 1~10 동적 레벨 + 원자적 포인트 + 게시판 CRUD + 관리자 대시보드
- **추가 코어 개선 (구현 & 검증 완료):**
  - 게시물 수정 (`/board/{bo_table}/{id}/edit`) 및 비회원 글 작성/수정/삭제 비밀번호(`wr_password` Argon2 해시) + 권한 분리
  - 관리자 게시판 설정 수정 모달 (`/admin/boards` → `POST /admin/boards/update`)
- **Phase 3 전체 완료 (`3-1` ~ `3-4` 구현 & E2E 검증 완료):**
  - `[x]` **3-1. 기본환경설정 (`/admin/settings`):** 홈페이지 기본설정, 전역 포인트 정책, 게시판/도배·금지어 설정, 회원가입/약관 설정, IP 접근 제어
  - `[x]` **3-2. 휴지통 관리 (`/admin/trash`):** Soft Delete 게시글/댓글 복원, 영구 삭제(디스크 파일 포함), 휴지통 비우기
  - `[x]` **3-3. DB 원클릭 백업 (`/admin/backup`):** 무중단 SQLite `VACUUM INTO` 스냅샷 다운로드 + DB/WAL/레코드 현황
  - `[x]` **3-4. 마이페이지 고도화 (`/member/profile`):** 닉네임·이메일 수정, 비밀번호 변경, 내 글/댓글 모아보기, 포인트 내역 필터
- **UI · UX · 다국어(i18n) 코어 고도화 (완료 & 검증 완료):**
  - `CRust` / `CRust_Editor` 공식 브랜딩 정비 및 `success` 웰컴 랜딩 테마 구축(`intro.png` 배경, `modern_dark` 제거)
  - 전 테마 & 관리자 전체 10개 페이지 **Light / Dark 모드 전환** 및 **4개국어(`ko`, `en`, `ja`, `zh`) 실시간 다국어(i18n)** 완비
  - **DB 시스템 메시지 다국어 입력:** `/admin/settings`의 기본 언어(`default_lang`)에 맞춰 `point_logs`, `notifications` 문구를 해당 언어로 DB 기록 (`tr_sys` + `get_site_default_lang`)
  - **포인트 천 단위 콤마(`,`) 기본 적용:** MiniJinja `comma` 필터 적용
  - **목록 일괄 선택 / 선택 삭제:** 휴지통, 파일센터, 회원관리, 게시판 목록 전체선택 & 일괄처리 지원
  - **관리자 상단 고정 헤더:** `.admin-topbar`에 `사이트 바로가기` 및 `로그아웃` 버튼 상시 고정

---

## 🏗️ 확장성 최우선 설계 & 개발 효율화 5대 원칙

1. **이벤트 기반 결합 (`EventBus` → Listener):**
   - 새 기능(대댓글, 쪽지, 신고, QnA 채택, OAuth 가입 등)을 추가할 때 기존 서비스에 알림/포인트/웹훅 코드를 직접 박지 않고 `DomainEvent`를 발행하여 리스너가 비동기 처리하도록 확장.
2. **Trait 기반 플러거블 어댑터:**
   - 스토리지(`StorageBackend`), 소셜 로그인(`OAuthProvider`), AI 제공자(`AiProvider`)를 모두 Trait + 레지스트리 패턴으로 구현해 신규 드라이버/프로바이더 추가 시 코어 수정 최소화.
3. **비(非)마이그레이션 확장 & 의존성 분리 빌드:**
   - 외부 크레이트 추가 없이 `extra_data`(JSON)와 기존 스키마로 끝나는 기능을 앞단(Track A~B)에 배치해 빠른 증분 빌드(20초대) 유지.
   - `reqwest` / 이미지 처리 등 무거운 크레이트 도입 작업은 후반부(Track C~E)로 묶어 한 번의 의존성 추가로 공유.
4. **"설정 페이지 vs 실행 페이지" 및 전용 도메인 분리:**
   - OAuth2는 `/admin/settings`에서 분리해 **전용 `/admin/oauth`**로 구축(향후 GitHub · Apple `.p8` 키 등 다양한 Provider 필드 수용).
   - AI 빌더도 **설정(`/admin/ai-settings`)**과 **실행 콘솔(`/admin/ai-builder`)**을 분리해 보안·백업·권한 정책 관리 강화.
5. **완성형 정의(Definition of Done):**
   - 모든 신규 페이지/스킨 작성 시 **4개국어(`locales/*.json`) + Light/Dark CSS 변수 + 콤마/일괄처리 + E2E 검증 스크립트**를 한 세트로 완료해 재작업(Rework) 차단.

---

## 🗺️ 향후 업그레이드 로드맵 (Track A → Track E)

### ✅ Track A. 확장 뼈대 & 커뮤니티 코어 *(신규 Crate 0개 · DB 변경 0회 — E2E 검증 완료)*

#### [x] A-1. `EventBus` 리스너 활성화 & 헤더 실시간 알림 벨 (🔔) — 완료 (`listener.rs`, `notification_api.rs`, `EventSource SSE` E2E 검증 완료)
- **확장 포인트:** `EventBus` 구독 백그라운드 워커를 띄워 향후 모든 도메인 이벤트(대댓글, 쪽지, 신고, 채택, OAuth 등)를 단일 파이프라인으로 알림/로그 처리.
- **기능:** `notification_service`에 읽음 처리(`mark_read`, `mark_all_read`) API 추가 + 전 테마 헤더 알림 벨 아이콘(안 읽은 뱃지, 최근 알림 드롭다운, 바로가기).

#### [x] A-2. 댓글 시스템 고도화 (계층형 · 권한 · 비밀댓글) — 완료 (`build_comment_views`, 수정/삭제/비밀댓글·비회원 Argon2 검증 E2E 완료)
- **확장 포인트:** 이미 존재하는 `comments.parent_id`, `is_secret`, `co_password` 컬럼 활용(스키마 무변경).
- **기능:** 대댓글(Reply) 트리 렌더링, 댓글 수정/삭제(비회원 댓글 비밀번호 Argon2 검증 포함), 비밀댓글(작성자·원글자·관리자만 열람), `CommentReplied` 이벤트 연동.

#### [x] A-3. 확장형 게시판 스킨 시스템 (`gallery`, `qna` + 스킨 규약) — 완료 (`list_available_skins`, `PostListItem`, `gallery`/`qna` 스킨·답변 채택 E2E 완료)
- **확장 포인트:** `bo_skin` 리졸버 표준화 및 `posts.extra_data` / `boards.extra_data` JSON 규약 정립 → 향후 새 스킨(및 AI 생성 스킨)이 Rust 재컴파일 없이 추가 가능한 구조 확보.
- **기능:**
  - `gallery`: 본문 첫 이미지 또는 `post_files` 기반 썸네일 카드형 목록 스킨.
  - `qna`: `extra_data` 기반 질문 상태/채택 댓글 연동 + 채택 시 추가 포인트 지급 및 알림 이벤트 발행.

#### [x] A-4. SEO · 피드 · 커스텀 페이지 확장 기반 — 완료 (`seo_service.rs`, `/robots.txt`, `/sitemap.xml`, `/rss`, OpenGraph E2E 완료)
- **확장 포인트:** 게시판 목록뿐 아니라 향후 추가될 정적/커스텀 페이지까지 자동 포함할 수 있는 URL/메타 공급자 구조로 설계.
- **기능:** `/robots.txt`, `/sitemap.xml`, `/rss` 자동 생성 및 게시글 상세 OpenGraph(`og:title`, `og:description`, `og:image`) 메타태그 자동 삽입.

---

### ✅ Track B. 회원 소통 & 운영 안전장치 *(내부 DB 확장 1회: `memos`, `reports` — E2E 검증 완료)*

#### [x] B-1. 쪽지(1:1 DM) & 신고 시스템 — 완료 (`memo.rs`, `memo_service.rs`, `report_service.rs`, `/admin/reports` E2E 검증 완료)
- **확장 포인트:** `/admin/settings`의 `memo_send_point` 차감 정책 + `EventBus` 알림 연동, 신고 대상(`target_type`: `post` / `comment` / `member`) 다형성 구조로 향후 확장 수용.
- **기능:** 회원 간 1:1 쪽지 송수신함(발신/수신/보관/삭제), 게시글·댓글 신고 접수 및 관리자 검토 패널(`/admin/reports`).

#### [x] B-2. 에디터 임시저장 (`CRust_Editor` Draft) — 완료 (`editor-draft.js` write/edit E2E 검증 완료)
- **확장 포인트:** `bo_table` + `post_id` 키 네임스페이스 기반 `localStorage` 자동 임시저장/복구 프롬프트 (글쓰기·수정 공통 어댑터).

---

### ✅ Track C. 미디어 파이프라인 & 클라우드 스토리지 *(크레이트 도입 1차 — E2E & S3 유닛 검증 완료)*

#### [x] C-1. 서버 측 이미지 리사이즈 & WebP 경량화 옵션 — 완료 (`image_processor.rs`, `/admin/settings` 옵션, `/uploads/{*key}` E2E 검증 완료)
- **확장 포인트:** 업로드 파이프라인 미들웨어/프로세서 추상화로 포맷·최대 해상도 설정 연동 (`/admin/settings` 또는 미디어 옵션).

#### [x] C-2. `StorageBackend` 확장 — `S3Storage` (AWS S3 / Cloudflare R2 / MinIO) — 완료 (`storage/s3.rs` SigV4 + `test_s3_storage_put_get_delete_sigv4` 통과)
- **확장 포인트:** 기존 `StorageBackend` trait(`put`, `local_path`, `get`, `delete`)에 `S3Storage` 구현체 추가 및 `AppState` 동적 드라이버 스위칭 (`config.toml` + `post_files.storage_driver` 기반 무중단 전환).

---

### ✅ Track D. 플러거블 소셜 인증 (OAuth2) — 전용 관리 페이지 분리 *(E2E 검증 완료)*

#### [x] D-1. `OAuthProvider` Trait & 다중 계정 연동 코어 + 전용 `/admin/oauth` — 완료 (`oauth/mod.rs`, `oauth/providers.rs`, `/admin/oauth`, `/auth/oauth/{provider}` E2E 완료)
- **확장 포인트:**
  - `/admin/settings`에서 완전 분리된 **전용 관리자 메뉴 `/admin/oauth`** 구축.
  - DB 구조: `oauth_providers` (Provider별 ON/OFF, `client_id`, `client_secret`, 확장 설정 `extra_config` JSON) + `member_oauth_links` (1명의 회원이 여러 소셜 계정 연결 가능).
  - Trait 기반 라우팅 `/auth/oauth/{provider}` → `/auth/oauth/{provider}/callback`.
- **구현 범위:**
  - **1차 지원:** 카카오(Kakao), 네이버(Naver), 구글(Google).
  - **확장 수용 설계:** GitHub, **Apple**(`Team ID`, `Key ID`, `.p8 Private Key` 등 `extra_config` JSON 수용), Discord 등을 코어 수정 없이 Provider 구현체만 추가해 즉시 활성화 가능하도록 설계.

---

### ✅ Track E (피날레). 관리자 내장 AI 테마 & 운영 빌더 *(설정 / 실행 분리 — E2E 검증 완료)*

> *모든 테마 규격·스킨(`basic`/`gallery`/`qna`)·다국어·커스텀 페이지·OAuth/스토리지 구조가 완성된 뒤 최종 레이어로 구현하여 전체 시스템을 안전하게 제어.*

#### [x] E-1. AI 빌더 설정 및 안전 정책 페이지 (`/admin/ai-settings`) — 완료 (`ai/mod.rs`, `ai/providers.rs`, API 키 마스킹 & 화이트리스트 검증 완료)
- **확장 포인트:** `AiProvider` trait (Gemini, OpenAI, Claude, 로컬 OpenAI 호환 엔드포인트) 지원.
- **기능:** API 키/모델 선택/프롬프트 한도, **파일 접근 화이트리스트**(`templates/themes/`, `locales/` 등), DB 설정 변경 권한 범위, 자동 백업 보관 개수 관리.

#### [x] E-2. AI 테마 & 운영 빌더 실행 콘솔 (`/admin/ai-builder`) — 완료 (Mock AI 생성·트래버설 차단·테마/페이지 적용·원클릭 롤백 E2E 완료)
- **컴파일/재시작 없이 즉시 실행되는 영역 (안전 화이트리스트 기반):**
  - 새 디자인 테마 폴더 생성(`templates/themes/{name}/theme.toml`, CSS, HTML 등) 및 기존 스킨·다국어 수정
  - 정적/커스텀 페이지 생성 및 SEO 사이트맵 자동 연동
  - 게시판/포인트/레벨/환경설정 DB 설정 변경
- **안전장치:** Path Traversal 완전 차단, 실행 전 자동 스냅샷 백업 생성, 관리자 미리보기 및 **원클릭 롤백**.

---

## 🚀 운영 & 배포 자동화 전략 (VPS 호스팅 기준)
1. **비(非)컴파일 확장 (재시작 0회):**
   - 화면/디자인/테마/다국어 변경은 `templates/themes/` 및 `locales/` 수정만으로 확장(또는 리로드) 가능하도록 설계.
   - 신규 입력 항목 확장은 `extra_data` (JSON) 컬럼을 활용해 DB 스키마 변경 없이 확장.
2. **코어/DB 테이블 변경 시 자동 재시작 배포:**
   - Linux VPS (`systemd` `Restart=always` + Nginx 리버스 프록시) 환경에서 GitHub Actions `git push` 기반 무중단 빌드·배포 및 자동 마이그레이션 실행.
