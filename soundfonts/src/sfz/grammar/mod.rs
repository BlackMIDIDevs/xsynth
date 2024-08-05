#![allow(clippy::manual_strip)]
#![allow(clippy::uninlined_format_args)]

use std::borrow::Cow;

use regex_bnf::*;

mod opcode_simd;
use opcode_simd::parse_opcode_name_simd;

use self::opcode_simd::parse_opcode_value_simd;

bnf! {
    Include = "#include" <Spaces> "\"" path:r#"[^"]+"# "\"";
    Group = "<" name:r"\w+" ">";
    Define = "#define" <Spaces> variable:r"\$\w+" <Spaces> value: <OpcodeValue>;
    Comment = "//" <UntilNextLine>;

    Opcode = name:<OpcodeName> <?Spaces> "=" <?Spaces> value: <OpcodeValue>;
    OpcodeValue = first:<OpcodeValuePart> rest:<[OpcodeValuePart]*>;

    // We have "beginnings" so that we can know when to stop parsing an opcode value faster
    IncludeBeginning = "#include";
    DefineBeginning = "#define";
    GroupBeginning = "<";
    CommentBeginning = "//";
    OpcodeBeginning = <OpcodeName> <?Spaces> "=";
    enum TokenBeginning = [OpcodeBeginning | GroupBeginning | IncludeBeginning | DefineBeginning | CommentBeginning];

    IsValidTokenAheadOnSameLine = <?Spaces> <TokenBeginning>;
    DoesLineEndAfter = <?Spaces> <NewLine>;
    enum IsEndOfOpcodeString = [DoesLineEndAfter | IsValidTokenAheadOnSameLine];
    OpcodeValuePart = <!IsEndOfOpcodeString> value:<ParseOpcodeValuePart>;

    enum TokenKind = [Opcode | Group | Include | Define | Comment];
    Token = kind:<TokenKind> <?SpacedAndNewLines>;
    Root = items:<[Token]^>;

    enum ErrorTolerantToken = [Token | SkipErroringLine];
    ErrorTolerantRoot = items:<[ErrorTolerantToken]^>;

    Spaces = r"[ ]+";
    UntilNextLine = r"[^\r\n]*" <NewLineOrEof>;
    SpacedAndNewLines = r"[ \n\r]+";
    NewLine = r"[\n\r]+";
    enum NewLineOrEof = [NewLine | Eof];
    Eof = ^;
    Empty = ;

    SkipErroringLine = r"[^\r\n]*" <NewLine>; // Can't use NewLineOrEof or we may end up in n infinite loop

    // OpcodeName = name:r"[\\w\\$]+";
    OpcodeName = name:(parse_opcode_name_simd);

    // ParseOpcodeValuePart = text:r"[^ \n\r]*[ ]*";
    ParseOpcodeValuePart = text:(parse_opcode_value_simd);
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
