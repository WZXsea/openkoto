use std::time::Duration;

use sqlx::{postgres::PgPoolOptions, PgPool};

use crate::{config::AppConfig, error::AppError};

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

pub async fn connect_and_migrate(config: &AppConfig) -> Result<PgPool, AppError> {
    tokio::fs::create_dir_all(&config.file_storage_dir).await?;

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&config.database_url)
        .await?;

    MIGRATOR.run(&pool).await?;

    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embeds_current_migrations() {
        let migration_count = MIGRATOR.iter().count();

        assert!(migration_count >= 8);
    }
}
