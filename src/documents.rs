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

#[derive(Debug, Clone)]
struct ContentSnippetDescription {
    tag: String,
    indentation: String,
    ellipsis_line: String,
    begin: usize,
    end: usize,
    nested: Vec<ContentSnippetDescription>,
}

#[derive(Debug)]
struct ContentFile {
    data: Vec<String>,
    lookup: HashMap<Tag, ContentSnippetDescription>,
}

impl ContentFile {
    fn new() -> Self {
        ContentFile {
            data: Vec::new(),
            lookup: HashMap::new(),
        }
    }
}

#[derive(Debug)]
enum MdSnippetTag {
    FullFile,
    FullSnippet { main: String },
    ElidedSnippet { main: String, sub: Vec<String> },
}

#[derive(Debug)]
struct MdSnippetId {
    path: String,
    tag: MdSnippetTag,
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

type ContentMap = HashMap<String, ContentFile>;

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
            content: ContentMap::new(),
        })
    }

    pub fn parse(&mut self) -> Result<(), GeoffreyError> {
        log::info!("#### parse md files for tags");
        let content = Mutex::new(&mut self.content);
        self.md_files
            .par_iter_mut()
            .map(|md_file| {
                Self::parse_single_md_file(md_file, &content)?;
                Ok(())
            })
            .collect::<Result<(), GeoffreyError>>()?;

        log::info!("#### parse content files for tags");
        let git_toplevel = &self.git_toplevel;
        self.content
            .par_iter_mut()
            .map(|(path, content_file)| {
                let absolute_path = git_toplevel.join(path);
                if !absolute_path.exists() {
                    return Err(GeoffreyError::ContentFileNotFound(path.to_owned()));
                }
                *content_file = Self::parse_content_file(&absolute_path)?;

                Ok(())
            })
            .collect::<Result<(), GeoffreyError>>()?;

        Ok(())
    }

    fn has_elided_lines(
        tags: &Vec<&str>,
        elided_lines: &mut Vec<usize>,
        ellipsis_lines: &mut Vec<(usize, usize, String)>,
        snip_desc: &ContentSnippetDescription,
    ) -> bool {
        let current_snippet_tag = &snip_desc.tag as &str;
        let mut keep_this = tags.contains(&current_snippet_tag);
        let keep_nested = snip_desc
            .nested
            .iter()
            .map(|snip_desc| {
                let keep = Self::has_elided_lines(tags, elided_lines, ellipsis_lines, snip_desc);
                keep_this |= keep;
                keep
            })
            .collect::<Vec<bool>>();

        if keep_this {
            keep_nested
                .iter()
                .zip(snip_desc.nested.iter())
                .for_each(|(keep, snip_desc)| {
                    if !keep {
                        ellipsis_lines.push((
                            snip_desc.begin,
                            snip_desc.end,
                            snip_desc.ellipsis_line.clone(),
                        ));
                        elided_lines.extend_from_slice(
                            &(snip_desc.begin..=snip_desc.end)
                                .into_iter()
                                .map(|x| x)
                                .collect::<Vec<usize>>(),
                        )
                    }
                });
        }

        keep_this
    }

    pub fn sync(self) -> Result<(), GeoffreyError> {
        log::info!("#### sync md files with content");
        self.md_files
            .par_iter()
            .map(|md_file| {
                // create synced data
                let mut synced_file = String::new();
                for segment in md_file.segments.iter() {
                    synced_file.push_str(&segment.text);
                    if let Some(snippet_id) = &segment.snippet_id {
                        let content_cache = self.content.get(&snippet_id.path).ok_or(
                            GeoffreyError::ContentFileNotFound(snippet_id.path.to_owned()),
                        )?;

                        let tag = match &snippet_id.tag {
                            MdSnippetTag::FullFile => "",
                            MdSnippetTag::FullSnippet { main } => &main,
                            MdSnippetTag::ElidedSnippet { main, .. } => &main,
                        };

                        let mut ellipsis_lines = Vec::<(usize, usize, String)>::new();

                        if let Some(snip_desc) = content_cache.lookup.get(tag) {
                            let mut elided_lines = Vec::new();
                            if let MdSnippetTag::ElidedSnippet { main, sub } = &snippet_id.tag {
                                let mut all_tags = Vec::<&str>::new();
                                all_tags.push(main);
                                sub.into_iter().for_each(|tag| all_tags.push(tag));

                                Self::has_elided_lines(
                                    &all_tags,
                                    &mut elided_lines,
                                    &mut ellipsis_lines,
                                    &snip_desc,
                                );
                                elided_lines.sort();

                                let mut empty_lines = Vec::new();
                                let mut potentially_remove = Vec::new();
                                let mut extend_empty_on_next_non_empty = false;

                                let mut current_line = snip_desc.end.min(snip_desc.begin + 1);
                                for elided in &elided_lines {
                                    while *elided > current_line {
                                        let trimmed = content_cache.data[current_line].trim();
                                        if trimmed.is_empty() {
                                            potentially_remove.push(current_line);
                                        } else {
                                            if extend_empty_on_next_non_empty {
                                                empty_lines.extend_from_slice(&potentially_remove);
                                            }
                                            extend_empty_on_next_non_empty = false;
                                            potentially_remove.clear();
                                        }
                                        current_line += 1;
                                    }
                                    empty_lines.extend_from_slice(&potentially_remove);
                                    potentially_remove.clear();
                                    extend_empty_on_next_non_empty = true;
                                    current_line += 1;
                                }
                                while snip_desc.end > current_line {
                                    let trimmed = content_cache.data[current_line].trim();
                                    if trimmed.is_empty() {
                                        potentially_remove.push(current_line);
                                    } else {
                                        empty_lines.extend_from_slice(&potentially_remove);
                                        potentially_remove.clear();
                                        break;
                                    }
                                    current_line += 1;
                                }
                                empty_lines.extend_from_slice(&potentially_remove);
                                potentially_remove.clear();

                                elided_lines.extend_from_slice(&empty_lines);
                                elided_lines.sort();
                            }

                            let snippet = match &snippet_id.tag {
                                MdSnippetTag::FullFile => content_cache.data[..]
                                    .into_iter()
                                    .map(|line| line as &str)
                                    .collect::<Vec<&str>>(),
                                MdSnippetTag::FullSnippet { .. } => content_cache.data
                                    [snip_desc.end.min(snip_desc.begin + 1)..snip_desc.end]
                                    .into_iter()
                                    .map(|line| line as &str)
                                    .collect::<Vec<&str>>(),
                                MdSnippetTag::ElidedSnippet { .. } => {
                                    let mut current_line = snip_desc.end.min(snip_desc.begin + 1);

                                    let mut remaining_lines = Vec::<&str>::new();
                                    let mut add_ellipsis_line = true;

                                    for elided in &elided_lines {
                                        while *elided > current_line {
                                            remaining_lines.push(&content_cache.data[current_line]);
                                            current_line += 1;
                                            add_ellipsis_line = true;
                                        }

                                        if add_ellipsis_line {
                                            for ellipsis in &ellipsis_lines {
                                                if current_line >= ellipsis.0
                                                    || current_line <= ellipsis.1
                                                {
                                                    remaining_lines.push(&ellipsis.2);
                                                    break;
                                                }
                                            }

                                            add_ellipsis_line = false;
                                        }
                                        current_line += 1;
                                    }
                                    while snip_desc.end > current_line {
                                        remaining_lines.push(&content_cache.data[current_line]);
                                        current_line += 1;
                                    }
                                    remaining_lines
                                }
                            };

                            let re = Regex::new(r"( *)//! \[(.*)\]")
                                .map_err(|_| GeoffreyError::RegexError)?;
                            for line in snippet {
                                // skip tag lines
                                if !re.is_match(line) {
                                    synced_file.push_str(
                                        line.strip_prefix(&snip_desc.indentation).unwrap_or(&line),
                                    );
                                }
                            }
                            Ok(())
                        } else {
                            Err(GeoffreyError::ContentSnippetNotFound(
                                snippet_id.path.to_owned(),
                                tag.to_owned(),
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

        let re_tag = Regex::new(r"^<!-- *\[geoffrey\] *\[([\w\s\.-/]*)\] *(\[(.*)\])? *-->")
            .map_err(|_| GeoffreyError::RegexError)?;

        let re_sub_tag = Regex::new(r"\[([\w\s\.-]*)\]").map_err(|_| GeoffreyError::RegexError)?;

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
                let str_tag = caps.get(3).map_or("", |matcher| matcher.as_str().trim());

                log::info!("{:?} '{}' - '{}'", md_file.path, path, str_tag);

                let tag = match str_tag {
                    "" => MdSnippetTag::FullFile,
                    _ => {
                        let mut caps_iter = re_sub_tag.captures_iter(str_tag);

                        if let Some(caps) = caps_iter.next() {
                            let main = caps
                                .get(1)
                                .ok_or(GeoffreyError::RegexError)?
                                .as_str()
                                .to_owned();
                            let sub = caps_iter
                                .map(|caps| {
                                    Ok(caps
                                        .get(1)
                                        .ok_or(GeoffreyError::RegexError)?
                                        .as_str()
                                        .to_owned())
                                })
                                .collect::<Result<Vec<String>, GeoffreyError>>()?;
                            MdSnippetTag::ElidedSnippet { main, sub }
                        } else {
                            MdSnippetTag::FullSnippet {
                                main: str_tag.to_owned(),
                            }
                        }
                    }
                };

                content
                    .lock()
                    .expect("could not lock mutex")
                    .insert(path.to_owned(), ContentFile::new());
                segment.snippet_id = Some(MdSnippetId {
                    path: path.to_owned(),
                    tag,
                });

                // next line must be the begin of a code block
                let mut line = String::new();
                if reader.read_line(&mut line)? > 0 && re_code_block.is_match(&line) {
                    segment.text.push_str(&line);
                    Ok(())
                } else {
                    Err(GeoffreyError::CodeBlockMustFollowTag(
                        md_file.path.clone(),
                        str_tag.to_owned(),
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
                        str_tag.to_owned(),
                    ));
                }
            }
            line.clear();
        }
        Ok(())
    }

    fn parse_content_file(path: &PathBuf) -> Result<ContentFile, GeoffreyError> {
        let file = fs::File::open(path)?;
        let mut reader = BufReader::new(file);

        let mut content_file = ContentFile::new();

        let content_snippet = ContentSnippetDescription {
            tag: String::new(),
            indentation: String::new(),
            ellipsis_line: String::new(),
            begin: 0,
            end: 0,
            nested: Vec::new(),
        };

        let root_content_snippet = Self::parse_next_content_snippet(
            &path,
            &mut reader,
            &mut content_file,
            content_snippet,
        )?;

        if content_file
            .lookup
            .insert(root_content_snippet.tag.clone(), root_content_snippet)
            .is_some()
        {
            return Err(GeoffreyError::ContentSnippetDoubleTag(
                path.clone(),
                "".to_owned(),
            ))?;
        }

        Ok(content_file)
    }

    fn parse_next_content_snippet<R>(
        path: &PathBuf,
        reader: &mut BufReader<R>,
        content_file: &mut ContentFile,
        mut current_snippet: ContentSnippetDescription,
    ) -> Result<ContentSnippetDescription, GeoffreyError>
    where
        R: std::io::Read,
    {
        let re = Regex::new(r"( *)//! \[(.*)\]").map_err(|_| GeoffreyError::RegexError)?;

        let mut line = String::new();
        loop {
            if reader.read_line(&mut line)? > 0 {
                if let Some(caps) = re.captures(&line) {
                    let new_tag = caps.get(2).ok_or(GeoffreyError::RegexError)?.as_str();

                    if current_snippet.tag == new_tag {
                        current_snippet.end = content_file.data.len();
                        content_file.data.push(line);
                        break Ok(current_snippet);
                    } else if new_tag.len() == 0 {
                        break Err(GeoffreyError::ContentSnippetEmptyTag(path.clone()));
                    } else {
                        let indentation = caps
                            .get(1)
                            .ok_or(GeoffreyError::RegexError)?
                            .as_str()
                            .to_owned();

                        let ellipsis_line = format!("{}// ...\n", indentation);

                        let new_snippet = ContentSnippetDescription {
                            tag: new_tag.to_owned(),
                            indentation,
                            ellipsis_line,
                            begin: content_file.data.len(),
                            end: 0,
                            nested: Vec::new(),
                        };

                        content_file.data.push(line);
                        line = String::new();

                        let nested_snippet = Self::parse_next_content_snippet(
                            &path,
                            reader,
                            content_file,
                            new_snippet,
                        )?;

                        if content_file
                            .lookup
                            .insert(nested_snippet.tag.clone(), nested_snippet.clone())
                            .is_some()
                        {
                            return Err(GeoffreyError::ContentSnippetDoubleTag(
                                path.clone(),
                                nested_snippet.tag.clone(),
                            ))?;
                        }

                        current_snippet.nested.push(nested_snippet);
                    }
                } else {
                    content_file.data.push(line);
                    line = String::new();
                }
            } else {
                if current_snippet.tag == line {
                    current_snippet.end = content_file.data.len().max(1) - 1;
                    break Ok(current_snippet);
                } else {
                    break Err(GeoffreyError::ContentSnippetEndTagNotFound(
                        path.clone(),
                        current_snippet.tag,
                    ));
                }
            }
        }
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
