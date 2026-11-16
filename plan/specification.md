# Rust Board (그누보드형 모던 웹 프레임워크) 종합 설계 명세서

> **문서 버전:** v1.1.0 (보안·확장성·운영 안정성 및 실제 에디터 코드 검수 반영)  
> **작성자:** 아사미야 (Asamiya) 💕  
> **프로젝트 루트:** `c:\Users\hades\OneDrive\Desktop\작업폴더\rust_board`  
> **기반 기술:** Rust (Axum, Tokio, SQLx/SeaORM, MiniJinja, Ammonia, Argon2), HTML5, HTMX, Asamiya-Editor  

---

## 1. 프로젝트 개요 (Overview)
본 프로젝트는 대한민국 대표 커뮤니티 솔루션인 **그누보드5(Gnuboard5)의 핵심 구조와 사용성(게시판, 회원, 레벨, 포인트, 권한 체계, 스킨, 여분 필드)**을 현대적인 **Rust 생태계**로 재해석하여 구현하는 고성능 단일 바이너리 웹 프레임워크입니다.

### 핵심 목표
1. **단일 바이너리 & 제로 의존성 배포:** Nginx, PHP-FPM, 복잡한 런타임 설치 없이 실행 파일 하나로 구동 (내장 SQLite3 + FTS5 기본, PostgreSQL/MySQL 확장 가능).
2. **Asamiya-Editor 기본 탑재 & 확장:** 자체 텍스트 에디터(`Asamiya-Editor`)의 실제 JS 구조와 100% 호환 연동하며, 인라인 파일 첨부 카드 및 선택적 PRO AI 초안 생성 API까지 지원.
3. **무재컴파일 스킨/템플릿 시스템:** `MiniJinja`를 활용하여 런타임에 HTML 스킨을 자유롭게 교체 및 수정 (개발 환경 핫 리로딩, 운영 환경 메모리 캐싱).
4. **철저한 포인트 & 레벨 경제 시스템:** 1~10 레벨 체계, 동적 등급 관리, 원자적(Atomic) DB 트랜잭션 기반 포인트 제어.
5. **대규모 확장이 가능한 프레임워크 코어:** `EventBus` 훅 시스템, `StorageBackend` 추상화, JSON 여분 필드(`extra_data`) 지원.

---

## 2. 전체 시스템 아키텍처 및 폴더 구조

```
rust_board/
├── Cargo.toml                       # Rust 의존성 및 패키지 설정
├── config.toml                      # 사이트 환경설정 (포트, DB URL, 스토리지, 보안 키)
├── plan/                            # [문서] 기획, 구조, DB 설계 문서
│   ├── README.md
│   ├── specification.md             # 본 설계 명세서 (v1.1.0)
│   └── database_schema.sql          # SQLite 3 표준 DDL 및 FTS5 스키마
├── uploads/                         # 로컬 스토리지 기본 저장소 (Git 제외)
│   ├── editor/                      # 에디터 인라인 이미지 (YYYYMM 단위)
│   └── board/                       # 게시판 첨부파일 (bo_table/YYYYMM 단위)
├── static/                          # 정적 에셋 서빙 (/static/...)
│   ├── editor/
│   │   └── asamiya/                 # Asamiya-Editor JS, CSS (인라인 파일 카드 확장 포함)
│   ├── css/                         # 공통 스타일시트
│   ├── js/                          # 공통 자바스크립트 (HTMX, 드롭존 스크립트)
│   └── images/                      # 사이트 정적 이미지 및 로고
├── templates/                       # MiniJinja HTML 템플릿 (다중 테마 & 스킨)
│   ├── admin/                       # 관리자 대시보드 뷰
│   │   ├── layout.html
│   │   ├── index.html               # 메인 대시보드 (통계 위젯)
│   │   ├── themes.html              # [신규] 디자인 테마 목록, 미리보기 및 원클릭 적용
│   │   ├── members.html             # 회원 목록 및 권한/레벨/차단 관리
│   │   ├── member_levels.html       # 동적 레벨 칭호/뱃지/자동승급 조건 설정
│   │   ├── boards.html              # 게시판 생성 및 권한/포인트/여분필드 설정
│   │   ├── files.html               # 미디어 & 첨부파일 통합 관리 센터
│   │   └── points.html              # 포인트 변동 내역 및 수동 지급/차감
│   └── themes/                      # 사이트 디자인 테마 디렉토리
│       ├── default/                 # 기본(Fallback) 디자인 테마
│       │   ├── theme.toml           # 테마 메타정보 (이름, 제작자, 버전, 썸네일)
│       │   ├── assets/              # 테마 전용 CSS, JS, 이미지 (/theme-assets/default/...)
│       │   ├── layout.html          # 기본 레이아웃 (CSRF 메타태그, 알림 뱃지 포함)
│       │   ├── index.html           # 메인 페이지 (최신글 위젯 등)
│       │   ├── board/
│       │   │   └── basic/           # 게시판 기본 스킨 (list, view, write)
│       │   │       ├── list.html    # 게시판 목록 (첨부 아이콘, FTS5 검색)
│       │   │       ├── view.html    # 게시글 읽기, 인라인 파일 카드 & 계층형 댓글
│       │   │       └── write.html   # 글쓰기/수정 (Asamiya-Editor & 스마트 드롭존)
│       │   └── member/
│       │       ├── login.html       # 로그인
│       │       ├── register.html    # 회원가입
│       │       └── profile.html     # 마이페이지, 포인트 내역, 알림 목록
│       └── modern_dark/             # (예시) 추가 디자인 테마 - 변경하고 싶은 파일만 오버라이드 가능
│           ├── theme.toml
│           ├── assets/
│           └── layout.html
└── src/                             # Rust 백엔드 소스코드
    ├── main.rs                      # 서버 진입점, 라우터, 백그라운드 워커 기동
    ├── config.rs                    # 설정 파일(config.toml) 파서
    ├── db.rs                        # DB 커넥션 풀 및 마이그레이션 초기화
    ├── error.rs                     # 커스텀 에러 핸들러 및 응답 래퍼
    ├── state.rs                     # 앱 전역 공유 상태 (AppState, active_theme 캐시)
    ├── models/                      # DB 엔티티 및 DTO
    │   ├── member.rs
    │   ├── board.rs
    │   ├── post.rs
    │   ├── file.rs
    │   ├── comment.rs
    │   ├── point.rs
    │   └── notification.rs
    ├── storage/                     # [확장성] 스토리지 드라이버 추상화 계층
    │   ├── mod.rs                   # StorageBackend Trait 정의
    │   ├── local.rs                 # 로컬 디스크 스토리지 구현체
    │   └── s3.rs                    # (향후 확장) AWS S3 / Cloudflare R2 구현체
    ├── events/                      # [확장성] 이벤트 훅 & 플러그인 버스
    │   ├── mod.rs                   # EventBus & Hook 정의 (PostCreated, MemberJoined 등)
    │   └── plugins/                 # 내장/스크립트 플러그인 어댑터
    ├── services/                    # 비즈니스 로직 계층
    │   ├── auth_service.rs          # Argon2id 인증, 영속 세션, 1일 1회 출석 포인트
    │   ├── point_service.rs         # 원자적(Atomic) 포인트 차감/적립/로그 트랜잭션
    │   ├── board_service.rs         # 글 작성/조회/권한 검사, Ammonia XSS 필터링
    │   ├── level_service.rs         # 동적 등급 및 자동 승급 판별 로직
    │   ├── file_service.rs          # 2단계 파일 생명주기, 본문 동기화, 고아 파일 GC
    │   ├── theme_service.rs         # [신규] 테마 스캔, 런타임 테마 전환, Fallback 리졸버
    │   └── notification_service.rs  # 댓글/레벨업/포인트 알림 생성 및 조회
    ├── handlers/                    # HTTP 요청 핸들러 (컨트롤러)
    │   ├── web/                     # SSR 템플릿 렌더링 핸들러
    │   │   ├── home.rs
    │   │   ├── board.rs
    │   │   ├── member.rs
    │   │   └── admin.rs
    │   └── api/                     # JSON / AJAX / 파일 스트리밍 엔드포인트
    │       ├── editor_upload.rs     # Asamiya-Editor 이미지 & 파일 업로드 API
    │       ├── download.rs          # 보안 다운로드 프록시 (/api/download/:file_uuid)
    │       └── board_api.rs         # 추천, 비동기 댓글 등록 등
    └── middlewares/                 # 요청 보안/검증 미들웨어
        ├── auth.rs                  # DB 세션 검증 (차단 회원 즉시 로그아웃)
        ├── csrf.rs                  # CSRF 토큰 생성 및 검증 미들웨어
        ├── permission.rs            # 레벨(1~10) 기반 접근 제어 Guard
        └── rate_limit.rs            # 비정상 요청/업로드 도배/DDoS 방어
```

---

## 3. 핵심 모듈별 기능 및 보안/확장 설계

### 3.1 Asamiya-Editor 실제 코드 검수 기반 연동 & 스마트 파일 파이프라인
* **원본 에디터 경로:** `C:\Users\hades\OneDrive\Desktop\작업폴더\Asamiya-Editor`
* **에셋 배치:** `asamiya-editor.js`, `asamiya-editor.css`를 `/static/editor/asamiya/`로 복사하고, 파일 첨부 카드 삽입(`handleFileUpload`) 및 드래그앤드롭 기능을 확장합니다.
* **실제 에디터 초기화 규격 (`asamiya-editor.js` 코드 검증 반영):**
  > ⚠️ **주의:** 실제 `asamiya-editor.js`는 생성자 옵션이 아니라 **전역 변수(`asamiyaEditorUploadUrl`, PRO 버전의 경우 `asamiyaEditorAiUrl`)**를 참조합니다. 전역 변수가 없으면 Base64 인라인으로 폴백되어 DB가 비대해지므로 반드시 아래와 같이 전역 URL을 선언해야 합니다.
  ```html
  <!-- 숨겨진 textarea (실제 폼 데이터 전송용) -->
  <textarea id="wr_content" name="wr_content" style="display:none;">{{ post.wr_content | default('') }}</textarea>
  <!-- Asamiya Editor 컨테이너 -->
  <div id="asamiya-container-wr_content"></div>

  <script>
    // ★ 실제 Asamiya-Editor가 참조하는 전역 업로드 엔드포인트 (CSRF 토큰 포함 가능)
    var asamiyaEditorUploadUrl = "/api/editor/upload";
    var asamiyaEditorFileUploadUrl = "/api/editor/file-upload";
    // (PRO 버전 AI 기능 사용 시) var asamiyaEditorAiUrl = "/api/editor/ai-generate";

    document.addEventListener("DOMContentLoaded", function() {
        window.editor = new AsamiyaEditor("#asamiya-container-wr_content", {
            placeholder: "내용을 입력해주세요...",
            height: "450px"
        });
        var initialContent = document.getElementById("wr_content").value;
        if (initialContent) {
            window.editor.setHTML(initialContent);
        }
    });

    function syncEditorContent() {
        document.getElementById("wr_content").value = window.editor.getHTML();
    }
  </script>
  ```
* **인라인 이미지 & 첨부파일 통합 관리 (`post_files` 단일 테이블):**
  * 이미지(`file_type = 'image'`)와 일반 첨부파일(`file_type = 'attachment'`) 모두 `post_files` 테이블에서 통합 관리합니다.
  * **업로드 API 응답 규격:**
    * 이미지 업로드 (`POST /api/editor/upload`): `{ "url": "/uploads/editor/2611/uuid.webp", "file_uuid": "..." }` (`asamiya-editor.js` 호환)
    * 파일 업로드 (`POST /api/editor/file-upload`): `{ "file_uuid": "...", "name": "문서.pdf", "size": 1048576, "download_url": "/api/download/..." }`
* **스마트 인라인 첨부 & 2단계 생명주기 동기화:**
  1. **보안 다운로드 프록시 (`/api/download/:file_uuid`):** 본문에 삽입된 다운로드 카드(`<div class="asamiya-file-card" data-file-uuid="...">`)는 정적 파일 경로 대신 프록시 URL을 가리키며, 요청 시 **권한 레벨 + 포인트 차감**을 검증한 뒤 `StorageBackend`를 통해 스트리밍합니다.
  2. **2단계 임시 업로드 & 고아 파일(Orphan) GC:**
     * 업로드 직후에는 `is_temp = 1`, `post_id = NULL` 상태로 저장.
     * 최종 글 저장 시 백엔드가 본문 HTML에서 사용된 `file_uuid` 및 이미지 URL을 추출해 `post_id`를 바인딩하고 `is_temp = 0`으로 확정.
     * 저장되지 않고 24시간 이상 방치된 `is_temp = 1` 파일은 백그라운드 스케줄러가 디스크와 DB에서 자동 삭제.
  3. **글 수정/삭제 시 본문-DB 자동 동기화:** 글 수정 시 본문에서 지워진 이미지·파일 카드는 즉시 정리 대상으로 마킹하고 스토리지에서 삭제합니다.
  4. **스토리지 추상화 (`StorageBackend` Trait):** `put()`, `get_stream()`, `delete()` 트레이트를 통해 로컬 디스크에서 추후 S3/Cloudflare R2로 코드 수정 없이 전환 가능합니다.

---

### 3.2 보안 아키텍처 (Security Architecture)
1. **Stored XSS 완벽 방어 (`ammonia` 커스텀 화이트리스트):**
   * `Asamiya-Editor`는 `styleWithCSS` 모드(인라인 `style` 속성)와 유튜브 `<iframe>`, 파일 카드 `<div>`를 생성합니다.
   * 저장 전 반드시 Rust **`ammonia::Builder`**를 거쳐 `<script>`, `onerror`, `javascript:` 스킴 등 XSS 벡터를 제거하되, 아래 항목만 정밀하게 허용합니다:
     * 안전 서식 태그 및 제한된 인라인 스타일(`color`, `background-color`, `text-align`, `font-size`, `font-weight`)
     * `<iframe>`: `src` 속성이 `https://www.youtube.com/embed/`로 시작하는 경우만 허용 (속성 필터 적용)
     * `<div class="asamiya-file-card" data-file-uuid="...">` 다운로드 위젯 속성 허용
     * `data:image/...` Base64 이미지 차단 (DB 비대화 및 악성 페이로드 방지)
2. **CSRF 방어:** 모든 상태 변경(`POST`, `PUT`, `DELETE`) 웹 폼과 AJAX(`fetch`/HTMX) 요청에 대해 서버 발급 CSRF 토큰(`X-CSRF-Token` 헤더 또는 폼 필드) 검증.
3. **서버 영속 세션 (`sessions` 테이블):** 단순 Stateless JWT 대신 DB 세션 저장소를 사용하여, 관리자가 회원을 차단(`is_blocked = 1`)하면 해당 회원의 모든 활성 세션이 즉시 무효화됩니다.
4. **파일 업로드 보안:** 허용 확장자/MIME 매직바이트 검증, 실행 파일(`.exe`, `.sh`, `.php`, `.js`, `.html`) 차단, 경로 탐색(Path Traversal) 방지를 위해 디스크 저장 파일명은 무조건 UUID로 치환.

---

### 3.3 회원 & 1~10 레벨 시스템 및 동적 확장
* **기본 레벨 (1~10):** `1`(손님), `2`(기본 가입회원), `3~9`(커뮤니티 등급), `10`(최고 관리자).
* **동적 확장 (`member_level_configs`):** 대시보드에서 레벨별 칭호, 뱃지 아이콘, 자동 승급 조건(`req_point`, `req_posts`, `req_comments`)을 동적으로 수정 가능.
* **여분 필드 (`extra_data` JSON):** `members`, `boards`, `posts` 테이블 모두에 `extra_data TEXT` 컬럼을 제공하여, 향후 스키마 변경 없이 구인구직/중고거래/프로필 커스텀 필드를 유연하게 확장 가능.
* **Soft Delete:** 회원, 게시글, 댓글 삭제 시 물리 삭제 대신 `deleted_at` 타임스탬프를 기록하여 오삭제 복구 및 감사 추적 보장.

---

### 3.4 동시성 안전 포인트 경제 시스템 (Atomic Point Economy)
* **Race Condition 원천 차단:** 포인트 차감 시 "조회 후 차감" 방식이 아닌 **DB 트랜잭션 내 원자적 조건부 업데이트**를 수행합니다.
  ```sql
  UPDATE members SET mb_point = mb_point + :delta
  WHERE id = :member_id AND (mb_point + :delta) >= 0;
  ```
* **1일 1회 출석 보너스 원자적 처리:** 로그인 트랜잭션 안에서 조건부 업데이트로 동시 다발 로그인 시 중복 지급을 차단합니다.
  ```sql
  UPDATE members SET mb_today_login = CURRENT_TIMESTAMP
  WHERE id = :member_id AND (mb_today_login IS NULL OR DATE(mb_today_login, 'localtime') < DATE('now', 'localtime'));
  ```
  (`rows_affected == 1`일 때만 `login_point` 지급 및 `point_logs` 기록)
* **중복 열람/다운로드 면제:** `point_logs`의 복합 인덱스 `(member_id, rel_table, rel_id, rel_action)`를 조회하여 이미 비용을 지불한 글/파일에 대한 재차감을 방지합니다.

---

### 3.5 확장성 코어: EventBus 훅 & FTS5 초고속 검색
1. **EventBus 훅 시스템 (`src/events/`):**
   * `PostCreated`, `PostDeleted`, `CommentCreated`, `MemberJoined`, `PointChanged` 등의 도메인 이벤트를 발행(Publish).
   * 향후 알림 발송, 자동 등급업 검사, 텔레그램/디스코드 웹훅, Rhai/Lua 스크립트 플러그인을 코어 코드 수정 없이 구독(Subscribe) 형태로 연결.
2. **FTS5 Trigram 전문 검색 (`posts_fts`):**
   * 내장 SQLite 3의 `FTS5` + `tokenize='trigram'` 가상 테이블을 사용하여 한국어 부분 일치 검색(`%검색어%`)도 `LIKE` 풀스캔 없이 밀리초(ms) 단위로 처리.

---

### 3.6 동적 디자인 테마 & 스킨 전환 시스템 (Dynamic Theme & Skin Engine)
* **설계 철학:** 기본 디자인(`default`)을 항상 안전망(Fallback)으로 유지하면서, 관리자가 원할 때 서버 재시작 없이 원클릭으로 사이트 전체 디자인 테마와 게시판별 스킨을 자유롭게 교체할 수 있습니다.
* **이중 레이어 구조:**
  1. **사이트 전체 테마 (`site_configs.active_theme`, 기본값 `'default'`):** `templates/themes/{theme_name}/` 폴더 단위로 사이트 레이아웃, 메인 페이지, 로그인/회원가입 UI, 색상 및 CSS/JS 자산을 통째로 교체.
  2. **게시판 개별 스킨 (`boards.bo_skin`, 기본값 `'basic'`):** 같은 테마 안에서도 게시판별로 `basic`(일반), `gallery`(갤러리), `webzine`(웹진) 등 스킨을 개별 지정 가능.
* **테마 메타데이터 (`theme.toml`):**
  ```toml
  name = "Modern Dark"
  slug = "modern_dark"
  version = "1.0.0"
  author = "Asamiya"
  description = "눈이 편안한 다크 모드 모던 커뮤니티 테마"
  thumbnail = "assets/preview.png"
  ```
* **무중단 런타임 전환 & 안전한 Fallback 체인 (`theme_service.rs`):**
  * **실시간 전환:** 관리자가 대시보드(`/admin/themes`)에서 `[적용하기]`를 클릭하면 DB(`site_configs`)와 서버 메모리(`AppState.active_theme`)를 동시에 업데이트하여 **재컴파일·재시작 없이 즉시 반영**.
  * **부분 오버라이드 지원 (Child-Theme 스타일):** 새 테마를 제작할 때 모든 HTML 파일을 복사할 필요 없이 바꾸고 싶은 파일(`layout.html`, `index.html` 등)만 넣어도 작동합니다. `ThemeResolver`가 아래 순서로 자동 Fallback 탐색을 수행하여 **템플릿 누락에 의한 500 에러를 원천 차단**합니다:
    * *일반 페이지:* `themes/{active_theme}/{path}` ➔ `themes/default/{path}`
    * *게시판 페이지:*
      1. `themes/{active_theme}/board/{bo_skin}/{file}`
      2. `themes/{active_theme}/board/basic/{file}`
      3. `themes/default/board/{bo_skin}/{file}`
      4. `themes/default/board/basic/{file}`
  * **전용 정적 에셋 헬퍼:** 템플릿에서 `{{ theme_asset('css/style.css') }}`를 호출하면 현재 활성 테마(또는 Fallback 된 `default` 테마)의 `/theme-assets/{theme}/...` 경로로 자동 매핑.
* **관리자 미리보기 & 보안:**
  * **관리자 미리보기(Preview):** 레벨 10 관리자는 사이트에 적용하기 전 `?preview_theme={theme_name}` 또는 미리보기 세션을 통해 본인 브라우저에서만 새 테마를 먼저 테스트 가능.
  * **보안 검증:** 테마명·스킨명은 `^[a-zA-Z0-9_-]+$` 정규식으로 엄격히 검증하여 경로 탐색(Path Traversal) 공격 차단.

---

## 4. 데이터베이스 엔티티 관계도 (ERD)

```
 [site_configs]             [member_level_configs] (1~10 동적 등급)
       │                              │
       │ (active_theme)               ▼
       │         [sessions] ◄─── [members] ────► [notifications]
       │                             │     │
       ▼                             ▼     ▼
 [ThemeResolver]             [point_logs] [posts] ◄── (N:1) ── [boards] ◄── [board_groups]
 (themes/{active} -> default)              │   │
                                           ▼   ▼
                                   [post_files] [comments] (대댓글 self-ref)
                                           │
                                           ▼
                                  [posts_fts (FTS5)]
```

---

## 5. 단계별 개발 로드맵 (Development Roadmap)

### Phase 1: 기반 인프라 & 보안 미들웨어 구축
- [ ] `Cargo.toml` 의존성 설정 (`axum`, `tokio`, `sqlx`, `minijinja`, `ammonia`, `argon2`, `tower-http`, `uuid`, `image`)
- [ ] `db.rs`: SQLite 3 커넥션 풀 초기화 (`WAL` 모드 및 `FOREIGN KEYS = ON` 활성화) & 스키마 자동 마이그레이션
- [ ] `StorageBackend` 트레이트 및 `LocalStorage` 구현
- [ ] `MiniJinja` + `ThemeResolver` (다중 테마 & `default` Fallback 지원, Dev 핫 리로딩 / Prod 캐싱) 및 `/static/`, `/theme-assets/` 서빙
- [ ] `Asamiya-Editor` 에셋 복사 및 인라인 파일 업로드 UI 확장

### Phase 2: 회원, 영속 세션, CSRF & 동적 레벨 시스템
- [ ] `sessions` 기반 인증, Argon2id 패스워드 해싱, 차단 회원 즉시 튕겨내기 미들웨어
- [ ] CSRF 토큰 발급 및 검증 미들웨어 (`csrf.rs`)
- [ ] 1~10 레벨 접근 제어 Guard (`permission.rs`)
- [ ] 원자적 1일 1회 출석 포인트 지급 & 동적 `member_level_configs` 관리

### Phase 3: 원자적 포인트 & EventBus 코어
- [ ] `point_service`: 원자적 SQL 잔액 업데이트 + `point_logs` 트랜잭션
- [ ] `EventBus`: `PostCreated`, `CommentCreated`, `PointChanged` 비동기 이벤트 전파
- [ ] 자동 등급업 워커 (`level_service`) & 알림 서비스 (`notification_service`)

### Phase 4: 게시판, Asamiya-Editor & 스마트 파일 생명주기
- [ ] 게시판 목록 (FTS5 trigram 검색 연동), 읽기, 쓰기/수정 뷰 구현
- [ ] `ammonia` 커스텀 HTML Sanitizer 적용 (YouTube iframe & 스타일 화이트리스트)
- [ ] `POST /api/editor/upload` 및 `POST /api/editor/file-upload` 구현
- [ ] 보안 다운로드 프록시(`/api/download/:file_uuid`) 및 고아 파일 자동 청소 백그라운드 태스크

### Phase 5: 댓글, 추천 및 관리자 대시보드
- [ ] 계층형 댓글(대댓글), 비밀댓글, 추천/비추천(`post_votes`)
- [ ] 관리자 대시보드: 통계, **디자인 테마 변경/미리보기(`/admin/themes`)**, 회원/동적 레벨 관리, 게시판 관리, **미디어/파일 통합 관리 센터**, 포인트 내역
