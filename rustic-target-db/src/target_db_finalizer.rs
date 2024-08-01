use colored::Colorize;
use deadpool_postgres::Pool;
use tracing::info;

pub struct TargetDBFinalizer {
    pub target_db_pool: Pool,
}

impl TargetDBFinalizer {
    pub fn new(target_db_pool: Pool) -> Self {
        Self { target_db_pool }
    }

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
}
