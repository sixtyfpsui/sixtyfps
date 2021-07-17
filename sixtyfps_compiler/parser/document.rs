/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */

use super::element::{parse_element, parse_element_content};
use super::prelude::*;
use super::r#type::parse_struct_declaration;

#[cfg_attr(test, parser_test)]
/// ```test,Document
/// Type := Base { }
/// Type := Base { SubElement { } }
/// Comp := Base {}  Type := Base {}
/// Type := Base {} export { Type }
/// import { Base } from "somewhere"; Type := Base {}
/// struct Foo := { foo: foo }
/// ```
pub fn parse_document(p: &mut impl Parser) -> bool {
    let mut p = p.start_node(SyntaxKind::Document);

    loop {
        match p.peek().as_str() {
            "export" => {
                if !parse_export(&mut *p) {
                    return false;
                }
            }
            "import" => {
                if !parse_import_specifier(&mut *p) {
                    return false;
                }
            }
            "struct" => {
                if !parse_struct_declaration(&mut *p) {
                    return false;
                }
            }
            _ => {
                if !parse_component(&mut *p) {
                    return false;
                }
            }
        }

        if p.nth(0).kind() == SyntaxKind::Eof {
            return true;
        }
    }
}

#[cfg_attr(test, parser_test)]
/// ```test,Component
/// Type := Base { }
/// Type := Base { prop: value; }
/// Type := Base { SubElement { } }
/// global Struct := { property<int> xx; }
/// ```
pub fn parse_component(p: &mut impl Parser) -> bool {
    let mut p = p.start_node(SyntaxKind::Component);
    let is_global = p.peek().as_str() == "global";
    if is_global {
        p.consume();
    }
    if !p.start_node(SyntaxKind::DeclaredIdentifier).expect(SyntaxKind::Identifier) {
        drop(p.start_node(SyntaxKind::Element));
        return false;
    }
    if !p.expect(SyntaxKind::ColonEqual) {
        drop(p.start_node(SyntaxKind::Element));
        return false;
    }

    if is_global && p.peek().kind() == SyntaxKind::LBrace {
        let mut p = p.start_node(SyntaxKind::Element);
        p.consume();
        parse_element_content(&mut *p);
        return p.expect(SyntaxKind::RBrace);
    }

    parse_element(&mut *p)
}

#[cfg_attr(test, parser_test)]
/// ```test,QualifiedName
/// Rectangle
/// MyModule.Rectangle
/// Deeply.Nested.MyModule.Rectangle
/// ```
pub fn parse_qualified_name(p: &mut impl Parser) -> bool {
    let mut p = p.start_node(SyntaxKind::QualifiedName);
    if !p.expect(SyntaxKind::Identifier) {
        return false;
    }

    loop {
        if p.nth(0).kind() != SyntaxKind::Dot {
            break;
        }
        p.consume();
        p.expect(SyntaxKind::Identifier);
    }

    true
}

#[cfg_attr(test, parser_test)]
/// ```test,ExportsList
/// export { Type }
/// export { Type, AnotherType }
/// export { Type as Foo, AnotherType }
/// export Foo := Item { }
/// export struct Foo := { foo: bar }
/// ```
fn parse_export(p: &mut impl Parser) -> bool {
    debug_assert_eq!(p.peek().as_str(), "export");
    let mut p = p.start_node(SyntaxKind::ExportsList);
    p.consume(); // "export"
    if p.test(SyntaxKind::LBrace) {
        loop {
            parse_export_specifier(&mut *p);
            match p.nth(0).kind() {
                SyntaxKind::RBrace => {
                    p.consume();
                    return true;
                }
                SyntaxKind::Eof => {
                    p.error("Expected comma");
                    return false;
                }
                SyntaxKind::Comma => {
                    p.consume();
                }
                _ => {
                    p.consume();
                    p.error("Expected comma")
                }
            }
        }
    } else if p.peek().as_str() == "struct" {
        parse_struct_declaration(&mut *p)
    } else {
        parse_component(&mut *p)
    }
}

#[cfg_attr(test, parser_test)]
/// ```test,ExportSpecifier
/// Type
/// Type as Something
/// ```
fn parse_export_specifier(p: &mut impl Parser) -> bool {
    let mut p = p.start_node(SyntaxKind::ExportSpecifier);
    {
        let mut p = p.start_node(SyntaxKind::ExportIdentifier);
        if !p.expect(SyntaxKind::Identifier) {
            return false;
        }
    }
    if p.peek().as_str() == "as" {
        p.consume();
        let mut p = p.start_node(SyntaxKind::ExportName);
        if !p.expect(SyntaxKind::Identifier) {
            return false;
        }
    }

    true
}

#[cfg_attr(test, parser_test)]
/// ```test,ImportSpecifier
/// import { Type1, Type2 } from "somewhere";
/// import "something.ttf";
/// ```
fn parse_import_specifier(p: &mut impl Parser) -> bool {
    debug_assert_eq!(p.peek().as_str(), "import");
    let mut p = p.start_node(SyntaxKind::ImportSpecifier);
    p.consume(); // "import"
    if p.peek().kind != SyntaxKind::StringLiteral {
        if !parse_import_identifier_list(&mut *p) {
            return false;
        }
        if p.peek().as_str() != "from" {
            p.error("Expected from keyword for import statement");
            return false;
        }
        if !p.expect(SyntaxKind::Identifier) {
            return false;
        }
    }
    let peek = p.peek();
    if peek.kind != SyntaxKind::StringLiteral
        || !peek.as_str().starts_with('"')
        || !peek.as_str().ends_with('"')
    {
        p.error("Expected plain string literal");
        return false;
    }
    p.consume();
    p.expect(SyntaxKind::Semicolon)
}

#[cfg_attr(test, parser_test)]
/// ```test,ImportIdentifierList
/// { Type1 }
/// { Type2, Type3 }
/// { }
/// { Type as Alias1, Type as AnotherAlias }
/// ```
fn parse_import_identifier_list(p: &mut impl Parser) -> bool {
    let mut p = p.start_node(SyntaxKind::ImportIdentifierList);
    if !p.expect(SyntaxKind::LBrace) {
        return false;
    }
    if p.test(SyntaxKind::RBrace) {
        return true;
    }
    loop {
        parse_import_identifier(&mut *p);
        match p.nth(0).kind() {
            SyntaxKind::RBrace => {
                p.consume();
                return true;
            }
            SyntaxKind::Eof => return false,
            SyntaxKind::Comma => {
                p.consume();
            }
            _ => {
                p.consume();
                p.error("Expected comma")
            }
        }
    }
}

#[cfg_attr(test, parser_test)]
/// ```test,ImportIdentifier
/// Type
/// Type as Alias1
/// ```
fn parse_import_identifier(p: &mut impl Parser) -> bool {
    let mut p = p.start_node(SyntaxKind::ImportIdentifier);
    {
        let mut p = p.start_node(SyntaxKind::ExternalName);
        if !p.expect(SyntaxKind::Identifier) {
            return false;
        }
    }
    if p.nth(0).kind() == SyntaxKind::Identifier && p.peek().as_str() == "as" {
        p.consume();
        let mut p = p.start_node(SyntaxKind::InternalName);
        if !p.expect(SyntaxKind::Identifier) {
            return false;
        }
    }
    true
}
