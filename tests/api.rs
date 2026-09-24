use std::net::SocketAddr;

use reqwest::StatusCode;
use serde_json::{json, Value};
use wasmer_sqlx_demo::{build_router, connect, ensure_schema_and_seed, load_database_settings};

async fn spawn_app() -> Option<(String, reqwest::Client)> {
    let settings = match load_database_settings() {
        Ok(settings) => settings,
        Err(err) => {
            eprintln!("skipping API test: {err}");
            return None;
        }
    };

    let state = match connect(&settings).await {
        Ok(state) => state,
        Err(err) => {
            eprintln!("skipping API test: could not connect to MySQL: {err}");
            return None;
        }
    };

    ensure_schema_and_seed(&state)
        .await
        .expect("schema and seed should succeed");

    let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .expect("bind test listener");
    let addr = listener.local_addr().expect("local addr");
    let app = build_router(state);

    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("test server");
    });

    Some((format!("http://{addr}"), reqwest::Client::new()))
}

#[tokio::test]
async fn health_and_item_list_include_collections() {
    let Some((base, client)) = spawn_app().await else {
        return;
    };

    let health: Value = client
        .get(format!("{base}/health"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(health["status"], "ok");
    assert_eq!(health["database"], "up");

    let items: Vec<Value> = client
        .get(format!("{base}/items"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert!(
        !items.is_empty(),
        "seeded items should be returned from GET /items"
    );
    for item in &items {
        assert!(item["id"].is_number());
        assert!(item["name"].is_string());
        assert_eq!(item["collection"]["id"], item["category"]["id"]);
        assert_eq!(item["collection"]["name"], item["category"]["name"]);
        assert!(!item["collection"]["name"].as_str().unwrap().is_empty());
    }
}

#[tokio::test]
async fn can_create_item_in_existing_collection() {
    let Some((base, client)) = spawn_app().await else {
        return;
    };

    let categories: Vec<Value> = client
        .get(format!("{base}/categories"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let category_id = categories[0]["id"].as_i64().expect("category id");

    let created = client
        .post(format!("{base}/items"))
        .json(&json!({
            "name": "API Test Gadget",
            "category_id": category_id
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let body: Value = created.json().await.unwrap();
    assert_eq!(body["name"], "API Test Gadget");
    assert_eq!(body["collection"]["id"], category_id);
    let item_id = body["id"].as_i64().unwrap();

    let listed: Vec<Value> = client
        .get(format!("{base}/items"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(listed.iter().any(|item| item["id"] == item_id));

    let deleted = client
        .delete(format!("{base}/items/{item_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn category_round_trip() {
    let Some((base, client)) = spawn_app().await else {
        return;
    };

    let created = client
        .post(format!("{base}/categories"))
        .json(&json!({ "name": "API Test Collection" }))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::CREATED);
    let body: Value = created.json().await.unwrap();
    let category_id = body["id"].as_i64().unwrap();
    assert_eq!(body["name"], "API Test Collection");

    let fetched: Value = client
        .get(format!("{base}/collections/{category_id}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(fetched["name"], "API Test Collection");

    let deleted = client
        .delete(format!("{base}/categories/{category_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
}
