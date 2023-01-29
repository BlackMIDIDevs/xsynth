#![allow(clippy::manual_strip)]
#![allow(clippy::uninlined_format_args)]

use std::borrow::Cow;

use soundfonts_macro::bnf;

mod opcode_simd;
use opcode_simd::parse_opcode_name_simd;

pub mod helpers;
use helpers::*;

// Basically I made a custom bnf syntax that automatically converts into rust code
//
// The syntax is:
// Tag = <AnotherTag> "some text" #"some regex";
// Labelled = name:<Tag> name2:#"some regex";
//
// enum Enum = [Tag1 | Tag2 | Tag3];
//
// Modifiers = <?Optional> <!Not> <[Array]> <[ArrayUntilEof]^> <(LookAhead)>;
// CustomFn = (fn_name)
// Eof = ^;
//

bnf! {
    Include = "#include" <Spaces> "\"" path:#"[^\"]+" "\"";
    Group = "<" name:#"\\w+" ">";
    Comment = "//" <UntilNextLine>;

    Opcode = name:<OpcodeName> <?Spaces> "=" <?Spaces> value: <OpcodeValue>;
    OpcodeValue = first:<OpcodeValuePart> rest:<[OpcodeValuePart]>;

    // We have "beginnings" so that we can know when to stop parsing an opcode value faster
    IncludeBeginning = "#include";
    GroupBeginning = "<";
    CommentBeginning = "//";
    OpcodeBeginning = <OpcodeName> <?Spaces> "=";
    enum TokenBeginning = [OpcodeBeginning | GroupBeginning | IncludeBeginning | CommentBeginning];

    IsValidTokenAheadOnSameLine = <?Spaces> <TokenBeginning>;
    DoesLineEndAfter = <?Spaces> <NewLine>;
    enum IsEndOfOpcodeString = [DoesLineEndAfter | IsValidTokenAheadOnSameLine];
    OpcodeValuePart = <!IsEndOfOpcodeString> value:<ParseOpcodeValuePart>;

    enum TokenKind = [Opcode | Group | Include | Comment];
    Token = kind:<TokenKind> <?SpacedAndNewLines>;
    Root = items:<[Token]^>;

    enum ErrorTolerantToken = [Token | SkipErroringLine];
    ErrorTolerantRoot = items:<[ErrorTolerantToken]^>;

    Spaces = #"[ ]+";
    UntilNextLine = #"[^\r\n]*" <NewLineOrEof>;
    ParseOpcodeValuePart = text:#"[^ \n\r]+[ ]*";
    SpacedAndNewLines = #"[ \n\r]+";
    NewLine = #"[\n\r]+";
    enum NewLineOrEof = [NewLine | Eof];
    Eof = ^;
    Empty = ;

    SkipErroringLine = #"[^\r\n]*" <NewLine>; // Can't use NewLineOrEof or we may end up in n infinite loop

    // OpcodeName = name:#"[\\w\\$]+";
    OpcodeName = name:(parse_opcode_name_simd);
}

impl<'a> Root<'a> {
    pub fn parse_full(s: &'a str) -> Result<Self, ParseError> {
        let parser = StringParser::new(s);
        let result = Self::parse(parser);

        result.map(|(r, _)| r)
    }
}

impl<'a> ErrorTolerantRoot<'a> {
    pub fn parse_full(s: &'a str) -> Result<Self, ParseError> {
        let parser = StringParser::new(s);
        let result = Self::parse(parser);

        result.map(|(r, _)| r)
    }
}

impl<'a> OpcodeValue<'a> {
    pub fn as_string(&self) -> Cow<'a, str> {
        if self.rest.is_empty() {
            return Cow::Borrowed(self.first.value.text.text);
        } else {
            let mut result = String::from(self.first.value.text.text);
            for part in self.rest.iter() {
                result.push_str(part.value.text.text);
            }
            return Cow::Owned(result);
        }
    }
}

impl<'a> Token<'a> {
    pub fn parse_as_iter(s: &'a str) -> impl Iterator<Item = Result<Token<'a>, ParseError>> {
        let mut parser = StringParser::new(s);
        std::iter::from_fn(move || {
            let result = Self::parse(parser);

            if let Err(e) = result {
                if parser.is_empty() {
                    return None;
                } else {
                    return Some(Err(e));
                }
            }

            Some(result.map(|(r, p)| {
                parser = p;
                r
            }))
        })
    }
}

impl<'a> ErrorTolerantToken<'a> {
    pub fn parse_as_iter(s: &'a str) -> impl Iterator<Item = Result<Token<'a>, ParseError>> {
        let mut parser = StringParser::new(s);
        std::iter::from_fn(move || {
            let result = Self::parse(parser);

            if let Err(e) = result {
                if parser.is_empty() {
                    return None;
                } else {
                    return Some(Err(e));
                }
            }

            Some(result.map(|(r, p)| {
                parser = p;
                r
            }))
        })
        .filter_map(|f| match f {
            Ok(ErrorTolerantToken::SkipErroringLine(_)) => None,
            Ok(ErrorTolerantToken::Token(t)) => Some(Ok(t)),
            Err(e) => Some(Err(e)),
        })
    }
}
