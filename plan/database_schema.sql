-- ==============================================================================
-- Rust Board (그누보드형 웹 프레임워크) 데이터베이스 표준 DDL
-- 파일명: plan/database_schema.sql
-- 기준 엔진: SQLite 3 (WAL 모드 + FTS5 Trigram 내장)
-- 참고: 향후 PostgreSQL / MySQL 확장 시 SeaORM / SQLx 마이그레이션 어댑터로 변환
-- ==============================================================================

PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;

-- 1. 사이트 환경설정 테이블
CREATE TABLE IF NOT EXISTS site_configs (
    cf_key VARCHAR(50) PRIMARY KEY,               -- 설정 식별 키 (예: 'site_title', 'login_point')
    cf_value TEXT NOT NULL,                       -- 설정 값
    description VARCHAR(255)                      -- 설정 설명
);

INSERT OR IGNORE INTO site_configs (cf_key, cf_value, description) VALUES
('site_title', 'Rust Board Community', '사이트 타이틀'),
('admin_email', 'admin@example.com', '최고관리자 이메일'),
('active_theme', 'default', '현재 활성화된 사이트 디자인 테마 폴더명'),
('login_point', '100', '1일 1회 첫 로그인 출석 포인트'),
('use_point', '1', '포인트 시스템 사용 여부 (1:사용, 0:미사용)'),
('register_level', '2', '신규 회원가입 시 기본 레벨'),
('register_point', '1000', '신규 회원가입 축하 포인트');


-- 2. 회원 레벨 및 동적 승급 설정 테이블 (1~10 레벨 동적 확장)
CREATE TABLE IF NOT EXISTS member_level_configs (
    level INTEGER PRIMARY KEY,                    -- 레벨 (1 ~ 10)
    level_name VARCHAR(50) NOT NULL,              -- 칭호/등급명 (예: 새싹회원, 일반회원, 관리자)
    badge_icon VARCHAR(100),                      -- 등급 뱃지 아이콘 (FontAwesome 클래스 또는 이미지 경로)
    req_point INTEGER NOT NULL DEFAULT 0,         -- 자동 승급 필요 포인트 (0이면 자동승급 없음)
    req_posts INTEGER NOT NULL DEFAULT 0,         -- 자동 승급 필요 작성글 수
    req_comments INTEGER NOT NULL DEFAULT 0       -- 자동 승급 필요 작성댓글 수
);

INSERT OR IGNORE INTO member_level_configs (level, level_name, badge_icon, req_point, req_posts, req_comments) VALUES
(1, '손님(비회원)', 'fa-user-secret', 0, 0, 0),
(2, '새싹회원', 'fa-seedling', 0, 0, 0),
(3, '일반회원', 'fa-user', 1500, 5, 10),
(4, '성실회원', 'fa-user-check', 3000, 20, 50),
(5, '열심회원', 'fa-medal', 6000, 50, 100),
(6, '우수회원', 'fa-award', 10000, 100, 200),
(7, '특별회원', 'fa-gem', 20000, 200, 400),
(8, 'VIP회원', 'fa-crown', 50000, 500, 1000),
(9, '부관리자', 'fa-shield-halved', 0, 0, 0),
(10, '최고관리자', 'fa-shield', 0, 0, 0);


-- 3. 회원 테이블 (Soft Delete 및 JSON 여분 필드 지원)
CREATE TABLE IF NOT EXISTS members (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    mb_id VARCHAR(50) NOT NULL UNIQUE,            -- 회원 로그인 아이디
    mb_password VARCHAR(255) NOT NULL,            -- 비밀번호 해시 (Argon2id)
    mb_name VARCHAR(50) NOT NULL,                 -- 실명 또는 닉네임
    mb_email VARCHAR(100) NOT NULL UNIQUE,        -- 이메일 주소
    mb_level INTEGER NOT NULL DEFAULT 2,          -- 회원 등급 (1~10, 기본 2)
    mb_point INTEGER NOT NULL DEFAULT 0,          -- 현재 보유 포인트 잔액
    mb_today_login DATETIME,                      -- 금일 로그인 일시 (출석 보너스 원자적 중복방지)
    mb_login_ip VARCHAR(50),                      -- 최종 로그인 IP
    is_blocked INTEGER NOT NULL DEFAULT 0,        -- 차단 여부 (0:정상, 1:차단)
    extra_data TEXT NOT NULL DEFAULT '{}',        -- JSON 여분 필드 (차후 프로필 항목 무한 확장)
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    deleted_at DATETIME DEFAULT NULL              -- Soft Delete 타임스탬프
);

CREATE INDEX IF NOT EXISTS idx_members_level ON members(mb_level) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_members_created ON members(created_at);


-- 4. 서버 영속 세션 테이블 (차단 회원 즉시 로그아웃 및 보안 제어)
CREATE TABLE IF NOT EXISTS sessions (
    id VARCHAR(128) PRIMARY KEY,                  -- 암호학적 랜덤 세션 토큰
    member_id INTEGER NOT NULL,                   -- 로그인 회원 ID
    csrf_token VARCHAR(128) NOT NULL,             -- 세션별 CSRF 방어 토큰
    ip_address VARCHAR(50),
    user_agent VARCHAR(255),
    expires_at DATETIME NOT NULL,                 -- 만료 일시
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (member_id) REFERENCES members(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_sessions_member ON sessions(member_id);
CREATE INDEX IF NOT EXISTS idx_sessions_expires ON sessions(expires_at);


-- 5. 포인트 변동 내역 테이블 (Point Transactions)
CREATE TABLE IF NOT EXISTS point_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    member_id INTEGER NOT NULL,                   -- 대상 회원 ID
    rel_table VARCHAR(50),                        -- 연관 테이블 ('login', 'board', 'file', 'admin')
    rel_id VARCHAR(50),                           -- 연관 대상 식별자 (post_id, file_uuid 등)
    rel_action VARCHAR(50) NOT NULL,              -- 행동 유형 ('login', 'write', 'comment', 'view', 'download', 'admin')
    point INTEGER NOT NULL,                       -- 증감 포인트 (+100, -50 등)
    balance_after INTEGER NOT NULL,               -- 변동 후 잔여 포인트
    content VARCHAR(255) NOT NULL,                -- 변동 사유 및 설명
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (member_id) REFERENCES members(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_point_logs_member ON point_logs(member_id, id DESC);
CREATE INDEX IF NOT EXISTS idx_point_logs_dedup ON point_logs(member_id, rel_table, rel_id, rel_action);


-- 6. 게시판 그룹 테이블
CREATE TABLE IF NOT EXISTS board_groups (
    gr_id VARCHAR(50) PRIMARY KEY,                -- 그룹 식별자 (예: 'community', 'customer')
    gr_subject VARCHAR(100) NOT NULL,             -- 그룹 명칭 (예: '커뮤니티', '고객지원')
    order_num INTEGER NOT NULL DEFAULT 0          -- 출력 순서
);


-- 7. 게시판 설정 테이블
CREATE TABLE IF NOT EXISTS boards (
    bo_table VARCHAR(50) PRIMARY KEY,             -- 게시판 식별 코드 (예: 'free', 'notice')
    gr_id VARCHAR(50),                            -- 소속 그룹 ID
    bo_subject VARCHAR(100) NOT NULL,             -- 게시판 이름
    bo_category_list TEXT,                        -- 분류 목록 (콤마 구분: '잡담,질문,정보')
    bo_skin VARCHAR(50) NOT NULL DEFAULT 'basic', -- 적용 템플릿 스킨 디렉토리명
    
    -- 레벨 권한 설정 (1~10)
    bo_list_level INTEGER NOT NULL DEFAULT 1,     -- 목록 보기 최소 레벨
    bo_read_level INTEGER NOT NULL DEFAULT 1,     -- 글 읽기 최소 레벨
    bo_write_level INTEGER NOT NULL DEFAULT 2,    -- 글 작성 최소 레벨
    bo_comment_level INTEGER NOT NULL DEFAULT 2,  -- 댓글 작성 최소 레벨
    bo_download_level INTEGER NOT NULL DEFAULT 2, -- 첨부파일 다운로드 최소 레벨
    
    -- 포인트 설정 (음수는 차감, 양수는 지급)
    bo_read_point INTEGER NOT NULL DEFAULT 0,     -- 글 열람 시 포인트
    bo_write_point INTEGER NOT NULL DEFAULT 5,    -- 글 작성 시 지급 포인트
    bo_comment_point INTEGER NOT NULL DEFAULT 2,  -- 댓글 작성 시 지급 포인트
    bo_download_point INTEGER NOT NULL DEFAULT -10, -- 파일 다운로드 시 차감 포인트
    
    -- 파일 업로드 설정
    bo_upload_count INTEGER NOT NULL DEFAULT 5,   -- 게시글당 첨부파일 허용 개수 (0~20)
    bo_upload_size INTEGER NOT NULL DEFAULT 10485760, -- 파일당 최대 크기 (Bytes, 기본 10MB)
    extra_data TEXT NOT NULL DEFAULT '{}',        -- JSON 여분 필드 스키마 정의 (커스텀 입력폼 확장용)
    
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (gr_id) REFERENCES board_groups(gr_id) ON DELETE SET NULL
);


-- 8. 게시글 테이블 (Posts - Soft Delete & JSON 여분 필드 지원)
CREATE TABLE IF NOT EXISTS posts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    bo_table VARCHAR(50) NOT NULL,                -- 소속 게시판 코드
    member_id INTEGER,                            -- 작성 회원 ID (비회원 작성 시 NULL)
    wr_name VARCHAR(50) NOT NULL,                 -- 작성자 닉네임
    wr_password VARCHAR(255),                     -- 비회원 글 비밀번호 해시
    wr_category VARCHAR(50),                      -- 게시글 분류
    wr_subject VARCHAR(255) NOT NULL,             -- 게시글 제목
    wr_content TEXT NOT NULL,                     -- 본문 HTML (Ammonia Sanitizer 통과 후 저장)
    
    wr_hit INTEGER NOT NULL DEFAULT 0,            -- 조회수
    wr_good INTEGER NOT NULL DEFAULT 0,           -- 추천수
    wr_nogood INTEGER NOT NULL DEFAULT 0,         -- 비추천수
    wr_comment_count INTEGER NOT NULL DEFAULT 0,  -- 댓글 수
    wr_file_count INTEGER NOT NULL DEFAULT 0,     -- 첨부파일 수 (목록 📎 아이콘 최적화용 캐시)
    
    is_notice INTEGER NOT NULL DEFAULT 0,         -- 공지사항 등록 여부 (0/1)
    is_secret INTEGER NOT NULL DEFAULT 0,         -- 비밀글 여부 (0/1)
    wr_link1 VARCHAR(255),                        -- 관련 링크 1
    wr_link2 VARCHAR(255),                        -- 관련 링크 2
    wr_ip VARCHAR(50),                            -- 작성자 IP
    extra_data TEXT NOT NULL DEFAULT '{}',        -- JSON 여분 필드 (wr_1~wr_10 대체 무한 확장)
    
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    deleted_at DATETIME DEFAULT NULL,             -- Soft Delete 타임스탬프
    FOREIGN KEY (bo_table) REFERENCES boards(bo_table) ON DELETE CASCADE,
    FOREIGN KEY (member_id) REFERENCES members(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_posts_list ON posts(bo_table, is_notice DESC, id DESC) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_posts_member ON posts(member_id) WHERE deleted_at IS NULL;


-- 8-1. 게시글 초고속 전문 검색 테이블 (SQLite FTS5 Trigram)
CREATE VIRTUAL TABLE IF NOT EXISTS posts_fts USING fts5(
    wr_subject,
    wr_content,
    content='posts',
    content_rowid='id',
    tokenize='trigram'
);

-- FTS5 자동 동기화 트리거 (Soft Delete 반영)
CREATE TRIGGER IF NOT EXISTS posts_fts_ai AFTER INSERT ON posts WHEN NEW.deleted_at IS NULL BEGIN
    INSERT INTO posts_fts(rowid, wr_subject, wr_content) VALUES (NEW.id, NEW.wr_subject, NEW.wr_content);
END;

CREATE TRIGGER IF NOT EXISTS posts_fts_ad AFTER DELETE ON posts WHEN OLD.deleted_at IS NULL BEGIN
    INSERT INTO posts_fts(posts_fts, rowid, wr_subject, wr_content) VALUES('delete', OLD.id, OLD.wr_subject, OLD.wr_content);
END;

CREATE TRIGGER IF NOT EXISTS posts_fts_au_del AFTER UPDATE OF wr_subject, wr_content, deleted_at ON posts WHEN OLD.deleted_at IS NULL BEGIN
    INSERT INTO posts_fts(posts_fts, rowid, wr_subject, wr_content) VALUES('delete', OLD.id, OLD.wr_subject, OLD.wr_content);
END;

CREATE TRIGGER IF NOT EXISTS posts_fts_au_ins AFTER UPDATE OF wr_subject, wr_content, deleted_at ON posts WHEN NEW.deleted_at IS NULL BEGIN
    INSERT INTO posts_fts(rowid, wr_subject, wr_content) VALUES (NEW.id, NEW.wr_subject, NEW.wr_content);
END;


-- 9. 통합 미디어 및 첨부파일 테이블 (인라인 이미지 + 일반 파일 생명주기 통합 관리)
CREATE TABLE IF NOT EXISTS post_files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_uuid VARCHAR(50) NOT NULL UNIQUE,        -- 보안 다운로드 프록시 & 본문 동기화 고유 UUID
    post_id INTEGER,                              -- 소속 게시글 ID (임시 업로드 시 NULL, 글 저장 시 바인딩)
    member_id INTEGER,                            -- 업로드 회원 ID
    file_type VARCHAR(20) NOT NULL DEFAULT 'attachment', -- 'image' (에디터 이미지) 또는 'attachment' (다운로드 파일)
    mime_type VARCHAR(100),                       -- MIME 타입 (예: 'image/webp', 'application/pdf')
    storage_driver VARCHAR(20) NOT NULL DEFAULT 'local', -- 스토리지 드라이버 ('local', 's3')
    bf_source VARCHAR(255) NOT NULL,              -- 원본 파일명 (예: '계약서.pdf')
    bf_file VARCHAR(255) NOT NULL,                -- 저장 파일명 (UUID 기반)
    bf_path VARCHAR(255) NOT NULL,                -- 상대 경로 또는 오브젝트 키
    bf_filesize INTEGER NOT NULL,                 -- 파일 크기 (Bytes)
    bf_download_count INTEGER NOT NULL DEFAULT 0, -- 다운로드 누적 카운트
    is_temp INTEGER NOT NULL DEFAULT 1,           -- 1: 글 저장 전 임시상태, 0: 정상 등록 완료
    status VARCHAR(20) NOT NULL DEFAULT 'active', -- 상태 ('active', 'deleted')
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (post_id) REFERENCES posts(id) ON DELETE CASCADE,
    FOREIGN KEY (member_id) REFERENCES members(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_post_files_uuid ON post_files(file_uuid);
CREATE INDEX IF NOT EXISTS idx_post_files_post ON post_files(post_id, status);
CREATE INDEX IF NOT EXISTS idx_post_files_temp ON post_files(is_temp, created_at);


-- 10. 댓글 테이블 (Comments - 대댓글 계층 및 Soft Delete 지원)
CREATE TABLE IF NOT EXISTS comments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    post_id INTEGER NOT NULL,                     -- 소속 게시글 ID
    parent_id INTEGER DEFAULT NULL,               -- 부모 댓글 ID (대댓글일 경우 부모 댓글의 id, 최상위면 NULL)
    member_id INTEGER,                            -- 작성 회원 ID
    co_name VARCHAR(50) NOT NULL,                 -- 작성자 닉네임
    co_password VARCHAR(255),                     -- 비회원 댓글 비밀번호 해시
    co_content TEXT NOT NULL,                     -- 댓글 내용
    co_ip VARCHAR(50),                            -- 작성자 IP
    is_secret INTEGER NOT NULL DEFAULT 0,         -- 비밀댓글 여부 (0/1)
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    deleted_at DATETIME DEFAULT NULL,             -- Soft Delete 타임스탬프
    FOREIGN KEY (post_id) REFERENCES posts(id) ON DELETE CASCADE,
    FOREIGN KEY (parent_id) REFERENCES comments(id) ON DELETE CASCADE,
    FOREIGN KEY (member_id) REFERENCES members(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_comments_post ON comments(post_id, id ASC) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_comments_parent ON comments(parent_id);


-- 11. 추천/비추천 이력 테이블 (중복 추천 방지)
CREATE TABLE IF NOT EXISTS post_votes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    post_id INTEGER NOT NULL,
    member_id INTEGER NOT NULL,
    vote_type VARCHAR(10) NOT NULL,               -- 'good' 또는 'nogood'
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(post_id, member_id),
    FOREIGN KEY (post_id) REFERENCES posts(id) ON DELETE CASCADE,
    FOREIGN KEY (member_id) REFERENCES members(id) ON DELETE CASCADE
);


-- 12. 실시간 / 커뮤니티 알림 테이블 (Notifications)
CREATE TABLE IF NOT EXISTS notifications (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    member_id INTEGER NOT NULL,                   -- 알림 수신 회원 ID
    noti_type VARCHAR(30) NOT NULL,               -- 유형 ('comment', 'reply', 'level_up', 'point', 'system')
    message VARCHAR(255) NOT NULL,                -- 알림 메시지
    target_url VARCHAR(255),                      -- 클릭 시 이동할 URL
    is_read INTEGER NOT NULL DEFAULT 0,           -- 읽음 여부 (0:안읽음, 1:읽음)
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (member_id) REFERENCES members(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_notifications_member ON notifications(member_id, is_read, id DESC);


-- 13. 회원 1:1 쪽지(DM) 테이블 (Memos)
CREATE TABLE IF NOT EXISTS memos (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    sender_id INTEGER NOT NULL,
    receiver_id INTEGER NOT NULL,
    me_subject VARCHAR(255) NOT NULL,
    me_content TEXT NOT NULL,
    is_read INTEGER NOT NULL DEFAULT 0,
    read_at DATETIME DEFAULT NULL,
    sender_deleted INTEGER NOT NULL DEFAULT 0,
    receiver_deleted INTEGER NOT NULL DEFAULT 0,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (sender_id) REFERENCES members(id) ON DELETE CASCADE,
    FOREIGN KEY (receiver_id) REFERENCES members(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_memos_receiver ON memos(receiver_id, receiver_deleted, id DESC);
CREATE INDEX IF NOT EXISTS idx_memos_sender ON memos(sender_id, sender_deleted, id DESC);


-- 14. 다형성 신고 관리 테이블 (Reports: post / comment / member 등 확장 수용)
CREATE TABLE IF NOT EXISTS reports (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    reporter_id INTEGER NOT NULL,
    target_type VARCHAR(30) NOT NULL,               -- 'post', 'comment', 'member'
    target_id INTEGER NOT NULL,
    bo_table VARCHAR(50),
    reason TEXT NOT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'pending',  -- 'pending', 'resolved', 'dismissed'
    admin_note TEXT,
    resolved_by INTEGER,
    resolved_at DATETIME DEFAULT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (reporter_id) REFERENCES members(id) ON DELETE CASCADE,
    FOREIGN KEY (resolved_by) REFERENCES members(id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_reports_status ON reports(status, id DESC);


-- 15. 확장형 소셜 인증 (OAuth2 Providers & Member OAuth Links)
CREATE TABLE IF NOT EXISTS oauth_providers (
    provider VARCHAR(30) PRIMARY KEY,
    display_name VARCHAR(50) NOT NULL,
    is_enabled INTEGER NOT NULL DEFAULT 0,
    client_id VARCHAR(255) NOT NULL DEFAULT '',
    client_secret VARCHAR(255) NOT NULL DEFAULT '',
    scopes VARCHAR(255) NOT NULL DEFAULT '',
    extra_config TEXT NOT NULL DEFAULT '{}',
    sort_order INTEGER NOT NULL DEFAULT 10,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

INSERT OR IGNORE INTO oauth_providers (provider, display_name, scopes, sort_order) VALUES
('kakao', 'Kakao', 'profile_nickname,account_email', 1),
('naver', 'Naver', 'name,email', 2),
('google', 'Google', 'openid email profile', 3),
('github', 'GitHub', 'read:user user:email', 4),
('apple', 'Apple', 'name email', 5);

CREATE TABLE IF NOT EXISTS member_oauth_links (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    member_id INTEGER NOT NULL,
    provider VARCHAR(30) NOT NULL,
    provider_user_id VARCHAR(191) NOT NULL,
    provider_email VARCHAR(255),
    provider_name VARCHAR(100),
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(provider, provider_user_id),
    FOREIGN KEY (member_id) REFERENCES members(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_oauth_links_member ON member_oauth_links(member_id);


-- 16. 커스텀 페이지 (Custom Pages - AI 빌더 및 운영자 정적 페이지 생성 + SEO 사이트맵 연동)
CREATE TABLE IF NOT EXISTS custom_pages (
    slug VARCHAR(80) PRIMARY KEY,
    title VARCHAR(255) NOT NULL,
    content_html TEXT NOT NULL,
    is_published INTEGER NOT NULL DEFAULT 1,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);


-- 17. 관리자 AI 빌더 변경 백업 스냅샷 (AI Change Backups & One-Click Rollback)
CREATE TABLE IF NOT EXISTS ai_change_backups (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    admin_id INTEGER NOT NULL,
    prompt_summary VARCHAR(255) NOT NULL,
    provider_used VARCHAR(30) NOT NULL,
    actions_json TEXT NOT NULL,
    snapshot_json TEXT NOT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'applied',
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    rolled_back_at DATETIME DEFAULT NULL,
    FOREIGN KEY (admin_id) REFERENCES members(id) ON DELETE CASCADE
);
