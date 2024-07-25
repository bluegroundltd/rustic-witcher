/// Helper struct for usage with any payload that will not
/// be relevant to the DMS CDC Operator and will only be needed
/// in our domain.
pub struct ExecutionPayload {
    target_application_users: Vec<String>,
}

impl ExecutionPayload {
    pub fn new(target_application_users: Vec<String>) -> Self {
        Self {
            target_application_users,
        }
    }

    pub fn target_application_users(&self) -> Vec<String> {
        self.target_application_users.clone()
    }
}
