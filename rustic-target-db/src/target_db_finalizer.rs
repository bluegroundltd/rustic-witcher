use bon::Builder;
use colored::Colorize;
use deadpool_postgres::Pool;
use std::env;
use tracing::info;

#[derive(Builder)]
pub struct TargetDBFinalizer {
    pub target_db_pool: Pool,
}

impl TargetDBFinalizer {
    /// Updates the sequence values by preparing the relevant SETVAL
    /// queries.
    ///
    /// Source: https://wiki.postgresql.org/wiki/Fixing_Sequences
    pub async fn update_sequence_values(&self, schema_name: &str) {
        let generate_sequence_value_update_queries = format!("\
            SELECT
                'SELECT SETVAL(' ||
                    quote_literal(quote_ident(sequence_namespace.nspname) || '.' || quote_ident(class_sequence.relname)) ||
                    ', COALESCE(MAX(' || quote_ident(pg_attribute.attname) || '), 1)::bigint ) FROM ' ||
                    quote_ident(table_namespace.nspname) || '.' || quote_ident(class_table.relname) || ';' AS query
            FROM pg_depend
                INNER JOIN pg_class AS class_sequence
                    ON class_sequence.oid = pg_depend.objid
                    AND class_sequence.relkind = 'S'
                INNER JOIN pg_class AS class_table
                    ON class_table.oid = pg_depend.refobjid
                INNER JOIN pg_attribute
                    ON pg_attribute.attrelid = class_table.oid
                    AND pg_depend.refobjsubid = pg_attribute.attnum
                INNER JOIN pg_namespace AS table_namespace
                    ON table_namespace.oid = class_table.relnamespace
                INNER JOIN pg_namespace AS sequence_namespace
                    ON sequence_namespace.oid = class_sequence.relnamespace
            WHERE table_namespace.nspname = '{schema_name}'
            ORDER BY sequence_namespace.nspname, class_sequence.relname;
        ");

        let target_db_client = self.target_db_pool.get().await.unwrap();

        let sequence_values_queries = target_db_client
            .query(&generate_sequence_value_update_queries.to_string(), &[])
            .await
            .unwrap();

        for result_line in sequence_values_queries {
            let query: String = result_line.get("query");

            info!(
                "{}: {}",
                "Updating sequence value with query".blue().bold(),
                query
            );
            target_db_client.execute(&query, &[]).await.unwrap();
        }
    }

    /// Grant permissions to the applications users we need for the target
    /// database.
    pub async fn grant_permissions_to_application_users(
        &self,
        database_name: &str,
        schema_name: &str,
        application_users: Vec<String>,
    ) {
        let grant_commands = application_users
            .iter()
            .flat_map(|user| {
                vec![
                    format!("grant usage on schema {db_schema} to {user}", db_schema = schema_name),
                    format!("grant create on schema {db_schema} to {user}", db_schema = schema_name),
                    format!("grant select, update, delete, insert on all tables in schema {db_schema} to {user}", db_schema = schema_name),
                    format!("grant connect, temp on database {db_name} to {user}", db_name = database_name),
                    format!("grant execute on all functions in schema {db_schema} to {user}", db_schema = schema_name),
                    format!("grant usage, select, update on all sequences in schema {db_schema} to {user}", db_schema = schema_name),
                    format!("alter default privileges in schema {db_schema} grant select, update, delete, insert on tables to {user}", db_schema = schema_name),
                    format!("alter default privileges in schema {db_schema} grant execute on functions to {user}", db_schema = schema_name),
                    format!("alter default privileges in schema {db_schema} grant usage, select on sequences to {user}", db_schema = schema_name),
                ]
            });

        let db_client = self.target_db_pool.get().await.unwrap();

        let joined_app_users = application_users.join(", ");

        info!("Granting permissions for users: {joined_app_users}");

        for command in grant_commands {
            db_client.execute(command.as_str(), &[]).await.unwrap();
        }
    }

    /// Executes post-import SQL queries if the flag is enabled.
    /// The queries are loaded from the {DATABASE_NAME}_{SCHEMA_NAME}_POST_IMPORT_SQL_QUERIES environment variable.
    /// Queries should be separated by semicolons.
    pub async fn execute_post_import_sql(&self, database_name: &str, schema_name: &str) {
        if !should_execute_post_import_sql() {
            info!(
                "{}",
                "Post-import SQL execution is disabled, skipping..."
                    .yellow()
                    .bold()
            );
            return;
        }

        info!(
            "{}",
            "Post-import SQL execution is enabled, loading queries..."
                .blue()
                .bold()
        );

        let queries_str = post_import_sql_queries(database_name, schema_name);

        if queries_str.trim().is_empty() {
            info!(
                "{}",
                "No post-import SQL queries found, skipping..."
                    .yellow()
                    .bold()
            );
            return;
        }

        // Split queries by semicolon and filter out empty strings
        let queries: Vec<&str> = queries_str
            .split(';')
            .map(|q| q.trim())
            .filter(|q| !q.is_empty())
            .collect();

        info!(
            "{}",
            format!("Found {} post-import SQL queries to execute", queries.len())
                .blue()
                .bold()
        );

        let db_client = self.target_db_pool.get().await.unwrap();

        for (index, query) in queries.iter().enumerate() {
            info!(
                "{}: [{}] {}",
                "Executing post-import SQL query".green().bold(),
                index + 1,
                query
            );

            match db_client.execute(*query, &[]).await {
                Ok(rows_affected) => {
                    info!(
                        "{}",
                        format!(
                            "Query {} executed successfully, {} rows affected",
                            index + 1,
                            rows_affected
                        )
                        .green()
                        .bold()
                    );
                }
                Err(e) => {
                    panic!(
                        "Failed to execute post-import SQL query {}: {} - Error: {:?}",
                        index + 1,
                        query,
                        e
                    );
                }
            }
        }

        info!(
            "{}",
            "Post-import SQL execution completed successfully"
                .green()
                .bold()
        );
    }
}

fn should_execute_post_import_sql() -> bool {
    env::var("POST_IMPORT_SQL_EXECUTION")
        .unwrap_or_else(|_| String::from("false"))
        .parse()
        .unwrap()
}

fn post_import_sql_queries(database_name: &str, schema_name: &str) -> String {
    let env_var_name = format!(
        "{}_{}_POST_IMPORT_SQL_QUERIES",
        database_name.to_uppercase(),
        schema_name.to_uppercase()
    );
    env::var(&env_var_name).unwrap_or_else(|_| String::from(""))
}
