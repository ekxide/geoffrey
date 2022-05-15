// SPDX-License-Identifier: Apache-2.0

use nom::bytes::complete::{is_not, tag, take_while};
use nom::character::complete::{alphanumeric0, space0};
use nom::combinator;
use nom::sequence;

use thiserror::Error;

use std::iter::Enumerate;
use std::str::Lines;

#[derive(Debug, PartialEq, Eq)]
pub struct Document<'a> {
    pub content: String,
    pub has_geoffrey_code_blocks: bool,
    pub sections: Vec<Section<'a>>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Section<'a> {
    TextLine(&'a str),
    GeoffreyCodeBlock {
        indentation: usize,
        tag: GeoffreyTag<'a>,
        begin: &'a str,
        end: &'a str,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub struct GeoffreyTag<'a> {
    file_name: &'a str,
    snippet: Snippet<'a>,
    options: Vec<GeoffreyOption>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Snippet<'a> {
    FullFile,
    FullBlock {
        id: &'a str,
    },
    ElidedBlock {
        main_id: &'a str,
        sub_ids: Vec<&'a str>,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub struct GeoffreyOption {}

/// Helper struct to pass line and text of the geoffrey tag to code block parsing
#[derive(Debug, Default, Clone, Copy)]
struct ParseContext<'a> {
    line: usize,
    text: &'a str,
}

// TODO add some context data to the errors
#[derive(Error, Debug, PartialEq, Eq)]
pub enum GeoffreyTagAttributeParseError {
    #[error("NotAnAttribute")]
    NotAnAttribute,
    #[error("UnmatchedBracket")]
    UnmatchedBracket,
    #[error("Empty")]
    Empty,
    #[error("InvalidCharacter")]
    InvalidCharacter,
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum GeoffreyTagParseError {
    #[error("Inappropriate remainder '{remainder}'")]
    InappropriateTagRemainder { remainder: String },
    #[error("InvalidFileName")]
    InvalidFileName {
        tag_data: String,
        error: GeoffreyTagAttributeParseError,
    },
    #[error("InappropriateSnippetRemainder")]
    InappropriateSnippetRemainder,
    #[error("NotEllidedBlockSnippet")]
    NotEllidedBlockSnippet,
    #[error("NotFullBlockSnippet")]
    NotFullBlockSnippet,
    #[error("InvalidFullBlockSnippet")]
    InvalidFullBlockSnippet,
    #[error("InvalidFullFileSnippet")]
    InvalidFullFileSnippet,
    #[error("InvalidSnippetMainId")]
    InvalidSnippetMainId {
        error: GeoffreyTagAttributeParseError,
    },
    #[error("InvalidSnippetSubIds")]
    InvalidSnippetSubIds {
        error: GeoffreyTagAttributeParseError,
    },
    #[error("UnmatchedNestedBrackets")]
    UnmatchedNestedBrackets,
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum GeoffreyCodeBlockParseError {
    #[error("Unexpected")]
    Unexpected,
    #[error(
        "NotCodeBlockBegin! Check for blank lines between the geoffrey tag and the code block!"
    )]
    NotCodeBlockBegin,
    #[error("Inappropriate remainder '{remainder}'")]
    InappropriateCodeBlockRemainder { remainder: String },
    #[error("InvalidLanguageSpecifier")]
    InvalidLanguageSpecifier { remainder: String },
    #[error("InvalidBackticksCount")]
    InvalidBackticksCount,
    #[error("InvalidBackticksEnd")]
    InvalidBackticksEnd,
    #[error("UnmatchedBackticks")]
    UnmatchedBackticks,
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum MdParseError {
    #[error("Not a geoffrey tag")]
    NotAGeoffreyTag,
    #[error("Line {line} with text '{text}' has error: {error}!")]
    GeoffreyTagError {
        line: usize,
        text: String,
        error: GeoffreyTagParseError,
    },
    #[error("Line {line} with tag '{text}'! No code block found! Please remove the tag or blank lines between the tag and code block!")]
    GeoffreyTagWithoutCodeBlock { line: usize, text: String },
    #[error("Line {line} with tag '{text}'! Error: {error}")]
    GeoffreyCodeBlockError {
        line: usize,
        text: String,
        error: GeoffreyCodeBlockParseError,
    },
}

type NomError<E> = nom::Err<nom::error::Error<E>>;

type TagAttributeParseResult<T> = std::result::Result<T, GeoffreyTagAttributeParseError>;
type TagParseResult<T> = std::result::Result<T, GeoffreyTagParseError>;
type CodeBlockParseResult<T> = std::result::Result<T, GeoffreyCodeBlockParseError>;
type ParseResult<T> = std::result::Result<T, MdParseError>;

pub fn parse(file_content: &str) -> ParseResult<Vec<Section>> {
    let mut sections = Vec::new();
    let mut lines = file_content.lines().enumerate();
    while let Some((n, i)) = lines.next() {
        let section = parse_section(n, i, &mut lines)?;
        sections.push(section);
    }

    Ok(sections)
}

fn parse_section<'a>(
    n: usize,
    i: &'a str,
    lines: &mut Enumerate<Lines<'a>>,
) -> ParseResult<Section<'a>> {
    let section = match geoffrey_tag_envelope(n, i) {
        Ok((attributes, indentation)) => geoffrey_code_block(
            indentation,
            attributes,
            lines,
            ParseContext {
                line: n + 1,
                text: i,
            },
        )?,
        Err(e) if e == MdParseError::NotAGeoffreyTag => Section::TextLine(i),
        Err(e) => return Err(e),
    };

    Ok(section)
}

fn md_code_block_begin(i: &str) -> CodeBlockParseResult<(&str, &str)> {
    let i = i.trim();
    let (i, code_block_fence) = is_not(" \t")(i)
        .map_err(|_: NomError<_>| GeoffreyCodeBlockParseError::NotCodeBlockBegin)?;
    let i = i.trim();
    if !i.is_empty() {
        return Err(
            GeoffreyCodeBlockParseError::InappropriateCodeBlockRemainder {
                remainder: i.into(),
            },
        );
    }

    // sanity checks
    let (lang, backticks) = take_while(|c| c == '`')(code_block_fence)
        .map_err(|_: NomError<_>| GeoffreyCodeBlockParseError::Unexpected)?;
    if backticks.len() < 3 {
        return Err(GeoffreyCodeBlockParseError::InvalidBackticksCount);
    }
    let (rem, _) =
        alphanumeric0(lang).map_err(|_: NomError<_>| GeoffreyCodeBlockParseError::Unexpected)?;

    let rem = rem.trim();
    if !rem.is_empty() {
        return Err(GeoffreyCodeBlockParseError::InvalidLanguageSpecifier {
            remainder: rem.into(),
        });
    }

    Ok((code_block_fence, backticks))
}

fn md_code_block_end<'a>(
    lines: &mut Enumerate<Lines<'a>>,
    backticks: &'a str,
    code_block_begin_context: ParseContext<'a>,
) -> ParseResult<&'a str> {
    for (n, i) in lines.by_ref() {
        let i = i.trim();
        let (i, code_block_fence) =
            combinator::opt(tag(backticks))(i).map_err(|_: NomError<_>| {
                MdParseError::GeoffreyCodeBlockError {
                    line: n + 1,
                    text: i.into(),
                    error: GeoffreyCodeBlockParseError::Unexpected,
                }
            })?;
        if let Some(code_block_fence) = code_block_fence {
            let i = i.trim();
            if !i.is_empty() {
                return Err(MdParseError::GeoffreyCodeBlockError {
                    line: n + 1,
                    text: i.into(),
                    error: GeoffreyCodeBlockParseError::InvalidBackticksEnd,
                });
            }
            return Ok(code_block_fence);
        }
    }

    Err(MdParseError::GeoffreyCodeBlockError {
        line: code_block_begin_context.line,
        text: code_block_begin_context.text.into(),
        error: GeoffreyCodeBlockParseError::UnmatchedBackticks,
    })
}

fn md_code_block<'a>(
    lines: &mut Enumerate<Lines<'a>>,
    tag_context: ParseContext<'a>,
) -> ParseResult<(&'a str, &'a str)> {
    let (n, i) = lines
        .next()
        .ok_or(MdParseError::GeoffreyTagWithoutCodeBlock {
            line: tag_context.line,
            text: tag_context.text.into(),
        })?;
    let (begin, backticks) = md_code_block_begin(i).map_err(|e| match e {
        GeoffreyCodeBlockParseError::NotCodeBlockBegin => {
            MdParseError::GeoffreyTagWithoutCodeBlock {
                line: tag_context.line,
                text: tag_context.text.into(),
            }
        }
        _ => MdParseError::GeoffreyCodeBlockError {
            line: n + 1,
            text: i.into(),
            error: e,
        },
    })?;
    let end = md_code_block_end(
        lines,
        backticks,
        ParseContext {
            line: n + 1,
            text: i,
        },
    )?;

    Ok((begin, end))
}

fn geoffrey_tag_envelope(n: usize, line: &str) -> ParseResult<(&str, usize)> {
    let (i, spaces) = space0(line).map_err(|_: NomError<_>| MdParseError::NotAGeoffreyTag)?;
    let (i, md_comment) = sequence::delimited(tag("<!--"), is_not("-->"), tag("-->"))(i)
        .map_err(|_: NomError<_>| MdParseError::NotAGeoffreyTag)?;
    let (rem, _) = space0(i).map_err(|_: NomError<_>| MdParseError::NotAGeoffreyTag)?;

    if !rem.is_empty() {
        return Err(MdParseError::GeoffreyTagError {
            line: n + 1,
            text: i.into(),
            error: GeoffreyTagParseError::InappropriateTagRemainder {
                remainder: rem.into(),
            },
        })?;
    }

    let (md_comment, _) =
        space0(md_comment).map_err(|_: NomError<_>| MdParseError::NotAGeoffreyTag)?;
    let (attributes, _) =
        tag("[geoffrey]")(md_comment).map_err(|_: NomError<_>| MdParseError::NotAGeoffreyTag)?;

    Ok((attributes, spaces.len()))
}

fn geoffrey_tag_data(i: &str) -> TagParseResult<(&str, Snippet)> {
    let (i, file_name) =
        geoffrey_attribute(i).map_err(|e| GeoffreyTagParseError::InvalidFileName {
            tag_data: i.into(),
            error: e,
        })?;
    let (rem, snippet) = geoffrey_attributes_snippet_list(i)?;
    if !rem.is_empty() {
        return Err(GeoffreyTagParseError::InappropriateSnippetRemainder);
    }

    Ok((file_name, snippet))
}

fn geoffrey_code_block<'a>(
    indentation: usize,
    attributes: &'a str,
    lines: &mut Enumerate<Lines<'a>>,
    tag_context: ParseContext<'a>,
) -> ParseResult<Section<'a>> {
    let (file_name, snippet) =
        geoffrey_tag_data(attributes).map_err(|e| MdParseError::GeoffreyTagError {
            line: tag_context.line,
            text: tag_context.text.into(),
            error: e,
        })?;
    let (begin, end) = md_code_block(lines, tag_context)?;

    Ok(Section::GeoffreyCodeBlock {
        indentation,
        tag: GeoffreyTag {
            file_name,
            snippet,
            options: Vec::new(),
        },
        begin,
        end,
    })
}

fn geoffrey_attribute(i: &str) -> TagAttributeParseResult<(&str, &str)> {
    let i = i.trim();
    let (i, _) =
        tag("[")(i).map_err(|_: NomError<_>| GeoffreyTagAttributeParseError::NotAnAttribute)?;
    let (i, attribute) = is_not("]")(i)
        .map_err(|_: NomError<_>| GeoffreyTagAttributeParseError::UnmatchedBracket)?;
    let (i, _) =
        tag("]")(i).map_err(|_: NomError<_>| GeoffreyTagAttributeParseError::UnmatchedBracket)?;
    let i = i.trim();

    let (rem, _) =
        is_not("[")(attribute).map_err(|_: NomError<_>| GeoffreyTagAttributeParseError::Empty)?;
    let (_, _) = combinator::verify(combinator::rest, |s: &str| s.is_empty())(rem)
        .map_err(|_: NomError<_>| GeoffreyTagAttributeParseError::InvalidCharacter)?;

    Ok((i, attribute.trim()))
}

fn geoffrey_attributes_snippet_list(i: &str) -> TagParseResult<(&str, Snippet)> {
    let i = i.trim();

    match geoffrey_attributes_snippet_elided_block(i) {
        Ok(ellided_block) => return Ok(ellided_block),
        Err(e) if e != GeoffreyTagParseError::NotEllidedBlockSnippet => return Err(e),
        _ => (),
    }

    match geoffrey_attributes_snippet_full_block(i) {
        Ok(full_block) => return Ok(full_block),
        Err(e) if e != GeoffreyTagParseError::NotFullBlockSnippet => return Err(e),
        _ => (),
    }

    geoffrey_attributes_snippet_full_file(i)
}

fn geoffrey_attributes_snippet_elided_block(i: &str) -> TagParseResult<(&str, Snippet)> {
    let i = i.trim();
    let (i, _) =
        tag("[")(i).map_err(|_: NomError<_>| GeoffreyTagParseError::NotEllidedBlockSnippet)?;
    let i = i.trim();
    let (_, _) =
        tag("[")(i).map_err(|_: NomError<_>| GeoffreyTagParseError::NotEllidedBlockSnippet)?;
    let (i, main_id) = geoffrey_attribute(i)
        .map_err(|e| GeoffreyTagParseError::InvalidSnippetMainId { error: e })?;

    let mut sub_ids = Vec::<&str>::new();

    let mut i = i;

    loop {
        match geoffrey_attribute(i) {
            Ok((ii, sub_id)) => {
                sub_ids.push(sub_id);
                i = ii;
            }
            Err(e) if e == GeoffreyTagAttributeParseError::NotAnAttribute => break,
            Err(e) => return Err(GeoffreyTagParseError::InvalidSnippetSubIds { error: e }),
        }
    }

    let i = i.trim();
    let (i, _) =
        tag("]")(i).map_err(|_: NomError<_>| GeoffreyTagParseError::UnmatchedNestedBrackets)?;
    let i = i.trim();

    Ok((i, Snippet::ElidedBlock { main_id, sub_ids }))
}

fn geoffrey_attributes_snippet_full_block(i: &str) -> TagParseResult<(&str, Snippet)> {
    match geoffrey_attribute(i) {
        Ok((i, id)) => Ok((i, Snippet::FullBlock { id })),
        Err(e) if e == GeoffreyTagAttributeParseError::NotAnAttribute => {
            Err(GeoffreyTagParseError::NotFullBlockSnippet)
        }
        _ => Err(GeoffreyTagParseError::InvalidFullBlockSnippet),
    }
}

fn geoffrey_attributes_snippet_full_file(i: &str) -> TagParseResult<(&str, Snippet)> {
    let i = i.trim();
    if !i.is_empty() {
        return Err(GeoffreyTagParseError::InvalidFullFileSnippet);
    }

    Ok((i, Snippet::FullFile))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_geoffrey_attributes() {
        assert_eq!(geoffrey_attribute("[foo]"), Ok(("", "foo")));
        assert_eq!(geoffrey_attribute(" [foo]"), Ok(("", "foo")));
        assert_eq!(geoffrey_attribute("[foo] "), Ok(("", "foo")));
        assert_eq!(geoffrey_attribute(" [foo] "), Ok(("", "foo")));
        assert_eq!(geoffrey_attribute("\t[foo]\t"), Ok(("", "foo")));
        assert_eq!(geoffrey_attribute("[ foo ]"), Ok(("", "foo")));
        assert_eq!(geoffrey_attribute("[foo bar]"), Ok(("", "foo bar")));
        assert_eq!(geoffrey_attribute("[foo]["), Ok(("[", "foo")));
        assert_eq!(geoffrey_attribute("[foo]]"), Ok(("]", "foo")));
        assert_eq!(geoffrey_attribute("[foo]bar["), Ok(("bar[", "foo")));
        assert_eq!(geoffrey_attribute("[foo]bar]"), Ok(("bar]", "foo")));
        assert_eq!(geoffrey_attribute("[foo]bar"), Ok(("bar", "foo")));
    }

    #[test]
    fn invalid_geoffrey_attributes() {
        assert!(geoffrey_attribute("").is_err());
        assert!(geoffrey_attribute("foo").is_err());
        assert!(geoffrey_attribute("foo[").is_err());
        assert!(geoffrey_attribute("foo[]").is_err());
        assert!(geoffrey_attribute("foo[bar]").is_err());
        assert!(geoffrey_attribute("[]").is_err());
        assert!(geoffrey_attribute("[[]").is_err());
        assert!(geoffrey_attribute("[]]").is_err());
        assert!(geoffrey_attribute("[foo[]").is_err());
        assert!(geoffrey_attribute("[foo[bar]").is_err());
        assert!(geoffrey_attribute("[[foo]").is_err());
        assert!(geoffrey_attribute("[]foo]").is_err());
    }

    #[test]
    fn valid_geoffrey_attributes_snippet_full_file() {
        assert_eq!(
            geoffrey_attributes_snippet_full_file(""),
            Ok(("", Snippet::FullFile))
        );
        assert_eq!(
            geoffrey_attributes_snippet_full_file(" "),
            Ok(("", Snippet::FullFile))
        );
        assert_eq!(
            geoffrey_attributes_snippet_full_file("  "),
            Ok(("", Snippet::FullFile))
        );
        assert_eq!(
            geoffrey_attributes_snippet_full_file("\t"),
            Ok(("", Snippet::FullFile))
        );
        assert_eq!(
            geoffrey_attributes_snippet_full_file(" \t"),
            Ok(("", Snippet::FullFile))
        );
        assert_eq!(
            geoffrey_attributes_snippet_full_file("\t "),
            Ok(("", Snippet::FullFile))
        );
        assert_eq!(
            geoffrey_attributes_snippet_full_file(" \t "),
            Ok(("", Snippet::FullFile))
        );
    }

    #[test]
    fn invalid_geoffrey_attributes_snippet_full_file() {
        assert!(geoffrey_attributes_snippet_full_file("foo").is_err());
        assert!(geoffrey_attributes_snippet_full_file("[foo]").is_err());
    }

    #[test]
    fn valid_geoffrey_attributes_snippet_full_block() {
        assert_eq!(
            geoffrey_attributes_snippet_full_block("[foo]"),
            Ok(("", Snippet::FullBlock { id: "foo" }))
        );
        assert_eq!(
            geoffrey_attributes_snippet_full_block("[foo bar]"),
            Ok(("", Snippet::FullBlock { id: "foo bar" }))
        );
        assert_eq!(
            geoffrey_attributes_snippet_full_block("[foo] bar"),
            Ok(("bar", Snippet::FullBlock { id: "foo" }))
        );
        assert_eq!(
            geoffrey_attributes_snippet_full_block(" [foo] "),
            Ok(("", Snippet::FullBlock { id: "foo" }))
        );
        assert_eq!(
            geoffrey_attributes_snippet_full_block("\t[foo]\t"),
            Ok(("", Snippet::FullBlock { id: "foo" }))
        );
        assert_eq!(
            geoffrey_attributes_snippet_full_block("[foo]["),
            Ok(("[", Snippet::FullBlock { id: "foo" }))
        );
    }

    #[test]
    fn invalid_geoffrey_attributes_snippet_full_block() {
        assert!(geoffrey_attributes_snippet_full_block("foo").is_err());
        assert!(geoffrey_attributes_snippet_full_block("foo[").is_err());
        assert!(geoffrey_attributes_snippet_full_block("[foo[").is_err());
    }

    #[test]
    fn valid_geoffrey_attributes_snippet_elided_block() {
        assert_eq!(
            geoffrey_attributes_snippet_elided_block("[[foo]]"),
            Ok((
                "",
                Snippet::ElidedBlock {
                    main_id: "foo",
                    sub_ids: vec![]
                }
            ))
        );
        assert_eq!(
            geoffrey_attributes_snippet_elided_block("[[foo bar]]"),
            Ok((
                "",
                Snippet::ElidedBlock {
                    main_id: "foo bar",
                    sub_ids: vec![]
                }
            ))
        );
        assert_eq!(
            geoffrey_attributes_snippet_elided_block(" [[foo]] "),
            Ok((
                "",
                Snippet::ElidedBlock {
                    main_id: "foo",
                    sub_ids: vec![]
                }
            ))
        );
        assert_eq!(
            geoffrey_attributes_snippet_elided_block("\t[[foo]]\t"),
            Ok((
                "",
                Snippet::ElidedBlock {
                    main_id: "foo",
                    sub_ids: vec![]
                }
            ))
        );
        assert_eq!(
            geoffrey_attributes_snippet_elided_block("[[foo][bar]]"),
            Ok((
                "",
                Snippet::ElidedBlock {
                    main_id: "foo",
                    sub_ids: vec!["bar"]
                }
            ))
        );
        assert_eq!(
            geoffrey_attributes_snippet_elided_block("[[foo] [bar]]"),
            Ok((
                "",
                Snippet::ElidedBlock {
                    main_id: "foo",
                    sub_ids: vec!["bar"]
                }
            ))
        );
        assert_eq!(
            geoffrey_attributes_snippet_elided_block("[ [foo][bar] ]"),
            Ok((
                "",
                Snippet::ElidedBlock {
                    main_id: "foo",
                    sub_ids: vec!["bar"]
                }
            ))
        );
        assert_eq!(
            geoffrey_attributes_snippet_elided_block("[[foo]\t[bar]]"),
            Ok((
                "",
                Snippet::ElidedBlock {
                    main_id: "foo",
                    sub_ids: vec!["bar"]
                }
            ))
        );
        assert_eq!(
            geoffrey_attributes_snippet_elided_block("[ [foo] [bar] ]"),
            Ok((
                "",
                Snippet::ElidedBlock {
                    main_id: "foo",
                    sub_ids: vec!["bar"]
                }
            ))
        );
        assert_eq!(
            geoffrey_attributes_snippet_elided_block("[ [ foo ] [ bar ] ]"),
            Ok((
                "",
                Snippet::ElidedBlock {
                    main_id: "foo",
                    sub_ids: vec!["bar"]
                }
            ))
        );
        assert_eq!(
            geoffrey_attributes_snippet_elided_block("[[foo] [bar] [baz]]"),
            Ok((
                "",
                Snippet::ElidedBlock {
                    main_id: "foo",
                    sub_ids: vec!["bar", "baz"]
                }
            ))
        );
        assert_eq!(
            geoffrey_attributes_snippet_elided_block("[[foo]]bar"),
            Ok((
                "bar",
                Snippet::ElidedBlock {
                    main_id: "foo",
                    sub_ids: vec![]
                }
            ))
        );
        assert_eq!(
            geoffrey_attributes_snippet_elided_block("[[foo]][bar"),
            Ok((
                "[bar",
                Snippet::ElidedBlock {
                    main_id: "foo",
                    sub_ids: vec![]
                }
            ))
        );
    }

    #[test]
    fn invalid_geoffrey_attributes_snippet_elided_block() {
        assert!(geoffrey_attributes_snippet_elided_block("foo").is_err());
        assert!(geoffrey_attributes_snippet_elided_block("[foo]").is_err());
        assert!(geoffrey_attributes_snippet_elided_block("[foo]]").is_err());
        assert!(geoffrey_attributes_snippet_elided_block("[[foo] bar]").is_err());
        assert!(geoffrey_attributes_snippet_elided_block("[[foo] bar]]").is_err());
        assert!(geoffrey_attributes_snippet_elided_block("[[foo] [bar]").is_err());
        assert!(geoffrey_attributes_snippet_elided_block("[[foo]-[bar]]").is_err());
        assert!(geoffrey_attributes_snippet_elided_block("[foo][bar]]").is_err());
        assert!(geoffrey_attributes_snippet_elided_block("[[foo][]]").is_err());
    }

    #[test]
    fn valid_geoffrey_attributes_snippet_list() {
        assert_eq!(
            geoffrey_attributes_snippet_list(""),
            Ok(("", Snippet::FullFile))
        );
        assert_eq!(
            geoffrey_attributes_snippet_list("  "),
            Ok(("", Snippet::FullFile))
        );
        assert_eq!(
            geoffrey_attributes_snippet_list("[foo]"),
            Ok(("", Snippet::FullBlock { id: "foo" }))
        );
        assert_eq!(
            geoffrey_attributes_snippet_list("[ foo ]"),
            Ok(("", Snippet::FullBlock { id: "foo" }))
        );
        assert_eq!(
            geoffrey_attributes_snippet_list("[foo] bar"),
            Ok(("bar", Snippet::FullBlock { id: "foo" }))
        );
        assert_eq!(
            geoffrey_attributes_snippet_list("[[foo]]"),
            Ok((
                "",
                Snippet::ElidedBlock {
                    main_id: "foo",
                    sub_ids: vec![]
                }
            ))
        );
        assert_eq!(
            geoffrey_attributes_snippet_list("[[  foo  ]]"),
            Ok((
                "",
                Snippet::ElidedBlock {
                    main_id: "foo",
                    sub_ids: vec![]
                }
            ))
        );
        assert_eq!(
            geoffrey_attributes_snippet_list("[[foo][bar]]"),
            Ok((
                "",
                Snippet::ElidedBlock {
                    main_id: "foo",
                    sub_ids: vec!["bar"]
                }
            ))
        );
        assert_eq!(
            geoffrey_attributes_snippet_list("[[foo]] bar"),
            Ok((
                "bar",
                Snippet::ElidedBlock {
                    main_id: "foo",
                    sub_ids: vec![]
                }
            ))
        );
    }

    #[test]
    fn invalid_geoffrey_attributes_snippet_list() {
        assert!(geoffrey_attributes_snippet_list("foo").is_err());
        assert!(geoffrey_attributes_snippet_list("foo bar").is_err());
        assert!(geoffrey_attributes_snippet_list("[foo").is_err());
        assert!(geoffrey_attributes_snippet_list("foo[").is_err());
        assert!(geoffrey_attributes_snippet_list("[foo[").is_err());
        assert!(geoffrey_attributes_snippet_list("[]").is_err());
        assert!(geoffrey_attributes_snippet_list("[[foo[").is_err());
        assert!(geoffrey_attributes_snippet_list("[[foo][").is_err());
        assert!(geoffrey_attributes_snippet_list("[[foo][]]").is_err());
        assert!(geoffrey_attributes_snippet_list("[[]]").is_err());
    }

    #[test]
    fn valid_geoffrey_tag_envelope() {
        assert_eq!(
            geoffrey_tag_envelope(1, "<!--[geoffrey] foo-->"),
            Ok((" foo", 0))
        );
        assert_eq!(
            geoffrey_tag_envelope(0, "<!--[geoffrey][foo.cpp]-->"),
            Ok(("[foo.cpp]", 0))
        );
        assert_eq!(
            geoffrey_tag_envelope(1, "<!--[geoffrey][foo.cpp][bar]-->"),
            Ok(("[foo.cpp][bar]", 0))
        );
        assert_eq!(
            geoffrey_tag_envelope(1, "<!--[geoffrey][foo.cpp][[bar]]-->"),
            Ok(("[foo.cpp][[bar]]", 0))
        );
        assert_eq!(
            geoffrey_tag_envelope(1, "<!--[geoffrey][foo.cpp]\n-->"),
            Ok(("[foo.cpp]\n", 0))
        );
        assert_eq!(
            geoffrey_tag_envelope(1, "<!-- [geoffrey] [foo.cpp] -->"),
            Ok((" [foo.cpp] ", 0))
        );
        assert_eq!(
            geoffrey_tag_envelope(1, "<!--\t[geoffrey]\t[foo.cpp]\t-->"),
            Ok(("\t[foo.cpp]\t", 0))
        );
        assert_eq!(
            geoffrey_tag_envelope(1, "<!-- [geoffrey] [foo.cpp] --> "),
            Ok((" [foo.cpp] ", 0))
        );
        assert_eq!(
            geoffrey_tag_envelope(1, "  <!-- [geoffrey] [foo.cpp] -->"),
            Ok((" [foo.cpp] ", 2))
        );
        assert_eq!(
            geoffrey_tag_envelope(1, "\t<!-- [geoffrey] [foo.cpp] -->"),
            Ok((" [foo.cpp] ", 1))
        );
    }

    #[test]
    fn invalid_geoffrey_tag_envelope() {
        assert!(geoffrey_tag_envelope(0, "<!-- [geof] [foo.cpp] -->").is_err());
        assert!(geoffrey_tag_envelope(1, "<!- [geoffrey] [foo.cpp] -->").is_err());
        assert!(geoffrey_tag_envelope(1, "<-- [geoffrey] [foo.cpp] -->").is_err());
        assert!(geoffrey_tag_envelope(1, "<!-- [geoffrey] [foo.cpp] --").is_err());
        assert!(geoffrey_tag_envelope(1, "<!-- [geoffrey] [foo.cpp] ->").is_err());
        assert!(geoffrey_tag_envelope(1, "<!-- geoffrey [foo.cpp] -->").is_err());

        assert!(geoffrey_tag_envelope(1, "<!-- [geoffrey] [foo.cpp] --> bar").is_err());
    }

    #[test]
    fn valid_geoffrey_tag_data() {
        assert_eq!(
            geoffrey_tag_data("[foo.cpp]"),
            Ok(("foo.cpp", Snippet::FullFile))
        );
        assert_eq!(
            geoffrey_tag_data(" [foo.cpp] "),
            Ok(("foo.cpp", Snippet::FullFile))
        );
        assert_eq!(
            geoffrey_tag_data("\t[foo.cpp]\t"),
            Ok(("foo.cpp", Snippet::FullFile))
        );
        assert_eq!(
            geoffrey_tag_data("[foo.cpp][bar]"),
            Ok(("foo.cpp", Snippet::FullBlock { id: "bar" }))
        );
        assert_eq!(
            geoffrey_tag_data("[foo.cpp][[bar]]"),
            Ok((
                "foo.cpp",
                Snippet::ElidedBlock {
                    main_id: "bar",
                    sub_ids: vec![]
                }
            ))
        );
        assert_eq!(
            geoffrey_tag_data("[foo.cpp][[bar][baz]]"),
            Ok((
                "foo.cpp",
                Snippet::ElidedBlock {
                    main_id: "bar",
                    sub_ids: vec!["baz"]
                }
            ))
        );
    }

    #[test]
    fn invalid_geoffrey_tag_data() {
        assert!(geoffrey_tag_data("foo").is_err());
        assert!(geoffrey_tag_data(" foo").is_err());
        assert!(geoffrey_tag_data("[foo.cpp] bar").is_err());
        assert!(geoffrey_tag_data("[foo.cpp] [bar] baz").is_err());
        assert!(geoffrey_tag_data("[foo.cpp] [[bar]] baz").is_err());
    }

    #[test]
    fn valid_md_code_block_begin() {
        assert_eq!(md_code_block_begin("```"), Ok(("```", "```")));
        assert_eq!(md_code_block_begin("``` "), Ok(("```", "```")));
        assert_eq!(md_code_block_begin("```\t"), Ok(("```", "```")));
        assert_eq!(md_code_block_begin("````"), Ok(("````", "````")));
        assert_eq!(md_code_block_begin(" ```"), Ok(("```", "```")));
        assert_eq!(md_code_block_begin("```cpp"), Ok(("```cpp", "```")));
    }

    #[test]
    fn invalid_md_code_block_begin() {
        assert!(md_code_block_begin("abc").is_err());
        assert!(md_code_block_begin("abc").is_err());
        assert!(md_code_block_begin("` ``").is_err());
        assert!(md_code_block_begin("``` ``").is_err());
        assert!(md_code_block_begin("``").is_err());
        assert!(md_code_block_begin("```cpp rs").is_err());
    }

    #[test]
    fn valid_md_code_block_end() {
        let mut lines = "```".lines().enumerate();
        assert_eq!(
            md_code_block_end(&mut lines, "```", ParseContext::default()),
            Ok("```")
        );
        assert_eq!(lines.count(), 0);

        let mut lines = "````\n".lines().enumerate();
        assert_eq!(
            md_code_block_end(&mut lines, "````", ParseContext::default()),
            Ok("````")
        );
        assert_eq!(lines.count(), 0);

        let mut lines = "``` \n".lines().enumerate();
        assert_eq!(
            md_code_block_end(&mut lines, "```", ParseContext::default()),
            Ok("```")
        );

        let mut lines = " ```\n".lines().enumerate();
        assert_eq!(
            md_code_block_end(&mut lines, "```", ParseContext::default()),
            Ok("```")
        );

        let mut lines = "\n```\n".lines().enumerate();
        assert_eq!(
            md_code_block_end(&mut lines, "```", ParseContext::default()),
            Ok("```")
        );

        let mut lines = "int main() {}\n```\n".lines().enumerate();
        assert_eq!(
            md_code_block_end(&mut lines, "```", ParseContext::default()),
            Ok("```")
        );

        let mut lines = "```\n\n# Foo".lines().enumerate();
        assert_eq!(
            md_code_block_end(&mut lines, "```", ParseContext::default()),
            Ok("```")
        );
        assert_eq!(lines.collect::<Vec<_>>(), vec![(1, ""), (2, "# Foo")]);

        let mut lines = "```\n````\n".lines().enumerate();
        assert_eq!(
            md_code_block_end(&mut lines, "````", ParseContext::default()),
            Ok("````")
        );
        assert_eq!(lines.collect::<Vec<_>>(), vec![]);

        let mut lines = "```\n````\n".lines().enumerate();
        assert_eq!(
            md_code_block_end(&mut lines, "```", ParseContext::default()),
            Ok("```")
        );
        assert_eq!(lines.collect::<Vec<_>>(), vec![(1, "````")]);
    }

    #[test]
    fn invalid_md_code_block_end() {
        let code_block_begin_context = ParseContext {
            line: 42,
            text: "    ````cpp",
        };

        let mut lines = "```\n".lines().enumerate();
        assert!(md_code_block_end(&mut lines, "````", code_block_begin_context).is_err());

        let mut lines = "````".lines().enumerate();
        assert!(md_code_block_end(&mut lines, "```", code_block_begin_context).is_err());

        let mut lines = "```` ```".lines().enumerate();
        assert!(md_code_block_end(&mut lines, "```", code_block_begin_context).is_err());

        let mut lines = "``\n".lines().enumerate();
        assert!(md_code_block_end(&mut lines, "```", code_block_begin_context).is_err());

        let mut lines = "```cpp\n".lines().enumerate();
        assert!(md_code_block_end(&mut lines, "```", code_block_begin_context).is_err());

        let mut lines = "a```\n".lines().enumerate();
        assert!(md_code_block_end(&mut lines, "```", code_block_begin_context).is_err());
    }

    #[test]
    fn valid_md_code_block() {
        let mut lines = "```\n```\n".lines().enumerate();
        assert_eq!(
            md_code_block(&mut lines, ParseContext::default()),
            Ok(("```", "```"))
        );
        assert_eq!(lines.count(), 0);

        let mut lines = " ```\n ```\n".lines().enumerate();
        assert_eq!(
            md_code_block(&mut lines, ParseContext::default()),
            Ok(("```", "```"))
        );

        let mut lines = "```cpp\n```\n".lines().enumerate();
        assert_eq!(
            md_code_block(&mut lines, ParseContext::default()),
            Ok(("```cpp", "```"))
        );

        let mut lines = "```\n```\n\n# Foo".lines().enumerate();
        assert_eq!(
            md_code_block(&mut lines, ParseContext::default()),
            Ok(("```", "```"))
        );
        assert_eq!(lines.collect::<Vec<_>>(), vec![(2, ""), (3, "# Foo")]);
    }

    #[test]
    fn invalid_md_code_block() {
        let tag_context = ParseContext {
            line: 42,
            text: "<!-- [geoffrey] [foo.cpp] -->",
        };

        let mut lines = "```\n````\n".lines().enumerate();
        assert!(md_code_block(&mut lines, tag_context).is_err());

        let mut lines = "````\n```\n".lines().enumerate();
        assert!(md_code_block(&mut lines, tag_context).is_err());

        let mut lines = "``\n``\n".lines().enumerate();
        assert!(md_code_block(&mut lines, tag_context).is_err());

        let mut lines = "``` ```".lines().enumerate();
        assert!(md_code_block(&mut lines, tag_context).is_err());

        let mut lines = "```\n".lines().enumerate();
        assert!(md_code_block(&mut lines, tag_context).is_err());

        let mut lines = "```cpp rs\n```\n".lines().enumerate();
        assert!(md_code_block(&mut lines, tag_context).is_err());
    }

    #[test]
    fn valid_geoffrey_code_block() {
        let mut lines = "```cpp\n```\n".lines().enumerate();
        assert_eq!(
            geoffrey_code_block(4, " [foo.cpp] ", &mut lines, ParseContext::default()),
            Ok(Section::GeoffreyCodeBlock {
                indentation: 4,
                tag: GeoffreyTag {
                    file_name: "foo.cpp",
                    snippet: Snippet::FullFile,
                    options: Vec::new()
                },
                begin: "```cpp",
                end: "```",
            })
        );
        assert_eq!(lines.count(), 0);

        let mut lines = "```cpp\n```\n".lines().enumerate();
        assert_eq!(
            geoffrey_code_block(0, "[foo.cpp] [bar]", &mut lines, ParseContext::default()),
            Ok(Section::GeoffreyCodeBlock {
                indentation: 0,
                tag: GeoffreyTag {
                    file_name: "foo.cpp",
                    snippet: Snippet::FullBlock { id: "bar" },
                    options: Vec::new()
                },
                begin: "```cpp",
                end: "```",
            })
        );

        let mut lines = "```cpp\nint maint() {}\n```\n".lines().enumerate();
        assert_eq!(
            geoffrey_code_block(0, "[foo.cpp]", &mut lines, ParseContext::default()),
            Ok(Section::GeoffreyCodeBlock {
                indentation: 0,
                tag: GeoffreyTag {
                    file_name: "foo.cpp",
                    snippet: Snippet::FullFile,
                    options: Vec::new()
                },
                begin: "```cpp",
                end: "```",
            })
        );

        let mut lines = "````cpp\n```\n````\n".lines().enumerate();
        assert_eq!(
            geoffrey_code_block(0, "[foo.cpp]", &mut lines, ParseContext::default()),
            Ok(Section::GeoffreyCodeBlock {
                indentation: 0,
                tag: GeoffreyTag {
                    file_name: "foo.cpp",
                    snippet: Snippet::FullFile,
                    options: Vec::new()
                },
                begin: "````cpp",
                end: "````",
            })
        );

        let mut lines = "```cpp\n<!-- [geoffrey] [bar.cpp] -->\n```\n"
            .lines()
            .enumerate();
        assert_eq!(
            geoffrey_code_block(0, "[foo.cpp]", &mut lines, ParseContext::default()),
            Ok(Section::GeoffreyCodeBlock {
                indentation: 0,
                tag: GeoffreyTag {
                    file_name: "foo.cpp",
                    snippet: Snippet::FullFile,
                    options: Vec::new()
                },
                begin: "```cpp",
                end: "```",
            })
        );

        let mut lines = "```cpp\n```\n\n# Foo".lines().enumerate();
        assert_eq!(
            geoffrey_code_block(4, " [foo.cpp] ", &mut lines, ParseContext::default()),
            Ok(Section::GeoffreyCodeBlock {
                indentation: 4,
                tag: GeoffreyTag {
                    file_name: "foo.cpp",
                    snippet: Snippet::FullFile,
                    options: Vec::new()
                },
                begin: "```cpp",
                end: "```",
            })
        );
        assert_eq!(lines.collect::<Vec<_>>(), vec![(2, ""), (3, "# Foo")]);
    }

    #[test]
    fn invalid_geoffrey_code_block() {
        let tag_context = ParseContext {
            line: 42,
            text: "<!-- [geoffrey] [foo.cpp] -->",
        };

        let mut lines = "```\n```\n".lines().enumerate();
        assert!(geoffrey_code_block(0, " [foo.cpp]] ", &mut lines, tag_context).is_err());

        let mut lines = "```\n```\n".lines().enumerate();
        assert!(geoffrey_code_block(0, " [[foo.cpp] ", &mut lines, tag_context).is_err());

        let mut lines = "```\n```\n".lines().enumerate();
        assert!(geoffrey_code_block(0, " [foo.cpp] [", &mut lines, tag_context).is_err());

        let mut lines = "```\n```\n".lines().enumerate();
        assert!(geoffrey_code_block(0, "[foo.cpp] [[foo] bar]", &mut lines, tag_context).is_err());

        let mut lines = "````\n```\n".lines().enumerate();
        assert!(geoffrey_code_block(0, "[foo.cpp]", &mut lines, tag_context).is_err());
    }

    #[test]
    fn valid_parse_section() {
        let mut lines = "```cpp\n```\n".lines().enumerate();
        assert_eq!(
            parse_section(0, "<!-- [geoffrey] [foo.cpp] -->", &mut lines),
            Ok(Section::GeoffreyCodeBlock {
                indentation: 0,
                tag: GeoffreyTag {
                    file_name: "foo.cpp",
                    snippet: Snippet::FullFile,
                    options: Vec::new()
                },
                begin: "```cpp",
                end: "```",
            })
        );

        let mut lines = "```cpp\n```\n".lines().enumerate();
        assert_eq!(
            parse_section(0, "  <!-- [geoffrey] [foo.cpp] [bar] -->", &mut lines),
            Ok(Section::GeoffreyCodeBlock {
                indentation: 2,
                tag: GeoffreyTag {
                    file_name: "foo.cpp",
                    snippet: Snippet::FullBlock { id: "bar" },
                    options: Vec::new()
                },
                begin: "```cpp",
                end: "```",
            })
        );

        let mut lines = "```cpp\n```\n".lines().enumerate();
        assert_eq!(
            parse_section(
                0,
                "<!-- [geoffrey] [foo.cpp] [[bar] [baz][bazz]] -->",
                &mut lines
            ),
            Ok(Section::GeoffreyCodeBlock {
                indentation: 0,
                tag: GeoffreyTag {
                    file_name: "foo.cpp",
                    snippet: Snippet::ElidedBlock {
                        main_id: "bar",
                        sub_ids: vec!["baz", "bazz"]
                    },
                    options: Vec::new()
                },
                begin: "```cpp",
                end: "```",
            })
        );

        let mut lines = "```cpp\n```\n# Foo".lines().enumerate();
        assert_eq!(
            parse_section(0, "<!-- [geoffrey] [foo.cpp] -->", &mut lines),
            Ok(Section::GeoffreyCodeBlock {
                indentation: 0,
                tag: GeoffreyTag {
                    file_name: "foo.cpp",
                    snippet: Snippet::FullFile,
                    options: Vec::new()
                },
                begin: "```cpp",
                end: "```",
            })
        );
        assert_eq!(lines.collect::<Vec<_>>(), vec![(2, "# Foo")]);

        let mut lines = "".lines().enumerate();
        assert_eq!(
            parse_section(0, " <!--[geoff]--> ", &mut lines),
            Ok(Section::TextLine(" <!--[geoff]--> "),)
        );
    }

    #[test]
    fn invalid_parse_section() {
        let mut lines = "".lines().enumerate();
        assert!(parse_section(0, "<!-- [geoffrey] [foo.cpp]] -->", &mut lines).is_err());

        let mut lines = "".lines().enumerate();
        assert!(parse_section(0, "<!-- [geoffrey] [[foo.cpp] -->", &mut lines).is_err());

        let mut lines = "".lines().enumerate();
        assert!(parse_section(0, "<!-- [geoffrey] [foo.cpp] [-->", &mut lines).is_err());

        let mut lines = "````\n```\n".lines().enumerate();
        assert!(parse_section(0, "<!-- [geoffrey] [foo.cpp] -->", &mut lines).is_err());

        let mut lines = "```\n````\n".lines().enumerate();
        assert!(parse_section(0, "<!-- [geoffrey] [foo.cpp] -->", &mut lines).is_err());

        let mut lines = "\n```cpp\n```\n# Foo".lines().enumerate();
        assert!(parse_section(0, "<!-- [geoffrey] [foo.cpp] -->", &mut lines).is_err());

        let mut lines = "```cpp```\n# Foo".lines().enumerate();
        assert!(parse_section(0, "<!-- [geoffrey] [foo.cpp] -->", &mut lines).is_err());
    }

    fn read_document() -> Document<'static> {
        Document {
            content: r"# Heading
  <!--[geoffrey][foo.cpp][bar]-->
  ```cpp
  int main() {
    return 0;
  }
  ```
Some text

More text
<!--[geoffrey][bar.cpp][foo]-->
```cpp
int answer() {
  return 42;
}
```
"
            .into(),
            has_geoffrey_code_blocks: false,
            sections: Vec::new(),
        }
    }

    #[test]
    fn valid_parse() {
        let mut doc = read_document();

        doc.sections = parse(&doc.content).unwrap();

        let mut expected_sections = Vec::new();
        expected_sections.push(Section::TextLine("# Heading"));
        expected_sections.push(Section::GeoffreyCodeBlock {
            indentation: 2,
            tag: GeoffreyTag {
                file_name: "foo.cpp",
                snippet: Snippet::FullBlock { id: "bar" },
                options: Vec::new(),
            },
            begin: "```cpp",
            end: "```",
        });
        expected_sections.push(Section::TextLine("Some text"));
        expected_sections.push(Section::TextLine(""));
        expected_sections.push(Section::TextLine("More text"));
        expected_sections.push(Section::GeoffreyCodeBlock {
            indentation: 0,
            tag: GeoffreyTag {
                file_name: "bar.cpp",
                snippet: Snippet::FullBlock { id: "foo" },
                options: Vec::new(),
            },
            begin: "```cpp",
            end: "```",
        });

        assert_eq!(doc.sections, expected_sections);
    }
}
