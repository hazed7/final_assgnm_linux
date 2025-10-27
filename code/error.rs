use thiserror::Error;

#[derive(Error, Debug)]
pub enum MonitorError {
    #[error("Failed to read system file: {0}")]
    FileRead(String),
    
    #[error("Failed to parse system data: {0}")]
    ParseError(String),
    
    #[error("Failed to execute command: {0}")]
    CommandError(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
}