use crate::error::GeoffreyError;

use regex::Regex;

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::vec::Vec;

#[derive(Debug)]
pub struct Snippets {
    data: HashMap<String, String>,
}

#[derive(Debug)]
struct MdFile {
    path: PathBuf,
    synced_file: String,
}

impl MdFile {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            synced_file: String::new(),
        }
    }
}

type ContentMap = HashMap<String, Snippets>;

#[derive(Debug)]
pub struct Documents {
    git_toplevel: PathBuf,
    md_files: Vec<MdFile>,
    content: ContentMap,
}

impl Documents {
    pub fn new(doc_path: PathBuf) -> Result<Self, GeoffreyError> {
        if !doc_path.exists() {
            return Err(GeoffreyError::DocPathDoesNotExist(doc_path));
        }

        let doc_dir = if doc_path.is_dir() {
            doc_path.clone()
        } else {
            doc_path
                .parent()
                .ok_or(GeoffreyError::GitToplevelError)?
                .to_path_buf()
        };
        let git_toplevel = std::process::Command::new("git")
            .arg("rev-parse")
            .arg("--show-toplevel")
            .current_dir(doc_dir)
            .output()
            .map_err(|_| GeoffreyError::GitToplevelError)?;

        let git_toplevel = PathBuf::from(
            std::str::from_utf8(&git_toplevel.stdout)
                .map_err(|_| GeoffreyError::GitToplevelError)?
                .trim(),
        );

        let mut md_files = Vec::new();

        if doc_path.is_file() {
            Self::is_md_file(doc_path).map(|file| md_files.push(MdFile::new(file)))?;
        } else {
            Self::find_md_files(&doc_path, &mut |file| md_files.push(MdFile::new(file)))?;
            if md_files.is_empty() {
                return Err(GeoffreyError::NoMarkdownFilesInPath(doc_path));
            }
        }

        Ok(Self {
            git_toplevel,
            md_files,
            content: HashMap::new(),
        })
    }

    pub fn parse(&mut self) -> Result<(), GeoffreyError> {
        for md_file in self.md_files.iter_mut() {
            Self::parse_single_md_file(&self.git_toplevel, md_file, &mut self.content)?;
        }
        Ok(())
    }

    pub fn sync(&self) -> Result<(), GeoffreyError> {
        for md_file in self.md_files.iter().as_ref() {
            let mut file = OpenOptions::new()
                .write(true)
                .create(false)
                .truncate(true)
                .open(md_file.path.clone())?;

            file.write_all(md_file.synced_file.as_bytes())?;
            file.sync_all()?
        }

        Ok(())
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

    fn parse_single_md_file(
        git_toplevel: &PathBuf,
        md_file: &mut MdFile,
        content: &mut ContentMap,
    ) -> Result<(), GeoffreyError> {
        let f = fs::File::open(md_file.path.clone())?;
        let mut reader = BufReader::new(f);

        let re_tag = Regex::new(r"^<!-- *\[geoffrey\] *\[([a-zA-Z0-9/._-]*)\] *(\[(.*)\])? *-->")
            .map_err(|_| GeoffreyError::RegexError)?;

        let re_code_block = Regex::new(r"```").map_err(|_| GeoffreyError::RegexError)?;

        let mut line = String::new();
        while reader.read_line(&mut line)? > 0 {
            if let Some(caps) = re_tag.captures(&line) {
                let path = caps.get(1).ok_or(GeoffreyError::RegexError)?.as_str();
                let tag = caps.get(3).map_or("", |matcher| matcher.as_str());

                log::info!("{:?} '{}' - '{}'", md_file.path, path, tag);

                let snippets_cache = if let Some(snippets_cache) = content.get(path) {
                    snippets_cache
                } else {
                    let absolute_path = git_toplevel.join(path);
                    if !absolute_path.exists() {
                        return Err(GeoffreyError::ContentFileNotFound(
                            md_file.path.clone(),
                            path.to_owned(),
                        ));
                    }
                    let snippets = Self::parse_content_file(&absolute_path)?;
                    content.insert(path.to_owned(), snippets);
                    content.get(path).expect("The value was just added")
                };

                if let Some(snippet) = snippets_cache.data.get(tag) {
                    md_file.synced_file.push_str(&line);

                    let mut code_block_line = String::new();
                    let mut within_code_block = false;
                    while reader.read_line(&mut code_block_line)? > 0 {
                        if re_code_block.is_match(&code_block_line) {
                            md_file.synced_file.push_str(&code_block_line);
                            if !within_code_block {
                                md_file.synced_file.push_str(&snippet);
                            } else {
                                break;
                            }
                            within_code_block = !within_code_block;
                        } else if !within_code_block {
                            return Err(GeoffreyError::CodeBlockFustFollowTag(
                                md_file.path.clone(),
                                tag.to_owned(),
                            ));
                        }

                        code_block_line.clear();
                    }
                } else {
                    return Err(GeoffreyError::ContentSnippetNotFound(
                        path.to_owned(),
                        tag.to_owned(),
                    ));
                }
            } else {
                md_file.synced_file.push_str(&line);
            }
            line.clear();
        }
        Ok(())
    }

    fn parse_content_file(path: &PathBuf) -> Result<Snippets, GeoffreyError> {
        let file = fs::File::open(path)?;
        let mut reader = BufReader::new(file);

        let re = Regex::new(r" *//! \[(.*)\]").map_err(|_| GeoffreyError::RegexError)?;

        let mut snippets = Snippets {
            data: HashMap::new(),
        };

        let mut current_tag = None;
        let mut line = String::new();
        while reader.read_line(&mut line)? > 0 {
            if let Some(caps) = re.captures(&line) {
                if current_tag.is_none() {
                    current_tag = Some(
                        caps.get(1)
                            .ok_or(GeoffreyError::RegexError)?
                            .as_str()
                            .to_owned(),
                    );
                    line.clear();
                } else {
                    current_tag = None;
                }
            }

            if current_tag.is_none() {
                line.clear()
            };

            current_tag
                .as_ref()
                .map(|tag| snippets.data.insert(tag.clone(), line.clone()));
        }

        Ok(snippets)
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
        assert_eq!(documents.md_files[0].path, doc_path);

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
        assert_eq!(documents.md_files[0].path, md_file);

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

        let mut documents = Documents::new(doc_path.clone())?;

        assert_eq!(documents.md_files.len(), 2);
        let mut files = documents
            .md_files
            .drain(..)
            .map(|md_file| md_file.path)
            .collect::<Vec<PathBuf>>();
        files.sort();
        assert_eq!(files[0], md_file_1);
        assert_eq!(files[1], md_file_2);

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

        let mut documents = Documents::new(doc_path.clone())?;

        assert_eq!(documents.md_files.len(), 2);
        let mut files = documents
            .md_files
            .drain(..)
            .map(|md_file| md_file.path)
            .collect::<Vec<PathBuf>>();
        files.sort();
        assert_eq!(files[0], md_file_1);
        assert_eq!(files[1], md_file_2);

        Ok(())
    }
}
