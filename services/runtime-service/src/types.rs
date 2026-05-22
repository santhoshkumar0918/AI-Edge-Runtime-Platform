use serde::{Deserialize, Serialize};

#[derive(Deserialize, Clone)]
pub struct ExecutionRequest {
    pub language: String,
    pub code: String,
    pub timeout_ms: Option<u64>,
}

#[derive(Serialize, Clone)]
pub struct ExecutionResult {
    pub id: String,
    pub status: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
}
