# Building the application

1. Install [WASIX](https://wasix.org/docs/language-guide/rust/installation): `cargo install cargo-wasix`
2. Install `lld`. The WASIX linker calls it directly (`apt install lld` on Debian/Ubuntu).
3. Build the application: `cargo wasix build --release`

The release module is written to `target/wasm32-wasmer-wasi/release/wasmer-sqlx-demo.wasm`. Re-run `wasmer deploy` after the build succeeds.
