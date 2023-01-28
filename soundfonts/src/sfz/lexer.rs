use std::{fs, io, path::Path};

use crate::FilterType;

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
        let space_regex = regex!(r#"([^\s]+)"#);
        let next_space = space_regex
            .find(self.input)
            .map(|v| v.end())
            .unwrap_or(self.input.len());
        let result = &self.input[..next_space];
        let remaining = &self.input[next_space..];
        self.input = remaining;
        result.trim().to_owned()
    }

    fn trim_start(&mut self) {
        self.input = self.input.trim_start();
    }

    fn empty(&self) -> bool {
        self.input.is_empty()
    }
}

macro_rules! parse {
    ($parser:expr, $parse:expr) => {
        let old_parser = $parser.clone();
        if let Some(token) = $parse() {
            return Some(token);
        }
        *$parser = old_parser;
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SfzGroupType {
    Region,
    Group,
    Global,
    Control,
    Other,
}

fn parse_equals(parser: &mut StringParser) -> Option<()> {
    parser.parse_regex(regex!(r"[ ]*=[ ]*"))?;
    Some(())
}

fn parse_basic_tag_name(parser: &mut StringParser, tag_name: &str) -> Option<()> {
    parser.parse_literal(tag_name)?;
    parse_equals(parser);
    Some(())
}

fn parse_vel_number(parser: &mut StringParser<'_>) -> Option<u8> {
    let num = parser.parse_regex(regex!(r"\d+"))?;
    num.parse().ok()
}

fn parse_key_number(parser: &mut StringParser<'_>) -> Option<u8> {
    let parsed = parser.parse_regex(regex!(r"\d+"))?;
    match parsed.parse().ok() {
        Some(val) => Some(val),
        None => {
            let note: String = parsed
                .chars()
                .filter(|c| !(c.is_ascii_digit() || c == &'-'))
                .collect();
            let semitone: i8 = match note.to_lowercase().as_str() {
                "c" => 0,
                "c#" => 1,
                "db" => 1,
                "d" => 2,
                "d#" => 3,
                "eb" => 3,
                "e" => 4,
                "f" => 5,
                "f#" => 6,
                "gb" => 6,
                "g" => 7,
                "g#" => 8,
                "ab" => 8,
                "a" => 9,
                "a#" => 10,
                "bb" => 10,
                "b" => 11,
                _ => return None,
            };
            let octave: String = parsed
                .chars()
                .filter(|c| c.is_ascii_digit() || c == &'-')
                .collect();
            let octave: i8 = octave.parse().ok().unwrap_or(-10);
            if octave < -1 {
                None
            } else {
                let midi_note = 12 + semitone + octave * 12;
                Some(midi_note as u8)
            }
        }
    }
}

fn parse_pan_number(parser: &mut StringParser<'_>) -> Option<i8> {
    let num = parser.parse_regex(regex!(r"[\-\d]+"))?;
    num.parse().ok()
}

fn parse_i16(parser: &mut StringParser<'_>) -> Option<i16> {
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
    FilVeltrack(i16),
    FilKeycenter(u8),
    FilKeytrack(i16),
    FilterType(FilterType),
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

fn parse_float_tag<T, F: Fn(f32) -> T>(
    parser: &mut StringParser,
    wrap: F,
    tag_name: &str,
) -> Option<T> {
    parse_basic_tag(parser, wrap, tag_name, parse_float)
}

fn parse_basic_tag<V, T, F: Fn(V) -> T>(
    parser: &mut StringParser,
    wrap: F,
    tag_name: &str,
    parse_value: impl Fn(&mut StringParser) -> Option<V>,
) -> Option<T> {
    parse!(parser, || {
        parse_basic_tag_name(parser, tag_name)?;
        let value = parse_value(parser)?;
        Some(wrap(value))
    });

    None
}

fn parse_ampeg_envelope(parser: &mut StringParser) -> Option<SfzAmpegEnvelope> {
    parse!(parser, || parse_float_tag(
        parser,
        SfzAmpegEnvelope::AmpegStart,
        "ampeg_start"
    ));
    parse!(parser, || parse_float_tag(
        parser,
        SfzAmpegEnvelope::AmpegDelay,
        "ampeg_delay"
    ));
    parse!(parser, || parse_float_tag(
        parser,
        SfzAmpegEnvelope::AmpegAttack,
        "ampeg_attack"
    ));
    parse!(parser, || parse_float_tag(
        parser,
        SfzAmpegEnvelope::AmpegHold,
        "ampeg_hold"
    ));
    parse!(parser, || parse_float_tag(
        parser,
        SfzAmpegEnvelope::AmpegDecay,
        "ampeg_decay"
    ));
    parse!(parser, || parse_float_tag(
        parser,
        SfzAmpegEnvelope::AmpegSustain,
        "ampeg_sustain"
    ));
    parse!(parser, || parse_float_tag(
        parser,
        SfzAmpegEnvelope::AmpegRelease,
        "ampeg_release"
    ));

    None
}

fn parse_region_flags(parser: &mut StringParser) -> Option<SfzRegionFlags> {
    parse!(parser, || {
        parse_basic_tag_name(parser, "sample")?;
        Some(SfzRegionFlags::Sample(
            parser.parse_until_space().replace('\\', "/"),
        ))
    });

    parse!(parser, || parse_basic_tag(
        parser,
        SfzRegionFlags::Lovel,
        "lovel",
        parse_vel_number
    ));
    parse!(parser, || parse_basic_tag(
        parser,
        SfzRegionFlags::Hivel,
        "hivel",
        parse_vel_number
    ));
    parse!(parser, || parse_basic_tag(
        parser,
        SfzRegionFlags::Lokey,
        "lokey",
        parse_key_number
    ));
    parse!(parser, || parse_basic_tag(
        parser,
        SfzRegionFlags::Hikey,
        "hikey",
        parse_key_number
    ));
    parse!(parser, || parse_basic_tag(
        parser,
        SfzRegionFlags::Pan,
        "pan",
        parse_pan_number
    ));
    parse!(parser, || parse_basic_tag(
        parser,
        SfzRegionFlags::PitchKeycenter,
        "pitch_keycenter",
        parse_key_number
    ));
    parse!(parser, || parse_basic_tag(
        parser,
        SfzRegionFlags::Key,
        "key",
        parse_key_number
    ));
    parse!(parser, || parse_basic_tag(
        parser,
        SfzRegionFlags::Cutoff,
        "cutoff",
        parse_float
    ));
    parse!(parser, || parse_basic_tag(
        parser,
        SfzRegionFlags::FilVeltrack,
        "fil_veltrack",
        parse_i16
    ));
    parse!(parser, || parse_basic_tag(
        parser,
        SfzRegionFlags::FilKeytrack,
        "fil_keytrack",
        parse_i16
    ));
    parse!(parser, || parse_basic_tag(
        parser,
        SfzRegionFlags::FilKeycenter,
        "fil_keycenter",
        parse_key_number
    ));

    parse!(parser, || {
        parse_basic_tag_name(parser, "fil_type")?;
        let group_name = parser.parse_regex(regex!(r"^\w+"))?;
        let fil_type = match group_name.as_ref() {
            "lpf_1p" => FilterType::LowPassPole,
            "lpf_2p" => FilterType::LowPass,
            "lpf_4p" => FilterType::LowPass,
            "lpf_6p" => FilterType::LowPass,
            "hpf_1p" => FilterType::HighPass,
            "hpf_2p" => FilterType::HighPass,
            "hpf_4p" => FilterType::HighPass,
            "hpf_6p" => FilterType::HighPass,
            "bpf_1p" => FilterType::BandPass,
            "bpf_2p" => FilterType::BandPass,
            _ => FilterType::LowPass,
        };
        Some(SfzRegionFlags::FilterType(fil_type))
    });

    parse!(parser, || {
        parse_basic_tag_name(parser, "loop_mode")?;
        let group_name = parser.parse_regex(regex!(r"^\w+"))?;
        let mode = match group_name.as_ref() {
            "no_loop" => SfzLoopMode::NoLoop,
            "one_shot" => SfzLoopMode::OneShot,
            "loop_continuous" => SfzLoopMode::LoopContinuous,
            "loop_sustain" => SfzLoopMode::LoopSustain,
            _ => SfzLoopMode::Other,
        };
        Some(SfzRegionFlags::LoopMode(mode))
    });

    parse!(parser, || {
        parse_basic_tag_name(parser, "default_path")?;
        Some(SfzRegionFlags::DefaultPath(
            parser.parse_until_space().replace('\\', "/"),
        ))
    });

    parse!(parser, || {
        let envelope = parse_ampeg_envelope(parser)?;
        Some(SfzRegionFlags::AmpegEnvelope(envelope))
    });

    None
}

fn parse_next_token(parser: &mut StringParser) -> Option<SfzToken> {
    parse!(parser, || {
        parser.parse_literal("<")?;
        let group_name = parser.parse_regex(regex!(r"^\w+"))?;
        parser.parse_literal(">")?;
        let group = match group_name.as_ref() {
            "region" => SfzGroupType::Region,
            "group" => SfzGroupType::Group,
            "master" => SfzGroupType::Global,
            "control" => SfzGroupType::Control,
            "global" => SfzGroupType::Global,
            _ => SfzGroupType::Other,
        };
        Some(SfzToken::Group(group))
    });

    parse!(parser, || {
        let envelope = parse_region_flags(parser)?;
        Some(SfzToken::RegionFlag(envelope))
    });

    None
}

fn parse_next_meta_token(parser: &mut StringParser) -> Option<SfzMetaToken> {
    parse!(parser, || {
        let token = parse_next_token(parser)?;
        Some(SfzMetaToken::InnerToken(token))
    });

    parse!(parser, || {
        parser.parse_literal("#include")?;
        parser.parse_regex(regex!("[ ]*"))?;
        parser.parse_literal("\"")?;
        let path = parser.parse_regex(regex!("[^\"]+"))?;
        parser.parse_literal("\"")?;
        Some(SfzMetaToken::Import(path.replace('\\', "/")))
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
