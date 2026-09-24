# wasmer-sqlx-demo

A small Axum REST API for evaluating **WASIX** deployments of Rust web backends on **Wasmer Edge**.

The app manages items and categories:

- A **category** (also exposed as a *collection*) has a name.
- An **item** has a name, belongs to one category, and has an `is_favourite` flag.

On startup the process:

1. Opens a **SQLx** MySQL pool (Wasmer injects `DB_*` credentials).
2. Wraps that pool in **SeaORM** and uses SeaORM for all CRUD queries.
3. Creates the `categories` and `items` tables if they do not exist, and adds `items.is_favourite` when that column is missing.
4. Inserts a handful of demo rows when the database is empty.

The Tokio runtime is the multi-threaded scheduler (`#[tokio::main(flavor = "multi_thread")]`).

## API

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/` | Service index |
| `GET` | `/health` | Liveness plus a SQLx `SELECT 1` |
| `GET` | `/items` | **Items with the collection they belong to** |
| `POST` | `/items` | Create an item `{ "name", "category_id", "is_favourite"? }` |
| `GET` | `/items/{id}` | One item, including its collection |
| `PUT` | `/items/{id}` | Update name, `category_id`, and/or `is_favourite` |
| `DELETE` | `/items/{id}` | Delete an item |
| `GET` | `/categories` | List categories |
| `POST` | `/categories` | Create `{ "name" }` |
| `GET` | `/categories/{id}` | One category |
| `PUT` | `/categories/{id}` | Rename a category |
| `DELETE` | `/categories/{id}` | Delete if it has no items |
| `GET` | `/categories/{id}/items` | Items in one collection |
| `GET` | `/collections` | Alias of `/categories` |
| `GET` | `/collections/{id}/items` | Alias of `/categories/{id}/items` |

`GET /items` returns JSON like:

```json
[
  {
    "id": 1,
    "name": "Mechanical Keyboard",
    "category_id": 1,
    "is_favourite": true,
    "category": { "id": 1, "name": "Electronics" },
    "collection": { "id": 1, "name": "Electronics" }
  }
]
```

`collection` is the same object as `category`. Categories are the collections items belong to.

## Prerequisites

Install these on the machine you will build and deploy from:

1. **Rust** via [rustup](https://rustup.rs/) (stable; 1.85 or newer).
2. **Wasmer CLI** — see [Install Wasmer](https://docs.wasmer.io/install).
3. **cargo-wasix** for the WASIX target:

   ```bash
   cargo install cargo-wasix
   ```

   The WASIX Rust toolchain is downloaded the first time you run `cargo wasix build`.

4. A [Wasmer](https://wasmer.io/) account.

MySQL is **not** something you provision yourself on Edge. Wasmer creates one managed MySQL database for the app and injects connection settings.

## Deploy to Wasmer Edge

Follow these steps in order.

### 1. Log in to the Wasmer registry

```bash
wasmer login
```

Complete the browser flow so the CLI can publish the package and create the Edge app.

### 2. Clone this repository and enter it

```bash
git clone https://github.com/danac/wasmer-sqlx-demo.git
cd wasmer-sqlx-demo
```

### 3. Choose the app owner (first deploy only)

`app.yaml` is the shared Edge app manifest. Leave `owner` commented out. It identifies your Wasmer account, so it does not belong in the committed file. Pass it on the command line in the deploy step below, or leave it unset and answer the prompt from `wasmer deploy`.

Optionally change:

- `name` — public app name (`https://<name>-<owner>.wasmer.app`)
- `locality.regions` — must be **exactly one** database-capable region:
  - `us-losa1` (Los Angeles, MySQL)
  - `fr-roub1` (Roubaix, MySQL)
  - `ca-beau1` (Beauharnois, MySQL)

Apps with a database cannot list multiple regions.

### 4. Build the WASIX module

```bash
cargo wasix build --release
```

This compiles `wasmer-sqlx-demo` to:

```text
target/wasm32-wasmer-wasi/release/wasmer-sqlx-demo.wasm
```

`wasmer.toml` points the package at that file.

If the WASIX build fails on the first try:

```bash
cargo update
cargo wasix build --release
```

WASIX often needs the crate graph that the WASIX registry (or the `wasix-org` forks) can compile. `cargo-wasix` writes `.cargo/config.toml` on first use so later builds resolve those crates automatically.

### 5. Deploy

```bash
wasmer deploy --owner YOUR_WASMER_USERNAME --no-persist-id
```

`--owner` selects the account without writing it into `app.yaml`. `--no-persist-id` stops the CLI from adding `app_id`. Later deploys still update the same app, because the name `wasmer-sqlx-demo` already exists under that owner.

A plain `wasmer deploy` appends `owner` and `app_id` to the bottom of `app.yaml`. Delete those two lines before committing. Keep `app.yaml` in git: the database capability, region, and package are the shared manifest. Do not gitignore the whole file.

On the first deploy Wasmer will:

1. Publish the local package described by `wasmer.toml`.
2. Create the Edge app from `app.yaml`.
3. Provision **MySQL** because of:

   ```yaml
   capabilities:
     database:
       engine: mysql
   ```

4. Inject these environment variables into the running app (there is **no** `DATABASE_URL`):

   | Variable | Meaning |
   | --- | --- |
   | `DB_HOST` | Regional MySQL hostname (`db.…`) |
   | `DB_PORT` | Non-default managed port |
   | `DB_NAME` | Database name |
   | `DB_USERNAME` | User |
   | `DB_PASSWORD` | Password |

The app builds a SQLx `MySqlConnectOptions` from those values. Wasmer MySQL uses TLS with a **private CA**, so the client sets `MySqlSslMode::Required` (encrypt, do not verify the chain). Do not turn TLS off.

Later deploys reuse the same database.

### 6. Wait until the deployment is ready

`wasmer deploy` prints the public URL, for example:

```text
https://wasmer-sqlx-demo-<owner>.wasmer.app
```

### 7. Call the API

```bash
export APP_URL=https://wasmer-sqlx-demo-<owner>.wasmer.app

curl -s "$APP_URL/health"
curl -s "$APP_URL/items"
curl -s "$APP_URL/collections"

curl -s -X POST "$APP_URL/categories" \
  -H 'content-type: application/json' \
  -d '{"name":"Garden"}'

curl -s -X POST "$APP_URL/items" \
  -H 'content-type: application/json' \
  -d '{"name":"Trowel","category_id":4,"is_favourite":true}'
```

`GET /items` is the required evaluation endpoint: each item includes the collection it belongs to.

### 8. Inspect the database (optional)

```bash
wasmer app database list --with-password
```

The Wasmer dashboard also has a Databases tab and a DB explorer.

To rotate credentials after a leak, use **Rotate Credentials** on that tab. The app picks up the new `DB_USERNAME` / `DB_PASSWORD` without a new deployment.

## Run the WASIX binary locally

After `cargo wasix build --release`:

```bash
wasmer run target/wasm32-wasmer-wasi/release/wasmer-sqlx-demo.wasm \
  --net \
  --env PORT=3000 \
  --env DATABASE_URL=mysql://demo:demo@127.0.0.1:3306/items_demo \
  --env DB_SSL_MODE=required
```

`--net` and threads (`wasmer-extra-flags` in `wasmer.toml`) are required for the multi-threaded Tokio server and outbound MySQL. The guest still needs a reachable MySQL server (local or remote).

Use `DB_SSL_MODE=required` against the Docker `mysql:8` server. That image turns TLS on by itself. `required` encrypts the connection and does not verify the certificate, which is the same mode Wasmer Edge uses. `disabled` fails: MySQL 8's default `caching_sha2_password` login then needs an RSA password exchange, and this build does not include SQLx's `mysql-rsa` feature.

## Native local development

Useful while iterating on the API without the WASIX toolchain.

### 1. Start MySQL

Example with Docker:

```bash
docker run --name wasmer-sqlx-mysql \
  -e MYSQL_DATABASE=items_demo \
  -e MYSQL_USER=demo \
  -e MYSQL_PASSWORD=demo \
  -e MYSQL_ROOT_PASSWORD=root \
  -p 3306:3306 \
  -d mysql:8
```

### 2. Configure the process

```bash
cp .env.example .env
# edit DATABASE_URL / PORT if needed
```

### 3. Run

```bash
cargo run
```

The server listens on `127.0.0.1:80` by default (Wasmer Edge’s `PORT`). Locally set `PORT=3000` as in `.env.example`.

```bash
curl -s http://127.0.0.1:3000/items
```

### 4. Tests

Unit tests always run:

```bash
cargo test
```

HTTP tests start a real listener and talk to MySQL. They skip (rather than fail) when `DATABASE_URL` or the Wasmer `DB_*` variables are missing, or when MySQL is unreachable.

```bash
DATABASE_URL=mysql://demo:demo@127.0.0.1:3306/items_demo cargo test
```

## Environment variables

| Variable | Used when | Notes |
| --- | --- | --- |
| `DATABASE_URL` | Local / WASIX-on-your-machine | `mysql://user:pass@host:port/db` |
| `DB_HOST`, `DB_PORT`, `DB_NAME`, `DB_USERNAME`, `DB_PASSWORD` | Wasmer Edge | Injected by the platform |
| `DB_SSL_MODE` | Optional override | `disabled`, `preferred`, `required`, `verify_ca`, `verify_identity`. Default is `required` for `db.*` / `*wasmer*` hosts, otherwise `preferred`. For local Docker MySQL 8, set `required`. `disabled` cannot log in with `caching_sha2_password` unless the client is built with SQLx `mysql-rsa`. |
| `PORT` | Always | Default `80` for Edge |
| `BIND_ADDR` | Optional | Default `127.0.0.1` (what Edge proxies to) |

`DATABASE_URL` wins when both styles are set.

## How SQLx and SeaORM are wired

```text
Wasmer DB_*  ──►  sqlx::MySqlPool  ──►  sea_orm::SqlxMySqlConnector
                      │                         │
                      │ health probe            │ schema, seed, CRUD
                      ▼                         ▼
                 SELECT 1                 SeaORM entities
```

The pool is created with SQLx. A clone of that pool is turned into a SeaORM `DatabaseConnection`. Handlers use SeaORM (`find_also_related`, `insert`, `update`, `delete`). `/health` uses the SQLx pool directly so both layers stay on the same connections.

## Project layout

```text
src/main.rs          multi-threaded Tokio entrypoint
src/config.rs        env + TLS mode
src/db.rs            SQLx connect, SeaORM schema + seed
src/entities/        SeaORM category and item models
src/api.rs           Axum routes and handlers
app.yaml             Wasmer Edge app + MySQL capability
wasmer.toml          WASIX package pointing at the wasm module
BUILD.md             commands `wasmer deploy` expects
```

## Notes for WASIX evaluation

- Compile with `cargo wasix`, not `cargo build --target wasm32-wasi`.
- Prefer **rustls** (`runtime-tokio-rustls` / `tls-rustls`). OpenSSL `native-tls` is a poor fit for WASIX.
- Do not use SQLx `query!` macros against Edge: they need a compile-time `DATABASE_URL` and offline metadata. Runtime queries through SeaORM avoid that.
- One managed database per app. The engine cannot be changed in place.
- Wasmer MySQL does not listen on `3306`. Always read `DB_PORT`.
