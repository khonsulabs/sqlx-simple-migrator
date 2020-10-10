# sqlx-simple-migrator

This crate is a very lightweight migration framework. It simply runs a series of sql commands in succession. It is not sophisticated.

Let's take a look at the built-in migration that the crate uses to create the table:

```rust
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
```

Right now, it's hard coded against TIMESTAMPTZ making this crate only suitable for Postgres. [@ecton](https://github.com/ecton) is only using this crate with Postgres, but would welcome any contributions to make this more generic.

Each `with_up` call is executed in the order it is added to the Migration structure. When rolling back a migration, the `with_down` instructions are operated in reverse order. This allows you to write `with_up` and `with_down` on a single-structure basis like the example above shows, keeping the up and down logic close together.

If you're working on a migration and want it to execute every time, just add `.debug()` to the builder pattern before returning it. `debug()` is not enabled on builds without `cfg(debug_assertions)` ensuring that if you build with `--release` for deploying, you will never accidentally deploy a migration that was still marked as being debugged.

Lastly, if you want to test rebuilding the database from scratch, you can use `.nuclear_debug()` instead, which will force every run to undo all migrations and redo them.

The pattern for executing migrations looks like this:

```rust
pub fn migrations() -> Vec<Migration> {
    vec![
        migration_0001_accounts::migration(),
        // ...
    ]
}

pub async fn run_all() -> Result<(), MigrationError> {
    let pool = connect_to_postgres();

    Migration::run_all(&pool, migrations()).await
}
```
