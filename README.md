# wasmer-sqlx-demo

A small Axum REST API for evaluating **WASIX** deployments of Rust web backends on **Wasmer Edge**.

The app manages items and categories:

- A **category** (also exposed as a *collection*) has a name.
- An **item** has a name and belongs to one category.

On startup the process:

1. Opens a **SQLx** MySQL pool (Wasmer injects `DB_*` credentials).
2. Wraps that pool in **SeaORM** and uses SeaORM for all CRUD queries.
3. Creates the `categories` and `items` tables if they do not exist.
4. Inserts a handful of demo rows when the database is empty.

The Tokio runtime is the multi-threaded scheduler (`#[tokio::main(flavor = "multi_thread")]`).

## API

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/` | Service index |
| `GET` | `/health` | Liveness plus a SQLx `SELECT 1` |
| `GET` | `/items` | **Items with the collection they belong to** |
| `POST` | `/items` | Create an item `{ "name", "category_id" }` |
| `GET` | `/items/{id}` | One item, including its collection |
| `PUT` | `/items/{id}` | Update name and/or `category_id` |
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

### 3. Set the app owner (first deploy only)

`app.yaml` is the Edge app manifest. Uncomment and set `owner` to your Wasmer username or namespace:

```yaml
owner: YOUR_WASMER_USERNAME
```

You can also leave it unset and answer the prompt from `wasmer deploy`.

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
wasmer deploy
```

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
  -d '{"name":"Trowel","category_id":4}'
```

`GET /items` is the required evaluation endpoint: each item includes the collection it belongs to.

### 8. Inspect the database (optional)

```bash
wasmer app database list --with-password
```

The Wasmer dashboard also has a Databases tab and a DB explorer.

To rotate credentials after a leak, use **Rotate Credentials** on that tab. The app picks up the new `DB_USERNAME` / `DB_PASSWORD` without a new deployment.

## Deploy from GitHub Actions

Yes: the Wasmer CLI accepts a registry token, so a workflow can deploy without `wasmer login`. `.github/workflows/deploy-wasmer.yml` builds the WASIX module and runs:

```bash
wasmer deploy --non-interactive --publish-package --bump --owner "$WASMER_OWNER"
```

`WASMER_TOKEN` is read from the environment (never print it).

### 1. Create a Wasmer API token

1. Open [Wasmer access tokens](https://wasmer.io/settings/access-tokens).
2. Create a token that can publish packages and deploy apps.
3. Copy it once. Do not commit it, and do not paste it into a chat or the workflow file.

### 2. Add GitHub repository secrets

In the GitHub repo: **Settings → Secrets and variables → Actions**:

| Secret | Value |
| --- | --- |
| `WASMER_TOKEN` | the token from the previous step |
| `WASMER_OWNER` | your Wasmer username or namespace |

### 3. Run the workflow

- A push to `main` deploys.
- **Actions → Deploy to Wasmer Edge → Run workflow** deploys the selected branch.

The job installs Rust and `cargo-wasix`, runs `cargo wasix build --release`, then `wasmer deploy`. The first WASIX toolchain download can take a while.

`--publish-package` is required in CI because `app.yaml` has `package: .` and there is nobody to answer the “publish this package?” prompt. `--bump` patches the package version so each run can publish again. `--owner` supplies the account that `app.yaml` leaves commented out.

If you already connected this repository in the Wasmer dashboard (Import Git), pushes can deploy from that link instead. You do not need both; pick Actions **or** the dashboard Git integration.

## Run the WASIX binary locally

After `cargo wasix build --release`:

```bash
wasmer run target/wasm32-wasmer-wasi/release/wasmer-sqlx-demo.wasm \
  --net \
  --env PORT=3000 \
  --env DATABASE_URL=mysql://demo:demo@127.0.0.1:3306/items_demo \
  --env DB_SSL_MODE=disabled
```

`--net` and threads (`wasmer-extra-flags` in `wasmer.toml`) are required for the multi-threaded Tokio server and outbound MySQL. The guest still needs a reachable MySQL server (local or remote).

## Native local development

Useful while iterating on the API without the WASIX toolchain. Starting MySQL is only the first step: the app still needs env vars, a TCP connection string, TLS turned off for local Docker, and `cargo run`.

The process talks to MySQL over **TCP** (`127.0.0.1:3306`). It does **not** use a Unix socket. `mysql://…@127.0.0.1:3306/…` in `.env` is required; a socket path such as `/tmp/mysql.sock` will not be read.

### 1. Start MySQL with Docker

Docker does not infer MySQL from the flags. `mysql:8` is an image from Docker Hub that already contains MySQL Server 8. Its default process is `mysqld` (via the image `ENTRYPOINT` / `CMD`). The `-e MYSQL_*` variables are read by that image’s entrypoint on first boot to create `items_demo` and the `demo` user. `-p 3306:3306` only publishes the port the server already listens on; `--name` is just the container name.

```bash
docker run --name wasmer-sqlx-mysql \
  -e MYSQL_DATABASE=items_demo \
  -e MYSQL_USER=demo \
  -e MYSQL_PASSWORD=demo \
  -e MYSQL_ROOT_PASSWORD=root \
  -p 3306:3306 \
  -d mysql:8
```

If you already created that container, start it again instead of `docker run`:

```bash
docker start wasmer-sqlx-mysql
```

The image takes several seconds to initialize. Wait until it accepts TCP connections:

```bash
until docker exec wasmer-sqlx-mysql \
  mysqladmin ping -h127.0.0.1 -uroot -proot --silent
do
  sleep 1
done
```

You can confirm the demo user works from the host (TCP, not the socket):

```bash
docker exec wasmer-sqlx-mysql \
  mysql -h127.0.0.1 -udemo -pdemo items_demo -e 'SELECT 1'
```

If host port `3306` is already taken, map another port (`-p 3307:3306`) and put that port in `DATABASE_URL`.

### 2. Configure the process

```bash
cp .env.example .env
```

`.env.example` already has values that match the container above:

```bash
DATABASE_URL=mysql://demo:demo@127.0.0.1:3306/items_demo
PORT=3000
BIND_ADDR=127.0.0.1
DB_SSL_MODE=disabled
```

`DB_SSL_MODE=disabled` is required for this Docker image: it does not present a client TLS setup that matches Wasmer’s managed MySQL. Without it, SQLx may try `preferred` and fail the handshake.

Without `PORT=3000`, `cargo run` binds `127.0.0.1:80` (the Edge default) and will fail unless you are root or something else is on port 80.

### 3. Run the API

```bash
cargo run
```

The process opens a SQLx pool, creates `categories` / `items` if they are missing, seeds demo rows when the tables are empty, then listens on `http://127.0.0.1:3000`.

```bash
curl -s http://127.0.0.1:3000/health
curl -s http://127.0.0.1:3000/items
```

`GET /items` should return the seeded rows, each with a `collection` object.

### 4. Tests

Unit tests always run:

```bash
cargo test
```

HTTP tests start a real listener and talk to MySQL. They skip (rather than fail) when `DATABASE_URL` or the Wasmer `DB_*` variables are missing, or when MySQL is unreachable.

```bash
DATABASE_URL=mysql://demo:demo@127.0.0.1:3306/items_demo \
  DB_SSL_MODE=disabled \
  cargo test
```

## Environment variables

| Variable | Used when | Notes |
| --- | --- | --- |
| `DATABASE_URL` | Local / WASIX-on-your-machine | `mysql://user:pass@host:port/db` |
| `DB_HOST`, `DB_PORT`, `DB_NAME`, `DB_USERNAME`, `DB_PASSWORD` | Wasmer Edge | Injected by the platform |
| `DB_SSL_MODE` | Optional override | `disabled`, `preferred`, `required`, `verify_ca`, `verify_identity`. Default is `required` for `db.*` / `*wasmer*` hosts, otherwise `preferred`. |
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
