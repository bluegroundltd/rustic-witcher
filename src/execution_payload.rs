use bon::Builder;

/// Helper struct for usage with any payload that will not
/// be relevant to the DMS CDC Operator and will only be needed
/// in our domain.
#[derive(Builder)]
pub struct ExecutionPayload {
    target_application_users: Vec<String>,
}

impl ExecutionPayload {
    pub fn target_application_users(&self) -> Vec<String> {
        self.target_application_users.clone()
    }
}
