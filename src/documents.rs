use crate::error::GeoffreyError;

use rayon::prelude::*;
use regex::Regex;

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::sync::Mutex;
use std::vec::Vec;

type Tag = String;
type Content = String;

#[derive(Debug)]
pub struct Snippets {
    data: HashMap<Tag, Content>,
}

#[derive(Debug)]
struct MdSnippetId {
    path: String,
    tag: String,
}

#[derive(Debug)]
struct MdSegment {
    text: String,
    snippet_id: Option<MdSnippetId>,
}

#[derive(Debug)]
struct MdFile {
    path: PathBuf,
    segments: Vec<MdSegment>,
}

impl MdFile {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            segments: Vec::new(),
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
        // parse the md files
        let content = Mutex::new(&mut self.content);
        self.md_files
            .par_iter_mut()
            .map(|md_file| {
                Self::parse_single_md_file(md_file, &content)?;
                Ok(())
            })
            .collect::<Result<(), GeoffreyError>>()?;

        // parse the content files
        let git_toplevel = &self.git_toplevel;
        self.content
            .par_iter_mut()
            .map(|(path, snippets)| {
                let absolute_path = git_toplevel.join(path);
                if !absolute_path.exists() {
                    return Err(GeoffreyError::ContentFileNotFound(path.to_owned()));
                }
                *snippets = Self::parse_content_file(&absolute_path)?;
                Ok(())
            })
            .collect::<Result<(), GeoffreyError>>()?;

        Ok(())
    }

    pub fn sync(self) -> Result<(), GeoffreyError> {
        self.md_files
            .par_iter()
            .map(|md_file| {
                // create synced data
                let mut synced_file = String::new();
                for segment in md_file.segments.iter() {
                    synced_file.push_str(&segment.text);
                    if let Some(snippet_id) = &segment.snippet_id {
                        let snippets_cache = self.content.get(&snippet_id.path).ok_or(
                            GeoffreyError::ContentFileNotFound(snippet_id.path.to_owned()),
                        )?;

                        if let Some(snippet) = snippets_cache.data.get(&snippet_id.tag) {
                            synced_file.push_str(snippet);
                            Ok(())
                        } else {
                            Err(GeoffreyError::ContentSnippetNotFound(
                                snippet_id.path.to_owned(),
                                snippet_id.tag.to_owned(),
                            ))
                        }?;
                    }
                }

                // sync to file
                let mut file = OpenOptions::new()
                    .write(true)
                    .create(false)
                    .truncate(true)
                    .open(md_file.path.clone())?;

                file.write_all(synced_file.as_bytes())?;
                file.sync_all()?;

                Ok(())
            })
            .collect::<Result<(), GeoffreyError>>()
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
        md_file: &mut MdFile,
        content: &Mutex<&mut ContentMap>,
    ) -> Result<(), GeoffreyError> {
        let f = fs::File::open(md_file.path.clone())?;
        let mut reader = BufReader::new(f);

        let re_tag = Regex::new(r"^<!-- *\[geoffrey\] *\[([a-zA-Z0-9/._-]*)\] *(\[(.*)\])? *-->")
            .map_err(|_| GeoffreyError::RegexError)?;

        let re_code_block = Regex::new(r"```").map_err(|_| GeoffreyError::RegexError)?;

        md_file.segments.push(MdSegment {
            text: String::new(),
            snippet_id: None,
        });
        let mut segment = md_file.segments.last_mut().expect("just added");

        let mut line = String::new();
        while reader.read_line(&mut line)? > 0 {
            segment.text.push_str(&line);
            if let Some(caps) = re_tag.captures(&line) {
                let path = caps.get(1).ok_or(GeoffreyError::RegexError)?.as_str();
                let tag = caps.get(3).map_or("", |matcher| matcher.as_str());

                log::info!("{:?} '{}' - '{}'", md_file.path, path, tag);

                content.lock().expect("could not lock mutex").insert(
                    path.to_owned(),
                    Snippets {
                        data: HashMap::new(),
                    },
                );
                segment.snippet_id = Some(MdSnippetId {
                    path: path.to_owned(),
                    tag: tag.to_owned(),
                });

                // next line must be the begin of a code block
                let mut line = String::new();
                if reader.read_line(&mut line)? > 0 && re_code_block.is_match(&line) {
                    segment.text.push_str(&line);
                    Ok(())
                } else {
                    Err(GeoffreyError::CodeBlockMustFollowTag(
                        md_file.path.clone(),
                        tag.to_owned(),
                    ))
                }?;

                // skip everything until the end of the code block which is part of the next segment
                md_file.segments.push(MdSegment {
                    text: String::new(),
                    snippet_id: None,
                });
                segment = md_file.segments.last_mut().expect("just added");

                let mut line = String::new();
                let mut end_of_block_found = false;
                while reader.read_line(&mut line)? > 0 {
                    if re_code_block.is_match(&line) {
                        segment.text.push_str(&line);
                        end_of_block_found = true;
                        break;
                    }
                    line.clear();
                }

                if !end_of_block_found {
                    return Err(GeoffreyError::CodeBlockEndMissing(
                        md_file.path.clone(),
                        tag.to_owned(),
                    ));
                }
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
