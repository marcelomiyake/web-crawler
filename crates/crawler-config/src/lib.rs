use anyhow::{Context, Result};
use sqlx::postgres::PgConnectOptions;
use std::str::FromStr;
use std::time::Duration;

/// Accept a developer DATABASE_URL or discrete Secret-backed DB_* variables.
/// Constructing options field-by-field avoids URL-encoding ambiguity for a
/// database password and prevents connection details from being logged.
pub fn postgres_options() -> Result<PgConnectOptions> {
    if let Ok(url) = std::env::var("DATABASE_URL") {
        return PgConnectOptions::from_str(&url)
            .context("DATABASE_URL is not a valid PostgreSQL connection URL");
    }

    let host = required("DB_HOST")?;
    let port = std::env::var("DB_PORT")
        .unwrap_or_else(|_| "5432".to_owned())
        .parse::<u16>()
        .context("DB_PORT must be a valid TCP port")?;
    let database = required("DB_NAME")?;
    let username = required("DB_USER")?;
    let password = required("DB_PASSWORD")?;

    Ok(PgConnectOptions::new()
        .host(&host)
        .port(port)
        .database(&database)
        .username(&username)
        .password(&password)
        .application_name("web-crawler"))
}

pub fn database_connect_timeout() -> Duration {
    Duration::from_secs(5)
}

fn required(name: &str) -> Result<String> {
    std::env::var(name).with_context(|| format!("required environment variable {name} is missing"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENVIRONMENT_LOCK: Mutex<()> = Mutex::new(());

    fn set(name: &str, value: &str) {
        // The test holds ENVIRONMENT_LOCK so this process has no concurrent env access.
        unsafe { std::env::set_var(name, value) };
    }

    fn remove(name: &str) {
        // The test holds ENVIRONMENT_LOCK so this process has no concurrent env access.
        unsafe { std::env::remove_var(name) };
    }

    #[test]
    fn parses_url_and_secret_backed_settings_and_reports_invalid_configuration() {
        let _guard = ENVIRONMENT_LOCK.lock().unwrap();
        for name in [
            "DATABASE_URL",
            "DB_HOST",
            "DB_PORT",
            "DB_NAME",
            "DB_USER",
            "DB_PASSWORD",
        ] {
            remove(name);
        }

        assert!(
            postgres_options().is_err(),
            "missing required values are rejected"
        );
        set("DB_HOST", "postgres.local");
        set("DB_NAME", "crawler");
        set("DB_USER", "worker");
        set("DB_PASSWORD", "secret with punctuation:/@");
        assert!(postgres_options().is_ok(), "the default port is accepted");

        set("DB_PORT", "not-a-port");
        assert!(postgres_options().is_err());
        set("DB_PORT", "5544");
        assert!(postgres_options().is_ok());

        set(
            "DATABASE_URL",
            "postgres://crawler:secret@127.0.0.1/crawler",
        );
        assert!(postgres_options().is_ok());
        set("DATABASE_URL", "not a database URL");
        assert!(postgres_options().is_err());
        assert_eq!(database_connect_timeout(), Duration::from_secs(5));

        for name in [
            "DATABASE_URL",
            "DB_HOST",
            "DB_PORT",
            "DB_NAME",
            "DB_USER",
            "DB_PASSWORD",
        ] {
            remove(name);
        }
    }
}
