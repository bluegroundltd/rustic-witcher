use std::env;

use colored::Colorize;
use deadpool_postgres::{GenericClient, Pool};
use dms_cdc_operator::{
    cdc::snapshot_payload::CDCOperatorSnapshotPayload,
    postgres::{postgres_operator::PostgresOperator, postgres_operator_impl::PostgresOperatorImpl},
};
use rustic_shell::shell_command_executor::ShellCommandExecutor;
use tokio::fs;
use tracing::{debug, error, info};

const RUSTIC_WITCHER_APP_NAME_PREFIX: &str = "rustic_witcher";
const PG_DUMP_FILE_NAME: &str = "pg_dump.sql";

pub struct TargetDbPreparator {
    pub source_db_pool: Pool,
    pub target_db_pool: Pool,
}

impl TargetDbPreparator {
    /// Asynchronously dumps the schema of a PostgreSQL database.
    ///
    /// # Arguments
    ///
    /// * `cdc_operator_snapshot_payload` - The CDC operator snapshot payload.
    pub async fn pg_dump_schema(&self, cdc_operator_snapshot_payload: &CDCOperatorSnapshotPayload) {
        let app_name = format!(
            "{RUSTIC_WITCHER_APP_NAME_PREFIX}_pg_dump_{db_name}_{db_schema}",
            db_name = cdc_operator_snapshot_payload.database_name(),
            db_schema = cdc_operator_snapshot_payload.schema_name()
        );
        env::set_var("PGAPPNAME", app_name);

        let dump_command = format!(
            "pg_dump --verbose --no-owner --no-privileges --schema-only --schema={schema_name} --dbname={db_name} --format=c -f {PG_DUMP_FILE_NAME}",
            schema_name = cdc_operator_snapshot_payload.schema_name(),
            db_name = cdc_operator_snapshot_payload.source_postgres_url(),
        );

        ShellCommandExecutor::execute_cmd(&dump_command).await
    }

    /// Asynchronously drops a schema from the target PostgreSQL database.
    ///
    /// # Arguments
    ///
    /// * `cdc_operator_snapshot_payload` - The CDC operator snapshot payload.
    ///
    pub async fn drop_schema(&self, cdc_operator_snapshot_payload: &CDCOperatorSnapshotPayload) {
        // Create a PostgresOperatorImpl instance
        let target_postgres_operator = PostgresOperatorImpl::new(self.target_db_pool.clone());

        target_postgres_operator
            .drop_schema(cdc_operator_snapshot_payload.schema_name().as_str())
            .await
            .expect("Failed to drop schema");
    }

    /// Asynchronously restores a schema to the target PostgreSQL database.
    ///
    /// # Arguments
    ///
    /// * `cdc_operator_snapshot_payload` - The CDC operator snapshot payload.
    ///
    pub async fn pg_restore_schema(
        &self,
        cdc_operator_snapshot_payload: &CDCOperatorSnapshotPayload,
    ) {
        let app_name = format!(
            "{RUSTIC_WITCHER_APP_NAME_PREFIX}_pg_restore_{db_name}_{db_schema}",
            db_name = cdc_operator_snapshot_payload.database_name(),
            db_schema = cdc_operator_snapshot_payload.schema_name()
        );
        env::set_var("PGAPPNAME", app_name);

        let restore_command = format!(
            "pg_restore --verbose --no-owner --no-privileges --dbname={db_name} {PG_DUMP_FILE_NAME}",
            db_name = cdc_operator_snapshot_payload.target_postgres_url(),
        );

        ShellCommandExecutor::execute_cmd(&restore_command).await;

        // Remove the pg_dump.sql file
        debug!("{}", "Removing {PG_DUMP_FILE_NAME} file".bold().green());
        fs::remove_file(PG_DUMP_FILE_NAME).await.unwrap();
    }

    /// Asynchronously creates a data import user for a given schema.
    ///
    /// # Arguments
    ///
    /// * `schema_name` - The name of the schema that the data import user will operate on.
    ///
    pub async fn create_data_import_user(
        &self,
        schema_name: &str,
        target_superuser_username: &str,
    ) {
        let client = self.target_db_pool.get().await.unwrap();
        let should_create_role_as_superuser = should_create_role_as_superuser();

        // If the role should be created as a superuser, add the SUPERUSER keyword to the query
        // This is a case for the initial setup of the target database, when we are running
        // in a playground environment.
        let superuser_query_addition = if should_create_role_as_superuser {
            " SUPERUSER "
        } else {
            " "
        };

        let superuser_username = superuser_username();
        let superuser_password = superuser_password();

        let create_role_query = format!(
            "CREATE ROLE {superuser_username}{superuser_query_addition}LOGIN PASSWORD '{superuser_password}'"
        );

        // Create the role
        let create_role_result = client.execute(&create_role_query, &[]).await;

        match create_role_result {
            Ok(_) => info!("{superuser_username} user created successfully"),
            Err(e) => {
                error!(
                    "{superuser_username} user already exists. Continuing... {}",
                    e
                );
            }
        }

        info!("Altering role to set session replication!");
        let alter_role_query =
            format!("ALTER ROLE {superuser_username} SET session_replication_role = 'replica'");
        let alter_role_result = client.execute(&alter_role_query, &[]).await;
        match alter_role_result {
            Ok(_) => info!("Role altered successfully"),
            Err(e) => {
                error!("Failed to alter role. Continuing... {}", e);
            }
        }

        info!("Granting permissions to {superuser_username} user");
        let mut data_import_user_preparation_commands = vec![
            format!(
                "GRANT ALL ON SCHEMA {} TO {superuser_username}",
                schema_name
            ),
            format!(
                "GRANT ALL ON ALL SEQUENCES IN SCHEMA {} TO {superuser_username}",
                schema_name
            ),
            format!(
                "GRANT ALL ON ALL TABLES IN SCHEMA {} TO {superuser_username}",
                schema_name
            ),
        ];

        // If the role should not be created as a superuser, grant the target superuser to the data import user
        if !should_create_role_as_superuser {
            data_import_user_preparation_commands.push(format!(
                "GRANT {target_superuser_username} TO {superuser_username}"
            ));
        }

        for command in data_import_user_preparation_commands {
            client
                .execute(command.as_str(), &[])
                .await
                .expect("Failed to execute: {command}");
        }
    }

    /// This was needed because not all of our live applications
    /// have properly setup their sequence ownership.
    pub async fn fix_sequences_ownership(&self, database_name: &str, schema_name: &str) {
        let fix_path = format!(
            "configuration_data/sequences_fix/{database_name}-{schema_name}/ownerships.txt"
        );

        let sequence_ownership_fixes = match std::fs::read_to_string(fix_path) {
            Err(_) => vec![],
            Ok(file_contents) => file_contents.lines().map(String::from).collect(),
        };

        let client = self.target_db_pool.get().await.unwrap();

        for sequence_fix in sequence_ownership_fixes {
            info!("Sequence ownership fix: [{sequence_fix}]");

            client.execute(&sequence_fix, &[]).await.unwrap();
        }
    }
}

fn should_create_role_as_superuser() -> bool {
    std::env::var("CREATE_ROLE_AS_SUPERUSER")
        .unwrap_or_else(|_| String::from("false"))
        .parse()
        .unwrap()
}

fn superuser_username() -> String {
    std::env::var("SUPERUSER_URL")
        .expect("SUPERUSER_URL not set")
        .split("://")
        .collect::<Vec<_>>()[1]
        .split(':')
        .collect::<Vec<_>>()[0]
        .to_string()
}

fn superuser_password() -> String {
    std::env::var("SUPERUSER_URL")
        .expect("SUPERUSER_URL not set")
        .split("://")
        .collect::<Vec<_>>()[1]
        .split(':')
        .collect::<Vec<_>>()[1]
        .split('@')
        .collect::<Vec<_>>()[0]
        .to_string()
}
