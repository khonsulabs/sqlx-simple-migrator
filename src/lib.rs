mod migration_0_initial;

use sqlx::{postgres::PgRow, prelude::*, PgPool};
use std::collections::HashSet;

#[derive(Default, Clone)]
pub struct Migration {
    pub name: String,
    pub up: Vec<String>,
    pub down: Vec<String>,
    pub mode: Mode,
}

use thiserror::Error;
#[derive(Error, Debug)]
pub struct MigrationError {
    pub statement: String,
    pub error: sqlx::Error,
}

#[derive(PartialEq, Clone)]
pub enum Mode {
    Stable,
    Debug,
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
    pub fn new(name: &str) -> Self {
        Migration {
            name: name.to_owned(),
            ..Default::default()
        }
    }

    pub fn with_up(mut self, up: &str) -> Self {
        self.up.push(up.to_owned());
        self
    }

    pub fn with_down(mut self, down: &str) -> Self {
        self.down.insert(0, down.to_owned());
        self
    }

    pub fn debug(mut self) -> Self {
        #[cfg(not(debug_assertions))]
        panic!("Debug migration turned on");
        self.mode = Mode::Debug;
        self
    }

    pub fn nuclear_debug(mut self) -> Self {
        #[cfg(not(debug_assertions))]
        panic!("Debug migration turned on");
        self.mode = Mode::NuclearDebug;
        self
    }

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

        if let Some(_) = migrations.iter().find(|m| Mode::NuclearDebug == m.mode) {
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
