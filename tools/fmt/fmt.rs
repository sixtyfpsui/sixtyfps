/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

use crate::TokenWriter;
use sixtyfps_compilerlib::parser::{syntax_nodes, NodeOrToken, SyntaxKind, SyntaxNode};

pub(crate) fn format_document(
    doc: syntax_nodes::Document,
    writer: &mut impl TokenWriter,
) -> Result<(), std::io::Error> {
    let mut state = FormatState::default();
    format_node(&doc, writer, &mut state)
}

#[derive(Default)]
struct FormatState {
    /// The whitespace have been written, all further whitespace can be skipped
    skip_all_whitespace: bool,
    /// The whitespace to add before the next token
    whitespace_to_add: Option<String>,
    /// The level of indentation
    indentation_level: u32,

    /// A counter that is incremented when something is inserted
    insertion_count: usize,

    /// a comment has been written followed maybe by some spacing
    after_comment: bool,
}

impl FormatState {
    fn new_line(&mut self) {
        if self.after_comment {
            return;
        }
        self.skip_all_whitespace = true;
        if let Some(x) = &mut self.whitespace_to_add {
            x.insert(0, '\n');
            return;
        }
        let mut new_line = String::from("\n");
        for _ in 0..self.indentation_level {
            new_line += "    ";
        }
        self.whitespace_to_add = Some(new_line);
    }

    fn insert_whitespace(&mut self, arg: &str) {
        if self.after_comment {
            return;
        }
        self.skip_all_whitespace = true;
        if !arg.is_empty() {
            if let Some(ws) = &mut self.whitespace_to_add {
                *ws += arg;
            } else {
                self.whitespace_to_add = Some(arg.into());
            }
        }
    }
}

fn format_node(
    node: &SyntaxNode,
    writer: &mut impl TokenWriter,
    state: &mut FormatState,
) -> Result<(), std::io::Error> {
    match node.kind() {
        SyntaxKind::Component => {
            return format_component(node, writer, state);
        }
        SyntaxKind::Element => {
            return format_element(node, writer, state);
        }
        SyntaxKind::PropertyDeclaration => {
            return format_property_declaration(node, writer, state);
        }
        SyntaxKind::Binding => {
            return format_binding(node, writer, state);
        }
        SyntaxKind::CallbackConnection => {
            return format_callback_connection(node, writer, state);
        }
        SyntaxKind::CallbackDeclaration => {
            return format_callback_declaration(node, writer, state);
        }
        SyntaxKind::QualifiedName => {
            return format_qualified_name(node, writer, state);
        }

        _ => (),
    }

    for n in node.children_with_tokens() {
        fold(n, writer, state)?;
    }
    Ok(())
}

fn fold(
    n: NodeOrToken,
    writer: &mut impl TokenWriter,
    state: &mut FormatState,
) -> std::io::Result<()> {
    match n {
        NodeOrToken::Node(n) => format_node(&n, writer, state),
        NodeOrToken::Token(t) => {
            if t.kind() == SyntaxKind::Whitespace {
                if state.skip_all_whitespace {
                    writer.with_new_content(t, "")?;
                    return Ok(());
                }
            } else {
                state.after_comment = t.kind() == SyntaxKind::Comment;
                state.skip_all_whitespace = false;
                if let Some(x) = state.whitespace_to_add.take() {
                    state.insertion_count += 1;
                    writer.insert_before(t, x.as_ref())?;
                    return Ok(());
                }
            }
            state.insertion_count += 1;
            writer.no_change(t)
        }
    }
}

fn whitespace_to(
    sub: &mut impl Iterator<Item = NodeOrToken>,
    element: SyntaxKind,
    writer: &mut impl TokenWriter,
    state: &mut FormatState,
    arg: &str,
) -> Result<bool, std::io::Error> {
    state.insert_whitespace(arg);
    for n in sub {
        match n.kind() {
            SyntaxKind::Whitespace | SyntaxKind::Comment => (),
            x if x == element => {
                fold(n, writer, state)?;
                return Ok(true);
            }
            _ => {
                eprintln!("Inconsistency: expected {:?},  found {:?}", element, n);
                fold(n, writer, state)?;
                return Ok(false);
            }
        }
        fold(n, writer, state)?;
    }
    eprintln!("Inconsistency: expected {:?},  not found", element);
    Ok(false)
}

fn finish_node(
    sub: impl Iterator<Item = NodeOrToken>,
    writer: &mut impl TokenWriter,
    state: &mut FormatState,
) -> Result<bool, std::io::Error> {
    // FIXME:  We should check that there are only comments or whitespace in sub
    for n in sub {
        fold(n, writer, state)?;
    }
    Ok(true)
}

fn format_component(
    node: &SyntaxNode,
    writer: &mut impl TokenWriter,
    state: &mut FormatState,
) -> Result<(), std::io::Error> {
    //let mut sub = node.first_child_or_token();
    let mut sub = node.children_with_tokens();
    let _ok = whitespace_to(&mut sub, SyntaxKind::DeclaredIdentifier, writer, state, "")?
        && whitespace_to(&mut sub, SyntaxKind::ColonEqual, writer, state, " ")?
        && whitespace_to(&mut sub, SyntaxKind::Element, writer, state, " ")?;

    finish_node(sub, writer, state)?;
    state.new_line();

    Ok(())
}

fn format_element(
    node: &SyntaxNode,
    writer: &mut impl TokenWriter,
    state: &mut FormatState,
) -> Result<(), std::io::Error> {
    let mut sub = node.children_with_tokens();
    if !(whitespace_to(&mut sub, SyntaxKind::QualifiedName, writer, state, "")?
        // note: QualifiedName is already adding one space
        && whitespace_to(&mut sub, SyntaxKind::LBrace, writer, state, "")?)
    {
        finish_node(sub, writer, state)?;
        return Ok(());
    }

    state.indentation_level += 1;
    state.new_line();
    let ins_ctn = state.insertion_count;

    for n in sub {
        if n.kind() == SyntaxKind::RBrace {
            state.indentation_level -= 1;
            state.whitespace_to_add = None;
            if ins_ctn == state.insertion_count {
                state.insert_whitespace(" ");
            } else {
                state.new_line();
            }
            fold(n, writer, state)?;
            state.new_line();
        } else {
            fold(n, writer, state)?;
        }
    }
    Ok(())
}

fn format_property_declaration(
    node: &SyntaxNode,
    writer: &mut impl TokenWriter,
    state: &mut FormatState,
) -> Result<(), std::io::Error> {
    let mut sub = node.children_with_tokens();
    let _ok = whitespace_to(&mut sub, SyntaxKind::Identifier, writer, state, "")?
        && whitespace_to(&mut sub, SyntaxKind::LAngle, writer, state, " ")?
        && whitespace_to(&mut sub, SyntaxKind::Type, writer, state, "")?
        && whitespace_to(&mut sub, SyntaxKind::RAngle, writer, state, "")?
        && whitespace_to(&mut sub, SyntaxKind::DeclaredIdentifier, writer, state, " ")?;

    state.skip_all_whitespace = true;
    // FIXME: more formatting
    for s in sub {
        fold(s, writer, state)?;
    }
    state.new_line();
    Ok(())
}

fn format_binding(
    node: &SyntaxNode,
    writer: &mut impl TokenWriter,
    state: &mut FormatState,
) -> Result<(), std::io::Error> {
    let mut sub = node.children_with_tokens();
    let _ok = whitespace_to(&mut sub, SyntaxKind::Identifier, writer, state, "")?
        && whitespace_to(&mut sub, SyntaxKind::Colon, writer, state, "")?
        && whitespace_to(&mut sub, SyntaxKind::BindingExpression, writer, state, " ")?;
    // FIXME: more formatting
    for s in sub {
        fold(s, writer, state)?;
    }
    state.new_line();
    Ok(())
}

fn format_callback_declaration(
    node: &SyntaxNode,
    writer: &mut impl TokenWriter,
    state: &mut FormatState,
) -> Result<(), std::io::Error> {
    let mut sub = node.children_with_tokens();
    let _ok = whitespace_to(&mut sub, SyntaxKind::Identifier, writer, state, "")?
        && whitespace_to(&mut sub, SyntaxKind::DeclaredIdentifier, writer, state, " ")?;

    while let Some(n) = sub.next() {
        state.skip_all_whitespace = true;
        match n.kind() {
            // SyntaxKind::LParent => {
            //     fold(n, writer, state)?;
            //     state.skip_all_whitespace = true;
            // }
            SyntaxKind::Comma => {
                fold(n, writer, state)?;
                state.insert_whitespace(" ");
            }
            SyntaxKind::Arrow => {
                state.insert_whitespace(" ");
                fold(n, writer, state)?;
                whitespace_to(&mut sub, SyntaxKind::ReturnType, writer, state, " ")?;
            }
            _ => {
                fold(n, writer, state)?;
            }
        }
    }
    state.new_line();
    Ok(())
}

fn format_callback_connection(
    node: &SyntaxNode,
    writer: &mut impl TokenWriter,
    state: &mut FormatState,
) -> Result<(), std::io::Error> {
    let mut sub = node.children_with_tokens();
    let _ok = whitespace_to(&mut sub, SyntaxKind::Identifier, writer, state, "")?;

    for s in sub {
        state.skip_all_whitespace = true;
        match s.kind() {
            SyntaxKind::FatArrow => {
                state.insert_whitespace(" ");
                fold(s, writer, state)?;
                state.insert_whitespace(" ");
            }
            SyntaxKind::Comma => {
                fold(s, writer, state)?;
                state.insert_whitespace(" ");
            }
            _ => fold(s, writer, state)?,
        }
    }
    state.new_line();
    Ok(())
}

fn format_qualified_name(
    node: &SyntaxNode,
    writer: &mut impl TokenWriter,
    state: &mut FormatState,
) -> Result<(), std::io::Error> {
    for n in node.children_with_tokens() {
        state.skip_all_whitespace = true;
        fold(n, writer, state)?;
    }
    /*if !node
        .last_token()
        .and_then(|x| x.next_token())
        .map(|x| {
            matches!(
                x.kind(),
                SyntaxKind::LParent
                    | SyntaxKind::RParent
                    | SyntaxKind::Semicolon
                    | SyntaxKind::Comma
            )
        })
        .unwrap_or(false)
    {
        state.insert_whitespace(" ");
    } else {
        state.skip_all_whitespace = true;
    }*/
    Ok(())
}
