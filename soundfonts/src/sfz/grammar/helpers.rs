#![allow(clippy::result_unit_err)]

use std::{cell::RefCell, collections::HashMap};

use thiserror::Error;

const PARSE_DEBUG: bool = false;

#[derive(Debug, Clone, Copy)]
pub struct FileLocation {
    pub line_number: usize,
    pub position: usize,
    pub index: usize,
}

impl FileLocation {
    pub fn start() -> Self {
        Self {
            line_number: 1,
            position: 0,
            index: 0,
        }
    }

    pub fn advanced_by(mut self, text: &str) -> Self {
        let lines = text.chars().filter(|c| *c == '\n').count();
        self.line_number += lines;
        self.index += text.len();

        if lines > 0 {
            let last_newline = text.rfind('\n').unwrap();
            self.position = text.len() - last_newline;
        } else {
            self.position += text.len();
        }

        self
    }
}

impl std::fmt::Display for FileLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}:{}", self.line_number, self.position)
    }
}

#[derive(Debug, Clone)]
pub struct TextLit<'a> {
    pub text: &'a str,
    pub location: FileLocation,
}

impl<'a> TextLit<'a> {
    pub fn new(text: &'a str, location: FileLocation) -> Self {
        Self { text, location }
    }
}

#[derive(Error, Debug, Clone)]
pub struct ParseError {
    pub message: &'static str,
    pub at: FileLocation,
    pub child_errors: Vec<ParseError>,
}

impl ParseError {
    pub fn new(message: &'static str, at: FileLocation) -> Self {
        Self {
            message,
            at,
            child_errors: Vec::new(),
        }
    }

    pub fn with_child(message: &'static str, at: FileLocation, child: ParseError) -> Self {
        if child.child_errors.len() == 1 {
            // If the child only has 1 error, just pass it up to avoid deep nesting
            Self {
                message,
                at,
                child_errors: child.child_errors,
            }
        } else {
            Self {
                message,
                at,
                child_errors: vec![child],
            }
        }
    }

    pub fn with_children(
        message: &'static str,
        at: FileLocation,
        children: Vec<ParseError>,
    ) -> Self {
        Self {
            message,
            at,
            child_errors: children,
        }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}", ParseErrorFmtDepth(self, 0))?;
        Ok(())
    }
}

struct ParseErrorFmtDepth<'a>(&'a ParseError, usize);

impl std::fmt::Display for ParseErrorFmtDepth<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let err = self.0;
        let depth = self.1;

        for _ in 0..depth {
            write!(f, "  ")?;
        }
        writeln!(f, "{} at {}", err.message, err.at)?;

        for child in &err.child_errors {
            write!(f, "{}", ParseErrorFmtDepth(child, depth + 1))?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct StringParser<'a> {
    pub text: &'a str,
    pub pos: FileLocation,
}

impl<'a> StringParser<'a> {
    pub fn new(input: &'a str) -> Self {
        Self {
            text: input,
            pos: FileLocation::start(),
        }
    }

    pub fn split_at(&self, pos: usize) -> (TextLit<'a>, Self) {
        let (left, right) = self.text.split_at(pos);
        let forked = Self {
            text: right,
            pos: self.pos.advanced_by(left),
        };
        let left = TextLit::new(left, self.pos);
        (left, forked)
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }
}

thread_local! {
    pub static REGEXES: RefCell<HashMap<String,lazy_regex::Regex>>  = RefCell::new(HashMap::new());
}

pub fn parse_string_lit<'a>(
    input: StringParser<'a>,
    lit: &'static str,
) -> Result<(TextLit<'a>, StringParser<'a>), ()> {
    if input.text.starts_with(lit) {
        if PARSE_DEBUG {
            println!("Matched string literal: {:?}", lit);
        }
        Ok(input.split_at(lit.len()))
    } else {
        if PARSE_DEBUG {
            println!("Failed to match string literal: {:?}", lit);
        }
        Err(())
    }
}

pub fn get_regex(regex: &str) -> lazy_regex::Regex {
    REGEXES.with(|regexes| {
        let mut regexes = regexes.borrow_mut();
        if let Some(regex) = regexes.get(regex) {
            regex.clone()
        } else {
            let new_regex = lazy_regex::Regex::new(&format!("^{}", regex)).unwrap();
            regexes.insert(regex.to_string(), new_regex.clone());
            new_regex
        }
    })
}

pub fn parse_string_regex<'a>(
    input: StringParser<'a>,
    regex_str: &'static str,
) -> Result<(TextLit<'a>, StringParser<'a>), ()> {
    let regex = get_regex(regex_str);

    if let Some(captures) = regex.find_at(input.text, 0) {
        if PARSE_DEBUG {
            println!("Matched regex: {:?}", regex_str);
        }
        let capture = captures;
        Ok(input.split_at(capture.end()))
    } else {
        if PARSE_DEBUG {
            println!("Failed to match regex: {:?}", regex_str);
        }
        Err(())
    }
}
