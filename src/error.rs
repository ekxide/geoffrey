use thiserror::Error;

use std::path::PathBuf;

#[derive(Error, Debug)]
pub enum GeoffreyError {
    #[error("The provided doc path does either not exist or geoffrey has no read permission to '{0}'")]
    DocPathDoesNotExist(PathBuf),
    #[error("The provided doc path does either not contain md files or geoffrey has no read permission to '{0}' or its sub-directories")]
    NoMarkdownFilesInPath(PathBuf),
    #[error("The provided doc path is not a markdown file: {0}")]
    NotAMarkdownFile(PathBuf),
    #[error("Error accessing file")]
    IoError(#[from] std::io::Error),
}
