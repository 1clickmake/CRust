use crate::config::DatabaseConfig;
use crate::services::auth_service;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

const SCHEMA_SQL: &str = include_str!("../plan/database_schema.sql");

pub async fn init_db(cfg: &DatabaseConfig) -> Result<SqlitePool, Box<dyn std::error::Error>> {
    let opts = SqliteConnectOptions::from_str(&cfg.url)?
        .create_if_missing(true)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(cfg.max_connections)
        .connect_with(opts)
        .await?;

    // Execute full DDL & FTS5 triggers
    sqlx::raw_sql(SCHEMA_SQL).execute(&pool).await?;

    seed_initial_data(&pool).await?;

    Ok(pool)
}

async fn seed_initial_data(pool: &SqlitePool) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Seed initial admin account if members table is empty
    let (member_count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM members")
        .fetch_one(pool)
        .await?;

    if member_count == 0 {
        let pw_hash = auth_service::hash_password("admin1234")?;
        sqlx::query(
            "INSERT INTO members (mb_id, mb_password, mb_name, mb_email, mb_level, mb_point)
             VALUES (?, ?, ?, ?, 10, 10000)",
        )
        .bind("admin")
        .bind(pw_hash)
        .bind("최고관리자")
        .bind("admin@localhost")
        .execute(pool)
        .await?;
        tracing::info!("초기 관리자 계정(admin / admin1234)을 생성했습니다.");
    }

    // 2. Seed initial board group and default boards
    let (board_count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM boards")
        .fetch_one(pool)
        .await?;

    if board_count == 0 {
        sqlx::query(
            "INSERT OR IGNORE INTO board_groups (gr_id, gr_subject, order_num) VALUES ('community', '커뮤니티', 1)",
        )
        .execute(pool)
        .await?;

        sqlx::query(
            "INSERT INTO boards (bo_table, gr_id, bo_subject, bo_category_list, bo_skin, bo_list_level, bo_read_level, bo_write_level, bo_comment_level, bo_download_level, bo_read_point, bo_write_point, bo_comment_point, bo_download_point)
             VALUES
             ('notice', 'community', '공지사항', '공지,업데이트', 'basic', 1, 1, 10, 2, 1, 0, 0, 2, 0),
             ('free', 'community', '자유게시판', '잡담,질문,정보', 'basic', 1, 1, 2, 2, 2, 0, 10, 3, -5)",
        )
        .execute(pool)
        .await?;

        let (admin_id,): (i64,) = sqlx::query_as("SELECT id FROM members WHERE mb_id = 'admin'")
            .fetch_one(pool)
            .await?;

        sqlx::query(
            "INSERT INTO posts (bo_table, member_id, wr_name, wr_category, wr_subject, wr_content, is_notice)
             VALUES (?, ?, '최고관리자', '공지', '🎉 CRust 프레임워크 1차 빌드에 오신 것을 환영합니다!',
             '<p>안녕하세요! <strong>Rust(Axum) + SQLite FTS5 + MiniJinja + CRust_Editor</strong> 기반으로 구축된 차세대 그누보드형 프레임워크입니다.</p><p>관리자 계정(<code>admin / admin1234</code>)으로 로그인하여 <b>테마 전환, 회원 레벨 관리, 게시판 설정, 에디터 파일 첨부</b> 기능을 마음껏 테스트해 보세요! 💕</p>', 1)",
        )
        .bind("notice")
        .bind(admin_id)
        .execute(pool)
        .await?;

        tracing::info!("초기 게시판(notice, free) 및 환영 공지글을 생성했습니다.");
    }

    Ok(())
}
