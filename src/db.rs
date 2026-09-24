use std::time::Duration;

use anyhow::Context;
use sea_orm::SqlxMySqlConnector;
use sea_orm::entity::prelude::*;
use sea_orm::{ActiveValue::Set, ConnectionTrait, DatabaseConnection, Schema, Statement};
use sqlx::mysql::MySqlPoolOptions;

use crate::config::DatabaseSettings;
use crate::entities::{category, item};
use crate::state::AppState;

const POOL_MAX_CONNECTIONS: u32 = 8;

pub async fn connect(settings: &DatabaseSettings) -> anyhow::Result<AppState> {
    let options = settings.to_connect_options();
    let pool = MySqlPoolOptions::new()
        .max_connections(POOL_MAX_CONNECTIONS)
        .acquire_timeout(Duration::from_secs(30))
        .connect_with(options)
        .await
        .context("failed to connect to MySQL with SQLx")?;

    sqlx::query("SELECT 1")
        .execute(&pool)
        .await
        .context("SQLx pool opened but a probe query failed")?;

    let db: DatabaseConnection = SqlxMySqlConnector::from_sqlx_mysql_pool(pool.clone());
    Ok(AppState { db, pool })
}

pub async fn ensure_schema_and_seed(state: &AppState) -> anyhow::Result<()> {
    create_schema(&state.db)
        .await
        .context("failed to create database schema")?;

    // GET_LOCK is connection-scoped. Hold one SQLx connection so concurrent
    // workers (or parallel tests) cannot race the empty-table seed.
    let mut lock = state
        .pool
        .acquire()
        .await
        .context("failed to acquire a MySQL connection for the seed lock")?;
    sqlx::query("SELECT GET_LOCK('wasmer_sqlx_demo_seed', 30)")
        .execute(&mut *lock)
        .await
        .context("failed to take the demo-data seed lock")?;

    let seed_result = seed_if_empty(&state.db)
        .await
        .context("failed to insert demo data");

    let _ = sqlx::query("SELECT RELEASE_LOCK('wasmer_sqlx_demo_seed')")
        .execute(&mut *lock)
        .await;
    seed_result
}

async fn create_schema(db: &DatabaseConnection) -> Result<(), sea_orm::DbErr> {
    let backend = db.get_database_backend();
    let schema = Schema::new(backend);

    let mut categories = schema.create_table_from_entity(category::Entity);
    categories.if_not_exists();
    db.execute(&categories).await?;

    let mut items = schema.create_table_from_entity(item::Entity);
    items.if_not_exists();
    db.execute(&items).await?;
    ensure_is_favourite_column(db).await?;

    tracing::info!("ensured categories and items tables exist");
    Ok(())
}

/// `CREATE TABLE IF NOT EXISTS` leaves an older `items` table unchanged.
/// Add the column when a database was created before `is_favourite` existed.
async fn ensure_is_favourite_column(db: &DatabaseConnection) -> Result<(), sea_orm::DbErr> {
    let existing = db
        .query_one_raw(Statement::from_string(
            db.get_database_backend(),
            "SELECT 1 AS present \
             FROM information_schema.COLUMNS \
             WHERE TABLE_SCHEMA = DATABASE() \
               AND TABLE_NAME = 'items' \
               AND COLUMN_NAME = 'is_favourite' \
             LIMIT 1",
        ))
        .await?;
    if existing.is_some() {
        return Ok(());
    }

    match db
        .execute_unprepared("ALTER TABLE items ADD COLUMN is_favourite bool NOT NULL DEFAULT FALSE")
        .await
    {
        Ok(_) => {
            tracing::info!("added items.is_favourite");
            Ok(())
        }
        Err(err) if is_duplicate_column(&err) => Ok(()),
        Err(err) => Err(err),
    }
}

async fn seed_if_empty(db: &DatabaseConnection) -> Result<(), sea_orm::DbErr> {
    if category::Entity::find().count(db).await? > 0 {
        tracing::info!("categories already present; skipping demo seed");
        return Ok(());
    }

    let electronics = match insert_category(db, "Electronics").await {
        Ok(model) => model,
        Err(err) if is_unique_violation(&err) => {
            tracing::info!("demo seed already applied by another worker");
            return Ok(());
        }
        Err(err) => return Err(err),
    };
    let books = insert_category(db, "Books").await?;
    let kitchen = insert_category(db, "Kitchen").await?;

    insert_item(db, "Mechanical Keyboard", electronics.id, true).await?;
    insert_item(db, "Noise-cancelling Headphones", electronics.id, false).await?;
    insert_item(db, "Dune", books.id, true).await?;
    insert_item(db, "The Rust Programming Language", books.id, false).await?;
    insert_item(db, "Cast Iron Skillet", kitchen.id, false).await?;

    tracing::info!("inserted demo categories and items");
    Ok(())
}

async fn insert_category(
    db: &DatabaseConnection,
    name: &str,
) -> Result<category::Model, sea_orm::DbErr> {
    category::ActiveModel {
        name: Set(name.to_owned()),
        ..Default::default()
    }
    .insert(db)
    .await
}

fn is_unique_violation(err: &sea_orm::DbErr) -> bool {
    let message = err.to_string();
    message.contains("Duplicate") || message.contains("1062")
}

fn is_duplicate_column(err: &sea_orm::DbErr) -> bool {
    let message = err.to_string();
    message.contains("Duplicate column") || message.contains("1060")
}

async fn insert_item(
    db: &DatabaseConnection,
    name: &str,
    category_id: i32,
    is_favourite: bool,
) -> Result<item::Model, sea_orm::DbErr> {
    item::ActiveModel {
        name: Set(name.to_owned()),
        category_id: Set(category_id),
        is_favourite: Set(is_favourite),
        ..Default::default()
    }
    .insert(db)
    .await
}
