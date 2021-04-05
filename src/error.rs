use thiserror::Error;

use std::path::PathBuf;

#[derive(Error, Debug)]
pub enum GeoffreyError {
    #[error(
        "The provided doc path does either not exist or geoffrey has no read permission to '{0}'"
    )]
    DocPathDoesNotExist(PathBuf),
    #[error("The provided doc path does either not contain md files or geoffrey has no read permission to '{0}' or its sub-directories")]
    NoMarkdownFilesInPath(PathBuf),
    #[error("The provided doc path '{0}' is not a markdown file")]
    NotAMarkdownFile(PathBuf),
    #[error("Could not get git toplevel")]
    GitToplevelError,
    #[error("Regex error")]
    RegexError,
    #[error("The content file '{1}' referenced in the markdown file '{0}' was not found")]
    ContentFileNotFound(PathBuf, String),
    #[error("The content snippet '{1}' not found in the content file '{0}' was not found")]
    ContentSnippetNotFound(String, String),
    #[error(
        "The code block must immediately follow the geoffrey tag '{1}' in the markdown file '{0}'"
    )]
    CodeBlockFustFollowTag(PathBuf, String),
    #[error("Error accessing file")]
    IoError(#[from] std::io::Error),
}
