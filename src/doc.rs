use crate::error::GeoffreyError;

use std::fs;
use std::path::PathBuf;
use std::vec::Vec;

#[derive(Debug)]
pub struct Documents {
    md_files: Vec<PathBuf>,
}

impl Documents {
    pub fn new(doc_path: PathBuf) -> Result<Self, GeoffreyError> {
        if !doc_path.exists() {
            return Err(GeoffreyError::DocPathDoesNotExist(doc_path));
        }

        let mut md_files = Vec::new();

        if doc_path.is_file() {
            Self::is_md_file(doc_path).map(|md_file| md_files.push(md_file))?;
        } else {
            Self::find_md_files(&doc_path, &mut |md_file| md_files.push(md_file))?;
            if md_files.is_empty() {
                return Err(GeoffreyError::NoMarkdownFilesInPath(doc_path));
            }
        }

        Ok(Self { md_files })
    }

    fn find_md_files(
        doc_path: &PathBuf,
        md_found_cb: &mut dyn FnMut(PathBuf),
    ) -> Result<(), GeoffreyError> {
        for dir_entry in fs::read_dir(doc_path)? {
            let dir_entry = dir_entry?;
            let path = dir_entry.path();
            if path.is_dir() {
                Self::find_md_files(&path, md_found_cb)?;
            } else {
                Self::is_md_file(path)
                    .map(|md_file| md_found_cb(md_file))
                    .ok();
            }
        }

        Ok(())
    }

    fn is_md_file(path: PathBuf) -> Result<PathBuf, GeoffreyError> {
        path.extension()
            .as_ref()
            .and_then(|ext_osstr| ext_osstr.to_str())
            .and_then(|ext| {
                if ext.eq_ignore_ascii_case("md") {
                    Some(())
                } else {
                    None
                }
            })
            .ok_or(GeoffreyError::NotAMarkdownFile(path.clone()))?;

        Ok(path)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use anyhow::{anyhow, Result};
    use tempfile::Builder;

    use std::fs::{DirBuilder, File};

    #[test]
    fn document_new_with_non_existing_path_fails() -> Result<()> {
        let tmp_dir = Builder::new().prefix("geoffrey").tempdir()?;

        let mut doc_path = PathBuf::new();
        doc_path.push(tmp_dir.path());
        doc_path.push("hypnotoad");
        match Documents::new(doc_path) {
            Err(GeoffreyError::DocPathDoesNotExist(_)) => Ok(()),
            _ => Err(anyhow!("Document::new with non existing path should fail!")),
        }
    }

    #[test]
    fn document_new_with_file_as_path_but_not_md_file_fails() -> Result<()> {
        let tmp_dir = Builder::new().prefix("geoffrey").tempdir()?;

        let mut doc_path = PathBuf::new();
        doc_path.push(tmp_dir.path());
        doc_path.push("hypnotoad.txt");

        File::create(doc_path.clone())?;

        match Documents::new(doc_path) {
            Err(GeoffreyError::NotAMarkdownFile(_)) => Ok(()),
            _ => Err(anyhow!(
                "Document::new with file as path path but not md file should fail!"
            )),
        }
    }

    #[test]
    fn document_new_with_file_as_path_which_is_md_file_succeeds() -> Result<()> {
        let tmp_dir = Builder::new().prefix("geoffrey").tempdir()?;

        let mut doc_path = PathBuf::new();
        doc_path.push(tmp_dir.path());
        doc_path.push("hypnotoad.md");

        File::create(doc_path.clone())?;

        let documents = Documents::new(doc_path.clone())?;

        assert_eq!(documents.md_files.len(), 1);
        assert_eq!(documents.md_files[0], doc_path);

        Ok(())
    }

    #[test]
    fn document_new_with_empty_dir_as_path_fails() -> Result<()> {
        let tmp_dir = Builder::new().prefix("geoffrey").tempdir()?;

        let mut doc_path = PathBuf::new();
        doc_path.push(tmp_dir.path());

        match Documents::new(doc_path) {
            Err(GeoffreyError::NoMarkdownFilesInPath(_)) => Ok(()),
            _ => Err(anyhow!("Document::new with empty dir as path should fail!")),
        }
    }

    #[test]
    fn document_new_with_dir_as_path_and_single_md_file_succeeds() -> Result<()> {
        let tmp_dir = Builder::new().prefix("geoffrey").tempdir()?;

        let mut doc_path = PathBuf::new();
        doc_path.push(tmp_dir.path());

        let mut md_file = doc_path.clone();
        md_file.push("hypnotoad.md");

        File::create(md_file.clone())?;

        let documents = Documents::new(doc_path.clone())?;

        assert_eq!(documents.md_files.len(), 1);
        assert_eq!(documents.md_files[0], md_file);

        Ok(())
    }

    #[test]
    fn document_new_with_dir_as_path_and_multiple_md_file_succeeds() -> Result<()> {
        let tmp_dir = Builder::new().prefix("geoffrey").tempdir()?;

        let mut doc_path = PathBuf::new();
        doc_path.push(tmp_dir.path());

        let mut md_file_1 = doc_path.clone();
        md_file_1.push("brain_slug.md");

        let mut md_file_2 = doc_path.clone();
        md_file_2.push("hypnotoad.md");

        File::create(md_file_1.clone())?;
        File::create(md_file_2.clone())?;

        let documents = Documents::new(doc_path.clone())?;

        assert_eq!(documents.md_files.len(), 2);
        let mut md_files = documents.md_files;
        md_files.sort();
        assert_eq!(md_files[0], md_file_1);
        assert_eq!(md_files[1], md_file_2);

        Ok(())
    }

    #[test]
    fn document_new_with_dir_as_path_and_md_file_in_nested_dir_succeeds() -> Result<()> {
        let tmp_dir = Builder::new().prefix("geoffrey").tempdir()?;

        let mut doc_path = PathBuf::new();
        doc_path.push(tmp_dir.path());

        let mut md_file_1 = doc_path.clone();
        md_file_1.push("brain_slug.md");

        let mut md_file_2_dir = doc_path.clone();
        md_file_2_dir.push("hypnotoad");
        DirBuilder::new().create(md_file_2_dir.clone())?;

        let mut md_file_2 = md_file_2_dir.clone();
        md_file_2.push("hypnotoad.md");

        File::create(md_file_1.clone())?;
        File::create(md_file_2.clone())?;

        let documents = Documents::new(doc_path.clone())?;

        assert_eq!(documents.md_files.len(), 2);
        let mut md_files = documents.md_files;
        md_files.sort();
        assert_eq!(md_files[0], md_file_1);
        assert_eq!(md_files[1], md_file_2);

        Ok(())
    }
}
