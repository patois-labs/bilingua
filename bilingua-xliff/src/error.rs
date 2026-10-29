//! Errors from reading and patching bilingual files and packages.

use thiserror::Error as ThisError;

/// Errors from opening, parsing and saving.
#[derive(Debug, ThisError)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("XML error at byte {pos}: {msg}")]
    Xml { pos: usize, msg: String },
    #[error("unsupported file: {0}")]
    Unsupported(String),
    /// A package (zip) was refused or is malformed: traversal, bomb, junk, encryption, missing parts.
    #[error("package refused: {0}")]
    Package(String),
}
