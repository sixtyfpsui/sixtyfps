/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

use crate::expression_tree::Expression;

/// Returns `0xaarrggbb`
pub fn parse_color_literal(str: &str) -> Option<u32> {
    if !str.starts_with('#') {
        return None;
    }
    if !str.is_ascii() {
        return None;
    }
    let str = &str[1..];
    let (r, g, b, a) = match str.len() {
        3 => (
            u8::from_str_radix(&str[0..=0], 16).ok()? * 0x11,
            u8::from_str_radix(&str[1..=1], 16).ok()? * 0x11,
            u8::from_str_radix(&str[2..=2], 16).ok()? * 0x11,
            255u8,
        ),
        4 => (
            u8::from_str_radix(&str[0..=0], 16).ok()? * 0x11,
            u8::from_str_radix(&str[1..=1], 16).ok()? * 0x11,
            u8::from_str_radix(&str[2..=2], 16).ok()? * 0x11,
            u8::from_str_radix(&str[3..=3], 16).ok()? * 0x11,
        ),
        6 => (
            u8::from_str_radix(&str[0..2], 16).ok()?,
            u8::from_str_radix(&str[2..4], 16).ok()?,
            u8::from_str_radix(&str[4..6], 16).ok()?,
            255u8,
        ),
        8 => (
            u8::from_str_radix(&str[0..2], 16).ok()?,
            u8::from_str_radix(&str[2..4], 16).ok()?,
            u8::from_str_radix(&str[4..6], 16).ok()?,
            u8::from_str_radix(&str[6..8], 16).ok()?,
        ),
        _ => return None,
    };
    Some((a as u32) << 24 | (r as u32) << 16 | (g as u32) << 8 | (b as u32))
}

#[test]
fn test_parse_color_literal() {
    assert_eq!(parse_color_literal("#abc"), Some(0xffaabbcc));
    assert_eq!(parse_color_literal("#ABC"), Some(0xffaabbcc));
    assert_eq!(parse_color_literal("#AbC"), Some(0xffaabbcc));
    assert_eq!(parse_color_literal("#AbCd"), Some(0xddaabbcc));
    assert_eq!(parse_color_literal("#01234567"), Some(0x67012345));
    assert_eq!(parse_color_literal("#012345"), Some(0xff012345));
    assert_eq!(parse_color_literal("_01234567"), None);
    assert_eq!(parse_color_literal("→↓←"), None);
    assert_eq!(parse_color_literal("#→↓←"), None);
    assert_eq!(parse_color_literal("#1234567890"), None);
}

pub fn unescape_string(string: &str) -> Option<String> {
    if string.contains('\n') {
        // FIXME: new line in string literal not yet supported
        return None;
    }
    let string = string.strip_prefix('"').or_else(|| string.strip_prefix('}'))?;
    let string = string.strip_suffix('"').or_else(|| string.strip_suffix("\\{"))?;
    if !string.contains('\\') {
        return Some(string.into());
    }
    let mut result = String::with_capacity(string.len());
    let mut pos = 0;
    loop {
        let stop = match string[pos..].find('\\') {
            Some(stop) => pos + stop,
            None => {
                result += &string[pos..];
                return Some(result);
            }
        };
        if stop + 1 >= string.len() {
            return None;
        }
        result += &string[pos..stop];
        pos = stop + 2;
        match string.as_bytes()[stop + 1] {
            b'"' => result += "\"",
            b'\\' => result += "\\",
            b'n' => result += "\n",
            b'u' => {
                if string.as_bytes().get(pos)? != &b'{' {
                    return None;
                }
                let end = string[pos..].find('}')? + pos;
                let x = u32::from_str_radix(&string[pos + 1..end], 16).ok()?;
                result.push(std::char::from_u32(x)?);
                pos = end + 1;
            }
            _ => return None,
        }
    }
}

#[test]
fn test_unescape_string() {
    assert_eq!(unescape_string(r#""foo_bar""#), Some("foo_bar".into()));
    assert_eq!(unescape_string(r#""foo\"bar""#), Some("foo\"bar".into()));
    assert_eq!(unescape_string(r#""foo\\\"bar""#), Some("foo\\\"bar".into()));
    assert_eq!(unescape_string(r#""fo\na\\r""#), Some("fo\na\\r".into()));
    assert_eq!(unescape_string(r#""fo\xa""#), None);
    assert_eq!(unescape_string(r#""fooo\""#), None);
    assert_eq!(unescape_string(r#""f\n\n\nf""#), Some("f\n\n\nf".into()));
    assert_eq!(unescape_string(r#""music\♪xx""#), None);
    assert_eq!(unescape_string(r#""music\"♪\"🎝""#), Some("music\"♪\"🎝".into()));
    assert_eq!(unescape_string(r#""foo_bar"#), None);
    assert_eq!(unescape_string(r#""foo_bar\"#), None);
    assert_eq!(unescape_string(r#"foo_bar""#), None);
    assert_eq!(unescape_string(r#""d\u{8}a\u{d4}f\u{Ed3}""#), Some("d\u{8}a\u{d4}f\u{ED3}".into()));
    assert_eq!(unescape_string(r#""xxx\""#), None);
    assert_eq!(unescape_string(r#""xxx\u""#), None);
    assert_eq!(unescape_string(r#""xxx\uxx""#), None);
    assert_eq!(unescape_string(r#""xxx\u{""#), None);
    assert_eq!(unescape_string(r#""xxx\u{22""#), None);
    assert_eq!(unescape_string(r#""xxx\u{qsdf}""#), None);
    assert_eq!(unescape_string(r#""xxx\u{1234567890}""#), None);
}

pub fn parse_number_literal(s: String) -> Result<Expression, String> {
    let bytes = s.as_bytes();
    let mut end = 0;
    while end < bytes.len() && matches!(bytes[end], b'0'..=b'9' | b'.') {
        end += 1;
    }
    let val = s[..end].parse().map_err(|_| "Cannot parse number literal".to_owned())?;
    let unit = s[end..].parse().map_err(|_| "Invalid unit".to_owned())?;
    Ok(Expression::NumberLiteral(val, unit))
}

#[test]
fn test_parse_number_literal() {
    use crate::expression_tree::Unit;

    fn doit(s: &str) -> Result<(f64, Unit), String> {
        parse_number_literal(s.into()).map(|e| match e {
            Expression::NumberLiteral(a, b) => (a, b),
            _ => panic!(),
        })
    }

    assert_eq!(doit("10"), Ok((10., Unit::None)));
    assert_eq!(doit("10phx"), Ok((10., Unit::Phx)));
    assert_eq!(doit("10.0phx"), Ok((10., Unit::Phx)));
    assert_eq!(doit("10.0"), Ok((10., Unit::None)));
    assert_eq!(doit("1.1phx"), Ok((1.1, Unit::Phx)));
    assert_eq!(doit("10.10"), Ok((10.10, Unit::None)));
    assert_eq!(doit("10000000"), Ok((10000000., Unit::None)));
    assert_eq!(doit("10000001phx"), Ok((10000001., Unit::Phx)));

    let wrong_unit = Err("Invalid unit".to_owned());
    let cannot_parse = Err("Cannot parse number literal".to_owned());
    assert_eq!(doit("10000001 phx"), wrong_unit);
    assert_eq!(doit("12.10.12phx"), cannot_parse);
    assert_eq!(doit("12.12oo"), wrong_unit);
    assert_eq!(doit("12.12€"), wrong_unit);
}
