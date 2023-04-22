use simdeez::prelude::*;

use regex_bnf::{ParseError, StringParser, TextSlice};

simd_runtime_generate! {
    pub fn parse_opcode_name_simd<'a>(input: StringParser<'a>) -> Result<(TextSlice<'a>, StringParser<'a>), ParseError> {
        let mut length = 0;

        // Manually implement parsing "[\w$]+"

        let char_start = S::Vi8::set1('a' as i8);
        let char_end = S::Vi8::set1('z' as i8);
        let char_upper_start = S::Vi8::set1('A' as i8);
        let char_upper_end = S::Vi8::set1('Z' as i8);
        let char_num_start = S::Vi8::set1('0' as i8);
        let char_num_end = S::Vi8::set1('9' as i8);

        let char_dollar = S::Vi8::set1('$' as i8);
        let char_underscore = S::Vi8::set1('_' as i8);

        let mut input_str = input.text;
        loop {
            // Make simd sample from input
            let bytes = input_str.as_bytes();
            let bytes_i8 = unsafe { std::mem::transmute::<&[u8], &[i8]>(bytes) };
            let simd_chars = S::Vi8::load_from_slice(bytes_i8);

            // Check if the character is in the range of a-z
            let char_is_lower = simd_chars.cmp_gte(char_start) & simd_chars.cmp_lte(char_end);

            // Check if the character is in the range of A-Z
            let char_is_upper = simd_chars.cmp_gte(char_upper_start) & simd_chars.cmp_lte(char_upper_end);

            // Check if the character is a number
            let char_is_num = simd_chars.cmp_gte(char_num_start) & simd_chars.cmp_lte(char_num_end);

            // Check if the character is a dollar sign
            let char_is_dollar = simd_chars.cmp_eq(char_dollar);

            // Check if the character is an underscore
            let char_is_underscore = simd_chars.cmp_eq(char_underscore);

            let valid_chars = char_is_lower | char_is_upper | char_is_num | char_is_dollar | char_is_underscore;

            if let Some(end) = valid_chars.index_of_first_falsy() {
                length += end;
                break;
            } else if input_str.len() < S::Vi8::WIDTH  {
                length += input_str.len();
                break;
            } else{
                length += S::Vi8::WIDTH;
                input_str = &input_str[S::Vi8::WIDTH..];
            }
        }

        if length > 0 {
            Ok(input.split_at(length))
        } else {
            Err(ParseError::new("Couldn't find opcode name", input.pos))
        }
    }
}

simd_runtime_generate! {
    pub fn parse_opcode_value_simd<'a>(input: StringParser<'a>) -> Result<(TextSlice<'a>, StringParser<'a>), ParseError> {
        let mut length = 0;

        // Manually implement parsing "[^ \r\n]+[ ]*"

        let char_space = S::Vi8::set1(' ' as i8);
        let char_newline = S::Vi8::set1('\n' as i8);
        let char_return = S::Vi8::set1('\r' as i8);

        let mut input_str = input.text;
        loop {
            // Make simd sample from input
            let bytes = input_str.as_bytes();
            let bytes_i8 = unsafe { std::mem::transmute::<&[u8], &[i8]>(bytes) };
            let simd_chars = S::Vi8::load_from_slice(bytes_i8);

            // Check if the character is a space
            let char_is_not_space = simd_chars.cmp_neq(char_space);

            // Check if the character is a newline
            let char_is_not_newline = simd_chars.cmp_neq(char_newline);

            // Check if the character is a return
            let char_is_not_return = simd_chars.cmp_neq(char_return);

            let valid_chars = char_is_not_space & char_is_not_newline & char_is_not_return;

            if let Some(end) = valid_chars.index_of_first_falsy() {
                length += end;
                input_str = &input_str[end..];
                break;
            } else if input_str.len() < S::Vi8::WIDTH  {
                length += input_str.len();
                break;
            } else{
                length += S::Vi8::WIDTH;
                input_str = &input_str[S::Vi8::WIDTH..];
            }
        }

        // Now manually parse the remaining spaces without simd
        // There's usually only 1 space, so it's not worth it to simd this
        loop {
            if input_str.starts_with(' ') {
                length += 1;
                input_str = &input_str[1..];
            } else {
                break;
            }
        }

        if length > 0 {
            Ok(input.split_at(length))
        } else {
            Err(ParseError::new("Couldn't find opcode name", input.pos))
        }
    }
}
