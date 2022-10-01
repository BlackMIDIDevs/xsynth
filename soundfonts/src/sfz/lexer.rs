use std::{fs, io, path::Path};

use lazy_regex::{regex, Regex};

#[derive(Debug, Clone)]
struct StringParser<'a> {
    input: &'a str,
}

impl<'a> StringParser<'a> {
    pub fn new(input: &'a str) -> Self {
        Self { input }
    }

    /// Try to match a string literal, advance the parser if it matches
    fn parse_literal(&mut self, literal: &str) -> Option<&'a str> {
        if self.input.starts_with(literal) {
            let len = literal.len();
            let result = &self.input[..len];
            let remaining = &self.input[len..];
            self.input = remaining;
            Some(result)
        } else {
            None
        }
    }

    /// Try to match regex, advance the parser if it matches
    fn parse_regex(&mut self, regex: &Regex) -> Option<String> {
        if let Some(caps) = regex.find_at(self.input, 0) {
            let len = caps.end();
            let result = &self.input[..len];
            let remaining = &self.input[len..];
            self.input = remaining;
            Some(result.to_owned())
        } else {
            None
        }
    }

    fn parse_until_line_end(&mut self) -> String {
        let line_end = self
            .input
            .find('\n')
            .or_else(|| self.input.find('\r'))
            .unwrap_or(self.input.len());

        let result = &self.input[..line_end];
        let remaining = &self.input[line_end..];
        self.input = remaining;
        result.trim().to_owned()
    }

    fn parse_until_space(&mut self) -> String {
        let space_regex = regex!("([\n\r ])|($)");
        let next_space = space_regex
            .find(self.input)
            .map(|v| v.start())
            .unwrap_or(self.input.len());

        let result = &self.input[..next_space];
        let remaining = &self.input[next_space..];
        self.input = remaining;
        result.to_owned()
    }

    fn trim_start(&mut self) {
        self.input = self.input.trim_start();
    }

    fn empty(&self) -> bool {
        self.input.is_empty()
    }
}

macro_rules! try_parse {
    ($parser:expr, $parse_fn:expr) => {
        let mut new_parser = $parser.clone();
        if let Some(token) = $parse_fn(&mut new_parser) {
            *$parser = new_parser;
            return Some(token);
        }
    };
    ($parser:expr, $parse_fn:expr, $enum_val:expr) => {
        let mut new_parser = $parser.clone();
        if let Some(token) = $parse_fn(&mut new_parser) {
            *$parser = new_parser;
            return Some($enum_val(token));
        }
    };
    ($parser:expr, $enum_val:expr, $val:ty, $parser_ident:ident, $parse_fn:tt) => {{
        fn parse_tag<'a>($parser_ident: &mut StringParser<'a>) -> Option<$val> {
            $parse_fn
        }
        try_parse!($parser, parse_tag, $enum_val);
    }};
}

macro_rules! try_parse_basic_tag {
    ($parser:expr, $enum_val:expr, $val:ty, $name:expr, $parse_fn:expr) => {{
        try_parse!($parser, $enum_val, $val, parser, {
            parse_basic_tag_name(parser, $name)?;
            $parse_fn(parser)
        });
    }};
}

macro_rules! try_parse_float {
    ($parser:expr, $enum_val:expr, $name:expr) => {{
        try_parse_basic_tag!($parser, $enum_val, f32, $name, parse_float);
    }};
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SfzGroupType {
    Region,
    Group,
    Master,
    Control,
    Other,
}

fn parse_equals(parser: &mut StringParser) -> Option<()> {
    parser.parse_regex(regex!(r"[ ]*=[ ]*"))?;
    Some(())
}

fn parse_basic_tag_name<'a>(parser: &mut StringParser<'a>, tag_name: &str) -> Option<()> {
    parser.parse_literal(tag_name)?;
    parse_equals(parser);
    Some(())
}

fn parse_vel_number(parser: &mut StringParser<'_>) -> Option<u8> {
    let num = parser.parse_regex(regex!(r"\d+"))?;
    num.parse().ok()
}

fn parse_key_number(parser: &mut StringParser<'_>) -> Option<u8> {
    let num = parser.parse_regex(regex!(r"\d+"))?;
    num.parse().ok()
}

fn parse_pan_number(parser: &mut StringParser<'_>) -> Option<i8> {
    let num = parser.parse_regex(regex!(r"[\-\d]+"))?;
    num.parse().ok()
}

fn parse_float(parser: &mut StringParser<'_>) -> Option<f32> {
    let num = parser.parse_regex(regex!(r"[\-\d\.]+"))?;
    num.parse().ok()
}

#[derive(Debug, Clone)]
pub enum SfzLoopMode {
    NoLoop,
    OneShot,
    LoopContinuous,
    LoopSustain,
    Other,
}

#[derive(Debug, Clone)]
pub enum SfzRegionFlags {
    Lovel(u8),
    Hivel(u8),
    Key(u8),
    Lokey(u8),
    Hikey(u8),
    PitchKeycenter(u8),
    Pan(i8),
    Sample(String),
    LoopMode(SfzLoopMode),
    Cutoff(f32),
    DefaultPath(String),
    AmpegEnvelope(SfzAmpegEnvelope),
}

#[derive(Debug, Clone)]
#[allow(clippy::enum_variant_names)]
pub enum SfzAmpegEnvelope {
    AmpegStart(f32),
    AmpegDelay(f32),
    AmpegAttack(f32),
    AmpegHold(f32),
    AmpegDecay(f32),
    AmpegSustain(f32),
    AmpegRelease(f32),
}

#[derive(Debug, Clone)]
pub enum SfzToken {
    Group(SfzGroupType),
    RegionFlag(SfzRegionFlags),
}

#[derive(Debug, Clone)]
pub enum SfzMetaToken {
    InnerToken(SfzToken),
    Import(String),
    Comment,
}

fn parse_ampeg_envelope(parser: &mut StringParser) -> Option<SfzAmpegEnvelope> {
    try_parse_float!(parser, SfzAmpegEnvelope::AmpegStart, "ampeg_start");
    try_parse_float!(parser, SfzAmpegEnvelope::AmpegDelay, "ampeg_delay");
    try_parse_float!(parser, SfzAmpegEnvelope::AmpegAttack, "ampeg_attack");
    try_parse_float!(parser, SfzAmpegEnvelope::AmpegHold, "ampeg_hold");
    try_parse_float!(parser, SfzAmpegEnvelope::AmpegDecay, "ampeg_decay");
    try_parse_float!(parser, SfzAmpegEnvelope::AmpegSustain, "ampeg_sustain");
    try_parse_float!(parser, SfzAmpegEnvelope::AmpegRelease, "ampeg_release");

    None
}

fn parse_region_flags(parser: &mut StringParser) -> Option<SfzRegionFlags> {
    try_parse!(parser, SfzRegionFlags::Sample, String, parser, {
        parse_basic_tag_name(parser, "sample")?;
        Some(parser.parse_until_line_end().replace('\\', "/"))
    });

    try_parse_basic_tag!(parser, SfzRegionFlags::Lovel, u8, "lovel", parse_vel_number);
    try_parse_basic_tag!(parser, SfzRegionFlags::Hivel, u8, "hivel", parse_vel_number);
    try_parse_basic_tag!(parser, SfzRegionFlags::Lokey, u8, "lokey", parse_key_number);
    try_parse_basic_tag!(parser, SfzRegionFlags::Hikey, u8, "hikey", parse_key_number);
    try_parse_basic_tag!(parser, SfzRegionFlags::Pan, i8, "pan", parse_pan_number);
    try_parse_basic_tag!(
        parser,
        SfzRegionFlags::PitchKeycenter,
        u8,
        "pitch_keycenter",
        parse_key_number
    );
    try_parse_basic_tag!(parser, SfzRegionFlags::Key, u8, "key", parse_key_number);
    try_parse_basic_tag!(parser, SfzRegionFlags::Cutoff, f32, "cutoff", parse_float);

    try_parse!(parser, SfzRegionFlags::LoopMode, SfzLoopMode, parser, {
        parse_basic_tag_name(parser, "loop_mode")?;
        let group_name = parser.parse_regex(regex!(r"^\w+"))?;
        match group_name.as_ref() {
            "no_loop" => Some(SfzLoopMode::NoLoop),
            "one_shot" => Some(SfzLoopMode::OneShot),
            "loop_continuous" => Some(SfzLoopMode::LoopContinuous),
            "loop_sustain" => Some(SfzLoopMode::LoopSustain),
            _ => Some(SfzLoopMode::Other),
        }
    });

    try_parse!(parser, SfzRegionFlags::DefaultPath, String, parser, {
        parse_basic_tag_name(parser, "default_path")?;
        Some(parser.parse_until_line_end())
    });

    try_parse!(
        parser,
        SfzRegionFlags::AmpegEnvelope,
        SfzAmpegEnvelope,
        parser,
        {
            let envelope = parse_ampeg_envelope(parser)?;
            Some(envelope)
        }
    );

    None
}

fn parse_next_token(parser: &mut StringParser) -> Option<SfzToken> {
    try_parse!(parser, SfzToken::Group, SfzGroupType, parser, {
        parser.parse_literal("<")?;
        let group_name = parser.parse_regex(regex!(r"^\w+"))?;
        parser.parse_literal(">")?;
        match group_name.as_ref() {
            "region" => Some(SfzGroupType::Region),
            "group" => Some(SfzGroupType::Group),
            "master" => Some(SfzGroupType::Master),
            "control" => Some(SfzGroupType::Control),
            _ => Some(SfzGroupType::Other),
        }
    });

    try_parse!(parser, SfzToken::RegionFlag, SfzRegionFlags, parser, {
        let envelope = parse_region_flags(parser)?;
        Some(envelope)
    });

    None
}

fn parse_next_meta_token(parser: &mut StringParser) -> Option<SfzMetaToken> {
    try_parse!(parser, SfzMetaToken::InnerToken, SfzToken, parser, {
        let token = parse_next_token(parser)?;
        Some(token)
    });

    try_parse!(parser, SfzMetaToken::Import, String, parser, {
        parser.parse_literal("#include")?;
        parser.parse_regex(regex!("[ ]*"))?;
        parser.parse_literal("\"")?;
        let path = parser.parse_regex(regex!("[^\"]+"))?;
        parser.parse_literal("\"")?;
        Some(path.replace('\\', "/"))
    });

    let mut comment_parser = parser.clone();
    if comment_parser.parse_literal("//").is_some() {
        comment_parser.parse_until_line_end();
        *parser = comment_parser;
        return Some(SfzMetaToken::Comment);
    }

    None
}

pub fn parse_all_tokens(file_path: &Path) -> io::Result<Vec<SfzToken>> {
    let file_path = file_path.canonicalize()?;
    let file = fs::read_to_string(&file_path)?;

    // Unwrap here is safe because the path is confirmed to be a file (read above)
    // and therefore it will always have a parent folder. The path is also canonicalized.
    let parent_path = file_path.parent().unwrap();

    let mut parser = StringParser::new(&file);

    let mut tokens = Vec::new();

    while !parser.empty() {
        parser.trim_start();
        if let Some(next_token) = parse_next_meta_token(&mut parser) {
            match next_token {
                SfzMetaToken::InnerToken(token) => {
                    tokens.push(token);
                }
                SfzMetaToken::Import(path) => {
                    let full_path = parent_path.join(path);
                    let mut parsed_tokens = parse_all_tokens(&full_path)?;
                    tokens.append(&mut parsed_tokens);
                }
                SfzMetaToken::Comment => {}
            }
        } else {
            parser.parse_until_space();
        }
    }

    Ok(tokens)
}
