use std::env;
use std::net::IpAddr;
use std::str::FromStr;

use anyhow::{anyhow, Context};
use percent_encoding::percent_decode_str;
use sqlx::mysql::{MySqlConnectOptions, MySqlSslMode};

/// Runtime settings loaded from the process environment.
#[derive(Clone, Debug)]
pub struct AppSettings {
    pub bind_ip: IpAddr,
    pub port: u16,
    pub db: DatabaseSettings,
}

/// MySQL coordinates used to open a SQLx pool.
#[derive(Clone, Debug)]
pub struct DatabaseSettings {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: String,
    pub ssl_mode: MySqlSslMode,
}

impl DatabaseSettings {
    pub fn to_connect_options(&self) -> MySqlConnectOptions {
        MySqlConnectOptions::new()
            .host(&self.host)
            .port(self.port)
            .database(&self.database)
            .username(&self.username)
            .password(&self.password)
            .ssl_mode(self.ssl_mode)
    }
}

pub fn load_settings() -> anyhow::Result<AppSettings> {
    let bind_ip = env::var("BIND_ADDR")
        .ok()
        .map(|value| value.parse::<IpAddr>())
        .transpose()
        .context("BIND_ADDR must be a valid IP address")?
        .unwrap_or_else(|| IpAddr::from([127, 0, 0, 1]));

    let port = env::var("PORT")
        .ok()
        .map(|value| value.parse::<u16>())
        .transpose()
        .context("PORT must be a valid TCP port")?
        .unwrap_or(80);

    Ok(AppSettings {
        bind_ip,
        port,
        db: load_database_settings()?,
    })
}

pub fn load_database_settings() -> anyhow::Result<DatabaseSettings> {
    if let Ok(url) = env::var("DATABASE_URL") {
        return database_settings_from_url(&url);
    }

    let host = env::var("DB_HOST").context(
        "set DATABASE_URL or the Wasmer database variables DB_HOST, DB_PORT, DB_NAME, DB_USERNAME, and DB_PASSWORD",
    )?;
    let port = env::var("DB_PORT")
        .unwrap_or_else(|_| "3306".to_string())
        .parse::<u16>()
        .context("DB_PORT must be a valid TCP port")?;
    let database = env::var("DB_NAME").context("DB_NAME is required when DATABASE_URL is unset")?;
    let username =
        env::var("DB_USERNAME").context("DB_USERNAME is required when DATABASE_URL is unset")?;
    let password = env::var("DB_PASSWORD").unwrap_or_default();
    let ssl_mode = ssl_mode_for_host(&host, env::var("DB_SSL_MODE").ok().as_deref());

    Ok(DatabaseSettings {
        host,
        port,
        database,
        username,
        password,
        ssl_mode,
    })
}

pub fn database_settings_from_url(url: &str) -> anyhow::Result<DatabaseSettings> {
    let options = MySqlConnectOptions::from_str(url)
        .with_context(|| format!("invalid DATABASE_URL: {url}"))?;
    let parsed = url::Url::parse(url).with_context(|| format!("invalid DATABASE_URL: {url}"))?;
    let host = options.get_host().to_string();
    if host.is_empty() {
        return Err(anyhow!("DATABASE_URL is missing a host"));
    }
    let ssl_override = env::var("DB_SSL_MODE").ok();
    let ssl_mode = ssl_mode_for_host(&host, ssl_override.as_deref());
    let password = parsed
        .password()
        .map(|value| percent_decode_str(value).decode_utf8_lossy().into_owned())
        .unwrap_or_default();

    Ok(DatabaseSettings {
        host,
        port: options.get_port(),
        database: options
            .get_database()
            .ok_or_else(|| anyhow!("DATABASE_URL is missing a database name"))?
            .to_string(),
        username: options.get_username().to_string(),
        password,
        ssl_mode,
    })
}

/// Wasmer managed MySQL uses a private CA, so the client must encrypt
/// without verifying the certificate chain. Local MySQL usually has no TLS.
pub fn ssl_mode_for_host(host: &str, override_mode: Option<&str>) -> MySqlSslMode {
    if let Some(value) = override_mode {
        return parse_ssl_mode(value).unwrap_or_else(|| default_ssl_mode(host));
    }
    default_ssl_mode(host)
}

pub fn parse_ssl_mode(value: &str) -> Option<MySqlSslMode> {
    match value.trim().to_ascii_lowercase().as_str() {
        "disabled" | "disable" | "off" | "false" => Some(MySqlSslMode::Disabled),
        "preferred" | "prefer" => Some(MySqlSslMode::Preferred),
        "required" | "require" | "on" | "true" => Some(MySqlSslMode::Required),
        "verify_ca" | "verify-ca" => Some(MySqlSslMode::VerifyCa),
        "verify_identity" | "verify-identity" => Some(MySqlSslMode::VerifyIdentity),
        _ => None,
    }
}

fn default_ssl_mode(host: &str) -> MySqlSslMode {
    let host = host.to_ascii_lowercase();
    if host.starts_with("db.") || host.contains("wasmer") {
        MySqlSslMode::Required
    } else {
        MySqlSslMode::Preferred
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wasmer_hosts_require_tls_without_cert_verification() {
        assert!(matches!(
            ssl_mode_for_host("db.us-losa1.wasmer.app", None),
            MySqlSslMode::Required
        ));
    }

    #[test]
    fn local_hosts_prefer_tls_but_allow_plaintext() {
        assert!(matches!(
            ssl_mode_for_host("127.0.0.1", None),
            MySqlSslMode::Preferred
        ));
    }

    #[test]
    fn explicit_ssl_mode_wins() {
        assert!(matches!(
            ssl_mode_for_host("db.example.wasmer.app", Some("disabled")),
            MySqlSslMode::Disabled
        ));
        assert!(matches!(
            ssl_mode_for_host("localhost", Some("required")),
            MySqlSslMode::Required
        ));
    }

    #[test]
    fn parses_database_url() {
        let settings = database_settings_from_url("mysql://demo:s3cret@127.0.0.1:3307/items_demo")
            .expect("url should parse");
        assert_eq!(settings.host, "127.0.0.1");
        assert_eq!(settings.port, 3307);
        assert_eq!(settings.database, "items_demo");
        assert_eq!(settings.username, "demo");
        assert_eq!(settings.password, "s3cret");
    }
}
