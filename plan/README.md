# Rust Board 프로젝트 기획 & 설계 문서

두목님 💕과 아미가 함께 구상하고 철저히 검수한 Rust 기반 웹 프레임워크의 종합 설계 문서 저장소입니다.

## 📁 문서 목차
1. [기능 명세 및 아키텍처 명세서 (`specification.md` v1.1.0)](file:///c:/Users/hades/OneDrive/Desktop/작업폴더/rust_board/plan/specification.md)
   - 프로젝트 개요, 디렉토리 구조, `StorageBackend` 및 `EventBus` 확장 코어
   - **`Asamiya-Editor` 실제 코드 검증 반영** (전역 변수 `asamiyaEditorUploadUrl`, PRO AI 연동, 스마트 인라인 파일 첨부)
   - **동적 디자인 테마 & 스킨 전환 시스템:** 기본 `default` 테마 + 런타임 무중단 테마 교체, 부분 오버라이드 Fallback 리졸버, 관리자 미리보기
   - **보안 아키텍처:** `Ammonia` 커스텀 XSS 필터(YouTube iframe/파일 카드 허용), CSRF 방어, 서버 영속 세션
   - **원자적(Atomic) 포인트 & 동적 1~10 회원 레벨 체계** (Race Condition 차단, `extra_data` 여분 필드, Soft Delete)
   - 5단계 개발 로드맵

2. [데이터베이스 DDL 스키마 (`database_schema.sql`)](file:///c:/Users/hades/OneDrive/Desktop/작업폴더/rust_board/plan/database_schema.sql)
   - SQLite 3 (WAL + FTS5 Trigram 전문 검색) 표준 DDL
   - `site_configs`, `member_level_configs`, `members`, `sessions`, `point_logs`
   - `board_groups`, `boards`, `posts`, `posts_fts` (FTS5 + 동기화 트리거)
   - `post_files` (인라인 이미지 + 첨부파일 통합 생명주기), `comments`, `post_votes`, `notifications`

3. [업그레이드 로드맵 & 진행 현황 (`setting.md`)](file:///c:/Users/hades/OneDrive/Desktop/작업폴더/rust_board/plan/setting.md)
   - Phase 1 · Phase 3 (`3-1 ~ 3-4`) · `CRust` UI/다국어(i18n)/DB 로깅 고도화 완료 현황
   - **확장성 최우선 로드맵 (`Track A ~ Track E` 전부 완료 & E2E 검증 완료):** `EventBus` 알림/대댓글/스킨/SEO(A) → 쪽지/신고/임시저장(B) → 이미지최적화/S3(C) → 전용 `/admin/oauth` 소셜로그인(D) → `/admin/ai-settings` · `/admin/ai-builder` 분리형 AI 빌더(E)


