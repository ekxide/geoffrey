// SPDX-License-Identifier: Apache-2.0

use nom::bytes::complete::{is_not, tag, take_while};
use nom::character::complete::{alphanumeric0, space0};
use nom::combinator;
use nom::sequence;
use nom::Parser;

use thiserror::Error;

use std::iter::Enumerate;
use std::str::Lines;

#[derive(Debug, PartialEq, Eq)]
pub struct Document<'a> {
    pub content: String,
    pub has_snippets: bool,
    pub sections: Vec<Snippet<'a>>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Snippet<'a> {
    TextLine(&'a str),
    GeoffreyCodeBlock {
        indentation: usize,
        // tag: GeoffreyTag<'a>,
        begin: &'a str,
        end: &'a str,
    },
}

#[derive(Error, Debug, PartialEq, Eq)]
pub enum ContentParseError {
    #[error("Not a code snippet tag")]
    NotACodeSnippetTAg,
}


type NomError<E> = nom::Err<nom::error::Error<E>>;

type ParseResult<T> = std::result::Result<T, ContentParseError>;



pub fn parse(file_content: &str) -> ParseResult<Vec<Snippet<'_>>> {
    let mut sections = Vec::new();
    let mut lines = file_content.lines().enumerate();
    while let Some((n, i)) = lines.next() {
        // let section = parse_section(n, i, &mut lines)?;
        // sections.push(section);
    }

    Ok(sections)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_snippet_tags() {
    }

}
