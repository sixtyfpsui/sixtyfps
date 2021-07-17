/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

//! This module contains the code for the lexer.
//!
//! It is kind of shared with parser.rs, which implements the lex_next_token based on the macro_rules
//! that declares token

#[derive(Default)]
pub struct LexState {
    /// The top of the stack is the level of embedded braces `{`.
    /// So we must still lex so many '}' before re-entering into a string mode and pop the stack.
    template_string_stack: Vec<u32>,
}

/// This trait is used by the `crate::parser::lex_next_token` function and is implemented
/// for rule passed to the macro which can be either a string literal, or a function
pub trait LexingRule {
    /// Return the size of the match for this rule, or 0 if there is no match
    fn lex(&self, text: &str, state: &mut LexState) -> usize;
}

impl<'a> LexingRule for &'a str {
    #[inline]
    fn lex(&self, text: &str, _: &mut LexState) -> usize {
        if text.starts_with(*self) {
            self.len()
        } else {
            0
        }
    }
}

impl<F: Fn(&str, &mut LexState) -> usize> LexingRule for F {
    #[inline]
    fn lex(&self, text: &str, state: &mut LexState) -> usize {
        (self)(text, state)
    }
}

pub fn lex_whitespace(text: &str, _: &mut LexState) -> usize {
    let mut len = 0;
    let chars = text.chars();
    for c in chars {
        if !c.is_whitespace() {
            break;
        }
        len += c.len_utf8();
    }
    len
}

pub fn lex_comment(text: &str, _: &mut LexState) -> usize {
    // FIXME: could report proper error if not properly terminated
    if text.starts_with("//") {
        return text.find(&['\n', '\r'] as &[_]).unwrap_or_else(|| text.len());
    }
    if text.starts_with("/*") {
        let mut nested = 0;
        let mut offset = 2;
        let bytes = text.as_bytes();
        while offset < bytes.len() {
            if let Some(star) = bytes[offset..].iter().position(|c| *c == b'*') {
                let star = star + offset;
                if star > offset && bytes[star - 1] == b'/' {
                    nested += 1;
                    offset = star + 1;
                } else if bytes[star + 1] == b'/' {
                    if nested == 0 {
                        return star + 2;
                    }
                    nested -= 1;
                    offset = star + 2;
                } else {
                    offset = star + 1;
                }
            } else {
                // Unterminated
                return 0;
            }
        }
        // Unterminated
        return 0;
    }

    0
}

pub fn lex_string(text: &str, state: &mut LexState) -> usize {
    if let Some(brace_level) = state.template_string_stack.last_mut() {
        if text.starts_with('{') {
            *brace_level += 1;
            return 0;
        } else if text.starts_with('}') {
            if *brace_level > 0 {
                *brace_level -= 1;
                return 0;
            } else {
                state.template_string_stack.pop();
            }
        } else if !text.starts_with('"') {
            return 0;
        }
    } else if !text.starts_with('"') {
        return 0;
    }
    let mut end = 1; // skip the '"'
    loop {
        let stop = match text[end..].find(&['"', '\\'][..]) {
            Some(stop) => end + stop,
            // FIXME: report an error for unterminated string
            None => return 0,
        };
        match text.as_bytes()[stop] {
            b'"' => {
                return stop + 1;
            }
            b'\\' => {
                if text.as_bytes()[stop + 1] == b'{' {
                    state.template_string_stack.push(0);
                    return stop + 2;
                }
                end = stop + 2;
            }
            _ => unreachable!(),
        }
    }
}

pub fn lex_number(text: &str, _: &mut LexState) -> usize {
    let mut len = 0;
    let mut chars = text.chars();
    let mut had_period = false;
    while let Some(c) = chars.next() {
        if !c.is_ascii_digit() {
            if !had_period && c == '.' && len > 0 {
                had_period = true;
            } else {
                if len > 0 {
                    if c == '%' {
                        return len + 1;
                    }
                    if c.is_ascii_alphabetic() {
                        len += c.len_utf8();
                        // The unit
                        for c in chars {
                            if !c.is_ascii_alphabetic() {
                                return len;
                            }
                            len += c.len_utf8();
                        }
                    }
                }
                break;
            }
        }
        len += c.len_utf8();
    }
    len
}

pub fn lex_color(text: &str, _: &mut LexState) -> usize {
    if !text.starts_with('#') {
        return 0;
    }
    let mut len = 1;
    let chars = text[1..].chars();
    for c in chars {
        if !c.is_ascii_alphanumeric() {
            break;
        }
        len += c.len_utf8();
    }
    len
}

pub fn lex_identifier(text: &str, _: &mut LexState) -> usize {
    let mut len = 0;
    let chars = text.chars();
    for c in chars {
        if !c.is_alphanumeric() && c != '_' && (c != '-' || len == 0) {
            break;
        }
        len += c.len_utf8();
    }
    len
}

pub fn lex(mut source: &str) -> Vec<crate::parser::Token> {
    let mut result = vec![];
    let mut offset = 0;
    let mut state = LexState::default();
    while !source.is_empty() {
        if let Some((len, kind)) = crate::parser::lex_next_token(source, &mut state) {
            result.push(crate::parser::Token {
                kind,
                text: source[..len].into(),
                offset,
                ..Default::default()
            });
            offset += len;
            source = &source[len..];
        } else {
            // FIXME: recover
            result.push(crate::parser::Token {
                kind: crate::parser::SyntaxKind::Error,
                text: source.into(),
                offset,
                ..Default::default()
            });
            //offset += source.len();
            break;
        }
    }
    result
}

#[test]
fn basic_lexer_test() {
    fn compare(source: &str, expected: &[(crate::parser::SyntaxKind, &str)]) {
        let actual = lex(source);
        let actual =
            actual.iter().map(|token| (token.kind, token.text.as_str())).collect::<Vec<_>>();
        assert_eq!(actual.as_slice(), expected);
    }

    compare(
        r#"45  /*hi/*_*/ho*/ "string""#,
        &[
            (crate::parser::SyntaxKind::NumberLiteral, "45"),
            (crate::parser::SyntaxKind::Whitespace, "  "),
            (crate::parser::SyntaxKind::Comment, "/*hi/*_*/ho*/"),
            (crate::parser::SyntaxKind::Whitespace, " "),
            (crate::parser::SyntaxKind::StringLiteral, r#""string""#),
        ],
    );

    compare(
        r#"12px+5.2+=0.7%"#,
        &[
            (crate::parser::SyntaxKind::NumberLiteral, "12px"),
            (crate::parser::SyntaxKind::Plus, "+"),
            (crate::parser::SyntaxKind::NumberLiteral, "5.2"),
            (crate::parser::SyntaxKind::PlusEqual, "+="),
            (crate::parser::SyntaxKind::NumberLiteral, "0.7%"),
        ],
    );
    compare(
        r#"aa_a.b1,c"#,
        &[
            (crate::parser::SyntaxKind::Identifier, "aa_a"),
            (crate::parser::SyntaxKind::Dot, "."),
            (crate::parser::SyntaxKind::Identifier, "b1"),
            (crate::parser::SyntaxKind::Comma, ","),
            (crate::parser::SyntaxKind::Identifier, "c"),
        ],
    );
    compare(
        r#"/*/**/*//**/*"#,
        &[
            (crate::parser::SyntaxKind::Comment, "/*/**/*/"),
            (crate::parser::SyntaxKind::Comment, "/**/"),
            (crate::parser::SyntaxKind::Star, "*"),
        ],
    );
    compare(
        "a//x\nb//y\r\nc//z",
        &[
            (crate::parser::SyntaxKind::Identifier, "a"),
            (crate::parser::SyntaxKind::Comment, "//x"),
            (crate::parser::SyntaxKind::Whitespace, "\n"),
            (crate::parser::SyntaxKind::Identifier, "b"),
            (crate::parser::SyntaxKind::Comment, "//y"),
            (crate::parser::SyntaxKind::Whitespace, "\r\n"),
            (crate::parser::SyntaxKind::Identifier, "c"),
            (crate::parser::SyntaxKind::Comment, "//z"),
        ],
    );
    compare(r#""x""#, &[(crate::parser::SyntaxKind::StringLiteral, r#""x""#)]);
    compare(
        r#"a"\"\\"x"#,
        &[
            (crate::parser::SyntaxKind::Identifier, "a"),
            (crate::parser::SyntaxKind::StringLiteral, r#""\"\\""#),
            (crate::parser::SyntaxKind::Identifier, "x"),
        ],
    );
    compare(
        r#""a\{b{c}d"e\{f}g"h}i"j"#,
        &[
            (crate::parser::SyntaxKind::StringLiteral, r#""a\{"#),
            (crate::parser::SyntaxKind::Identifier, "b"),
            (crate::parser::SyntaxKind::LBrace, "{"),
            (crate::parser::SyntaxKind::Identifier, "c"),
            (crate::parser::SyntaxKind::RBrace, "}"),
            (crate::parser::SyntaxKind::Identifier, "d"),
            (crate::parser::SyntaxKind::StringLiteral, r#""e\{"#),
            (crate::parser::SyntaxKind::Identifier, "f"),
            (crate::parser::SyntaxKind::StringLiteral, r#"}g""#),
            (crate::parser::SyntaxKind::Identifier, "h"),
            (crate::parser::SyntaxKind::StringLiteral, r#"}i""#),
            (crate::parser::SyntaxKind::Identifier, "j"),
        ],
    );
}
