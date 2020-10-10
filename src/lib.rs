mod migration_0_initial;

use sqlx::{postgres::PgRow, prelude::*, PgPool};
use std::collections::HashSet;
use thiserror::Error;

#[derive(Default, Clone)]
/// A single database migration
pub struct Migration {
    pub name: String,
    pub up: Vec<String>,
    pub down: Vec<String>,
    pub mode: Mode,
}

#[derive(Error, Debug)]
/// An error executing a migration
pub struct MigrationError {
    pub statement: String,
    pub error: sqlx::Error,
}

#[derive(PartialEq, Clone)]
/// The migration's execution mode
pub enum Mode {
    /// The migration is stable and ready for deployment
    Stable,
    /// The migration is still being worked on and should not be deployed
    Debug,
    /// The migration is still being worked on and should not be deployed. This
    /// mode is mostly used to test complex migrations that alter existing
    /// structures to ensure the entire migration is re-playable
    NuclearDebug,
}

impl Default for Mode {
    fn default() -> Self {
        Mode::Stable
    }
}

macro_rules! migration_try {
    ($condition:expr, $stmt:expr) => {{
        match $condition {
            Ok(result) => result,
            Err(err) => {
                return Err(MigrationError {
                    statement: $stmt.to_owned(),
                    error: err,
                })
            }
        }
    }};
}

use std::fmt::{Display, Formatter};
impl Display for MigrationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Error executing sql \"{}\": {}",
            self.statement, self.error
        )
    }
}

impl Migration {
    /// Create an empty migration. `name` is used as a unique key to check if
    /// the migration has been completed already. If you are using
    /// `std::file!()` make sure to not change your build paths between
    /// deployments, or normalize the paths before passing them in as the
    /// migration name.
    pub fn new(name: &str) -> Self {
        Migration {
            name: name.to_owned(),
            ..Default::default()
        }
    }

    /// Add an "Up" sql statement that is performed when applying the migration
    pub fn with_up(mut self, up: &str) -> Self {
        self.up.push(up.to_owned());
        self
    }

    /// Add a "Down" sql statement that is performed when rolling a migration back
    pub fn with_down(mut self, down: &str) -> Self {
        self.down.insert(0, down.to_owned());
        self
    }

    /// Mark this migration as executing in debug mode. Will panic if `#[cfg(not(debug_assertions))]`
    pub fn debug(mut self) -> Self {
        #[cfg(not(debug_assertions))]
        panic!("Debug migration turned on");
        self.mode = Mode::Debug;
        self
    }

    /// Mark this migration as executing in "nuclear" debug mode, forcing all migrations to-rerun. Will panic if `#[cfg(not(debug_assertions))]`
    pub fn nuclear_debug(mut self) -> Self {
        #[cfg(not(debug_assertions))]
        panic!("Debug migration turned on");
        self.mode = Mode::NuclearDebug;
        self
    }

    /// Execute all of the migrations against the PgPool provided.
    pub async fn run_all(
        pool: &PgPool,
        mut supplied_migrations: Vec<Migration>,
    ) -> Result<(), MigrationError> {
        let mut migrations = vec![migration_0_initial::migration()];
        migrations.append(&mut supplied_migrations);
        let mut performed_migrations: HashSet<String> = HashSet::new();
        sqlx::query("SELECT name FROM migrations")
            .map(|row: PgRow| {
                performed_migrations.insert(row.get("name"));
            })
            .fetch_all(pool)
            .await
            .unwrap_or_default();

        if matches!(
            migrations.iter().find(|m| Mode::NuclearDebug == m.mode),
            Some(_)
        ) {
            // If any migration is nuclear, roll everything back, then execute all the migraitons again
            let mut reverse_migrations = migrations.clone();
            reverse_migrations.reverse();

            for migration in reverse_migrations {
                migration.undo(&pool).await?;
                performed_migrations.remove(&migration.name);
            }
            for migration in migrations {
                migration.perform(&pool).await?;
            }
        } else {
            for migration in migrations {
                if let Mode::Debug = migration.mode {
                    migration.undo(&pool).await?;
                    performed_migrations.remove(&migration.name);
                }

                if !performed_migrations.contains(&migration.name) {
                    migration.perform(&pool).await?;
                }
            }
        }

        Ok(())
    }

    async fn perform(&self, db: &PgPool) -> Result<(), MigrationError> {
        let mut tx = migration_try!(db.begin().await, "BEGIN TRANSACTION");
        println!("Performing {}", self.name);
        for statement in self.up.iter() {
            migration_try!(sqlx::query(statement).execute(&mut tx).await, statement);
        }
        migration_try!(
            sqlx::query("INSERT INTO migrations (name) VALUES ($1)")
                .bind(&self.name)
                .execute(&mut tx)
                .await,
            "INSERT INTO migrations (name) VALUES ($1)"
        );
        migration_try!(tx.commit().await, "COMMIT TRANSACTION");
        Ok(())
    }

    async fn undo(&self, db: &PgPool) -> Result<(), MigrationError> {
        let mut tx = migration_try!(db.begin().await, "BEGIN TRANSACTION");
        println!("Undoing {}", self.name);
        for statement in self.down.iter() {
            migration_try!(sqlx::query(statement).execute(&mut tx).await, statement);
        }
        // Only attempt to delete the migration record if we aren't the initial migration being undone.
        if self.name != migration_0_initial::NAME {
            migration_try!(
                sqlx::query("DELETE FROM migrations WHERE name = $1")
                    .bind(&self.name)
                    .execute(&mut tx)
                    .await,
                "DELETE FROM migrations WHERE name = $1"
            );
        }
        migration_try!(tx.commit().await, "COMMIT TRANSACTION");
        Ok(())
    }
}
