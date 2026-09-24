use sea_orm::DatabaseConnection;
use sqlx::MySqlPool;

/// Shared application state.
///
/// The SQLx pool is the real connection. SeaORM is wrapped around a clone of
/// that pool so REST handlers can query through the ORM.
#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub pool: MySqlPool,
}
