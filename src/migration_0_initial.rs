use super::Migration;

pub const NAME: &str = "initial";

pub fn migration() -> Migration {
    Migration::new(NAME)
        .with_up(
            r#"
        CREATE TABLE migrations (
            name TEXT NOT NULL PRIMARY KEY,
            executed_at TIMESTAMPTZ NOT NULL DEFAULT now()
        )
        "#,
        )
        .with_down(
            r#"
        DROP TABLE IF EXISTS migrations
        "#,
        )
}
