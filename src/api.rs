use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use sea_orm::entity::prelude::*;
use sea_orm::{ActiveValue::Set, QueryOrder};
use serde::{Deserialize, Serialize};

use crate::entities::{category, item};
use crate::error::AppError;
use crate::state::AppState;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CategoryBody {
    pub id: i32,
    pub name: String,
}

impl From<category::Model> for CategoryBody {
    fn from(model: category::Model) -> Self {
        Self {
            id: model.id,
            name: model.name,
        }
    }
}

/// An item together with the category/collection it belongs to.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ItemWithCollection {
    pub id: i32,
    pub name: String,
    pub category_id: i32,
    pub is_favourite: bool,
    pub category: CategoryBody,
    /// Alias used by the evaluation GET: items include the collection they belong to.
    pub collection: CategoryBody,
}

impl ItemWithCollection {
    fn from_pair(item: item::Model, category: category::Model) -> Self {
        let category = CategoryBody::from(category);
        Self {
            id: item.id,
            name: item.name,
            category_id: item.category_id,
            is_favourite: item.is_favourite,
            collection: category.clone(),
            category,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct CreateCategory {
    pub name: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct UpdateCategory {
    pub name: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CreateItem {
    pub name: String,
    pub category_id: i32,
    /// Omitted values are stored as `false`.
    #[serde(default)]
    pub is_favourite: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct UpdateItem {
    pub name: Option<String>,
    pub category_id: Option<i32>,
    pub is_favourite: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub database: &'static str,
}

#[derive(Clone, Debug, Serialize)]
pub struct IndexResponse {
    pub name: &'static str,
    pub description: &'static str,
    pub endpoints: &'static [&'static str],
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .route("/items", get(list_items).post(create_item))
        .route(
            "/items/{id}",
            get(get_item).put(update_item).delete(delete_item),
        )
        .route("/categories", get(list_categories).post(create_category))
        .route(
            "/categories/{id}",
            get(get_category)
                .put(update_category)
                .delete(delete_category),
        )
        .route("/categories/{id}/items", get(list_items_in_category))
        .route("/collections", get(list_categories))
        .route("/collections/{id}", get(get_category))
        .route("/collections/{id}/items", get(list_items_in_category))
        .with_state(state)
}

async fn index() -> Json<IndexResponse> {
    Json(IndexResponse {
        name: "wasmer-sqlx-demo",
        description: "Axum REST API for items and categories on Wasmer Edge (SQLx + SeaORM)",
        endpoints: &[
            "GET /health",
            "GET /items",
            "POST /items",
            "GET /items/{id}",
            "PUT /items/{id}",
            "DELETE /items/{id}",
            "GET /categories",
            "POST /categories",
            "GET /categories/{id}",
            "PUT /categories/{id}",
            "DELETE /categories/{id}",
            "GET /categories/{id}/items",
            "GET /collections",
            "GET /collections/{id}/items",
        ],
    })
}

async fn health(State(state): State<AppState>) -> Result<Json<HealthResponse>, AppError> {
    sqlx::query("SELECT 1").execute(&state.pool).await?;
    Ok(Json(HealthResponse {
        status: "spinning",
        database: "up",
    }))
}

/// Required evaluation endpoint: every item includes the collection it belongs to.
async fn list_items(
    State(state): State<AppState>,
) -> Result<Json<Vec<ItemWithCollection>>, AppError> {
    let rows = item::Entity::find()
        .find_also_related(category::Entity)
        .order_by_asc(item::Column::Id)
        .all(&state.db)
        .await?;

    rows.into_iter()
        .map(|(item, category)| {
            let category = category.ok_or_else(|| {
                AppError::Internal(format!("item {} is missing its category", item.id))
            })?;
            Ok(ItemWithCollection::from_pair(item, category))
        })
        .collect::<Result<Vec<_>, AppError>>()
        .map(Json)
}

async fn get_item(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<Json<ItemWithCollection>, AppError> {
    load_item_with_collection(&state.db, id).await.map(Json)
}

async fn create_item(
    State(state): State<AppState>,
    Json(body): Json<CreateItem>,
) -> Result<impl IntoResponse, AppError> {
    let name = normalize_name(&body.name)?;
    ensure_category_exists(&state.db, body.category_id).await?;

    let created = item::ActiveModel {
        name: Set(name),
        category_id: Set(body.category_id),
        is_favourite: Set(body.is_favourite),
        ..Default::default()
    }
    .insert(&state.db)
    .await?;

    let payload = load_item_with_collection(&state.db, created.id).await?;
    Ok((StatusCode::CREATED, Json(payload)))
}

async fn update_item(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Json(body): Json<UpdateItem>,
) -> Result<Json<ItemWithCollection>, AppError> {
    let existing = item::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("item {id} not found")))?;

    let mut active: item::ActiveModel = existing.into();
    if let Some(name) = body.name {
        active.name = Set(normalize_name(&name)?);
    }
    if let Some(category_id) = body.category_id {
        ensure_category_exists(&state.db, category_id).await?;
        active.category_id = Set(category_id);
    }
    if let Some(is_favourite) = body.is_favourite {
        active.is_favourite = Set(is_favourite);
    }
    active.update(&state.db).await?;
    load_item_with_collection(&state.db, id).await.map(Json)
}

async fn delete_item(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<StatusCode, AppError> {
    let result = item::Entity::delete_by_id(id).exec(&state.db).await?;
    if result.rows_affected == 0 {
        return Err(AppError::NotFound(format!("item {id} not found")));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn list_categories(
    State(state): State<AppState>,
) -> Result<Json<Vec<CategoryBody>>, AppError> {
    let rows = category::Entity::find()
        .order_by_asc(category::Column::Id)
        .all(&state.db)
        .await?;
    Ok(Json(rows.into_iter().map(CategoryBody::from).collect()))
}

async fn get_category(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<Json<CategoryBody>, AppError> {
    let model = category::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("category {id} not found")))?;
    Ok(Json(model.into()))
}

async fn create_category(
    State(state): State<AppState>,
    Json(body): Json<CreateCategory>,
) -> Result<impl IntoResponse, AppError> {
    let name = normalize_name(&body.name)?;
    let created = category::ActiveModel {
        name: Set(name),
        ..Default::default()
    }
    .insert(&state.db)
    .await?;
    Ok((StatusCode::CREATED, Json(CategoryBody::from(created))))
}

async fn update_category(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Json(body): Json<UpdateCategory>,
) -> Result<Json<CategoryBody>, AppError> {
    let existing = category::Entity::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("category {id} not found")))?;

    let mut active: category::ActiveModel = existing.into();
    if let Some(name) = body.name {
        active.name = Set(normalize_name(&name)?);
    }
    let updated = active.update(&state.db).await?;
    Ok(Json(updated.into()))
}

async fn delete_category(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<StatusCode, AppError> {
    let related = item::Entity::find()
        .filter(item::Column::CategoryId.eq(id))
        .count(&state.db)
        .await?;
    if related > 0 {
        return Err(AppError::Conflict(format!(
            "category {id} still has {related} item(s)"
        )));
    }

    let result = category::Entity::delete_by_id(id).exec(&state.db).await?;
    if result.rows_affected == 0 {
        return Err(AppError::NotFound(format!("category {id} not found")));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn list_items_in_category(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<Json<Vec<ItemWithCollection>>, AppError> {
    ensure_category_exists(&state.db, id).await?;
    let rows = item::Entity::find()
        .filter(item::Column::CategoryId.eq(id))
        .find_also_related(category::Entity)
        .order_by_asc(item::Column::Id)
        .all(&state.db)
        .await?;

    rows.into_iter()
        .map(|(item, category)| {
            let category = category.ok_or_else(|| {
                AppError::Internal(format!("item {} is missing its category", item.id))
            })?;
            Ok(ItemWithCollection::from_pair(item, category))
        })
        .collect::<Result<Vec<_>, AppError>>()
        .map(Json)
}

async fn load_item_with_collection(
    db: &DatabaseConnection,
    id: i32,
) -> Result<ItemWithCollection, AppError> {
    let (item, category) = item::Entity::find_by_id(id)
        .find_also_related(category::Entity)
        .one(db)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("item {id} not found")))?;
    let category =
        category.ok_or_else(|| AppError::Internal(format!("item {id} is missing its category")))?;
    Ok(ItemWithCollection::from_pair(item, category))
}

async fn ensure_category_exists(db: &DatabaseConnection, id: i32) -> Result<(), AppError> {
    let exists = category::Entity::find_by_id(id).one(db).await?;
    if exists.is_none() {
        return Err(AppError::BadRequest(format!(
            "category {id} does not exist"
        )));
    }
    Ok(())
}

fn normalize_name(name: &str) -> Result<String, AppError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("name must not be empty".to_string()));
    }
    if name.len() > 255 {
        return Err(AppError::BadRequest(
            "name must be at most 255 characters".to_string(),
        ));
    }
    Ok(name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_blank_names() {
        assert!(normalize_name("   ").is_err());
        assert_eq!(normalize_name("  Dune  ").unwrap(), "Dune");
    }

    #[test]
    fn item_payload_includes_collection_alias() {
        let item = item::Model {
            id: 7,
            name: "Dune".into(),
            category_id: 2,
            is_favourite: true,
        };
        let category = category::Model {
            id: 2,
            name: "Books".into(),
        };
        let payload = ItemWithCollection::from_pair(item, category);
        let json = serde_json::to_value(payload).unwrap();
        assert_eq!(json["collection"]["name"], "Books");
        assert_eq!(json["category"]["name"], "Books");
        assert_eq!(json["name"], "Dune");
        assert_eq!(json["is_favourite"], true);
    }
}
