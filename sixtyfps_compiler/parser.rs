/* This file is part of the SixtyFPS Project -- https://sixtyfps.io
    Copyright (c) 2021 Olivier Goffart <olivier.goffart@sixtyfps.io>
    Copyright (c) 2021 Simon Hausmann <simon.hausmann@sixtyfps.io>

LICENSE BEGIN
    SPDX-License-Identifier: GPL-3.0-only
    This file is also available under commercial licensing terms.
    Please contact info@sixtyfps.io for more information.
LICENSE END */
/*! The sixtyfps language parser

This module is responsible to parse a string onto a syntax tree.

The core of it is the `DefaultParser` class that holds a list of token and
generates a `rowan::GreenNode`

This module has different sub modules with the actual parser functions

*/

use crate::diagnostics::{BuildDiagnostics, SourceFile, Spanned};
pub use smol_str::SmolStr;
use std::convert::TryFrom;

mod document;
mod element;
mod expressions;
mod statements;
mod r#type;

/// Each parser submodule would simply do `use super::prelude::*` to import typically used items
mod prelude {
    #[cfg(test)]
    pub use super::{syntax_nodes, SyntaxNode, SyntaxNodeVerify};
    pub use super::{DefaultParser, Parser, SyntaxKind};
    #[cfg(test)]
    pub use parser_test_macro::parser_test;
}

#[cfg(test)]
pub trait SyntaxNodeVerify {
    /// The SyntaxKind corresponding to this type
    const KIND: SyntaxKind;
    /// Asserts that the node is of the given SyntaxKind and that it has the expected children
    /// Panic if this is not the case
    fn verify(node: SyntaxNode) {
        assert_eq!(node.kind(), Self::KIND)
    }
}

/// Check that a node has the assumed children
#[cfg(test)]
macro_rules! verify_node {
    // nothing to verify
    ($node:ident, _) => {};
    // Some combination of children
    ($node:ident, [ $($t1:tt $($t2:ident)?),* ]) => {
        // Check that every children is there
        $(verify_node!(@check_has_children $node, $t1 $($t2)* );)*

        // check that there are not too many nodes
        for c in $node.children() {
            assert!(
                false $(|| c.kind() == verify_node!(@extract_kind $t1 $($t2)*))*,
                "Node is none of [{}]\n{:?}", stringify!($($t1 $($t2)*),*) ,c);
        }

        // recurse
        $(
            for _c in $node.children().filter(|n| n.kind() == verify_node!(@extract_kind $t1 $($t2)*)) {
                <verify_node!(@extract_type $t1 $($t2)*)>::verify(_c)
            }
        )*
    };

    // Any number of this kind.
    (@check_has_children $node:ident, * $kind:ident) => {};
    // 1 or 0
    (@check_has_children $node:ident, ? $kind:ident) => {
        let count = $node.children_with_tokens().filter(|n| n.kind() == SyntaxKind::$kind).count();
        assert!(count <= 1, "Expecting one or zero sub-node of type {}, found {}\n{:?}", stringify!($kind), count, $node);
    };
    // Exactly one
    (@check_has_children $node:ident, $kind:ident) => {
        let count = $node.children_with_tokens().filter(|n| n.kind() == SyntaxKind::$kind).count();
        assert_eq!(count, 1, "Expecting exactly one sub-node of type {}\n{:?}", stringify!($kind), $node);
    };
    // Exact number
    (@check_has_children $node:ident, $count:literal $kind:ident) => {
        let count = $node.children_with_tokens().filter(|n| n.kind() == SyntaxKind::$kind).count();
        assert_eq!(count, $count, "Expecting {} sub-node of type {}, found {}\n{:?}", $count, stringify!($kind), count, $node);
    };

    (@extract_kind * $kind:ident) => {SyntaxKind::$kind};
    (@extract_kind ? $kind:ident) => {SyntaxKind::$kind};
    (@extract_kind $count:literal $kind:ident) => {SyntaxKind::$kind};
    (@extract_kind $kind:ident) => {SyntaxKind::$kind};

    (@extract_type * $kind:ident) => {$crate::parser::syntax_nodes::$kind};
    (@extract_type ? $kind:ident) => {$crate::parser::syntax_nodes::$kind};
    (@extract_type $count:literal $kind:ident) => {$crate::parser::syntax_nodes::$kind};
    (@extract_type $kind:ident) => {$crate::parser::syntax_nodes::$kind};
}

macro_rules! node_accessors {
    // nothing
    (_) => {};
    // Some combination of children
    ([ $($t1:tt $($t2:ident)?),* ]) => {
        $(node_accessors!{@ $t1 $($t2)*} )*
    };

    (@ * $kind:ident) => {
        #[allow(non_snake_case)]
        pub fn $kind(&self) -> impl Iterator<Item = $kind> {
            self.0.children().filter(|n| n.kind() == SyntaxKind::$kind).map(Into::into)
        }
    };
    (@ ? $kind:ident) => {
        #[allow(non_snake_case)]
        pub fn $kind(&self) -> Option<$kind> {
            self.0.child_node(SyntaxKind::$kind).map(Into::into)
        }
    };
    (@ 2 $kind:ident) => {
        #[allow(non_snake_case)]
        pub fn $kind(&self) -> ($kind, $kind) {
            let mut it = self.0.children().filter(|n| n.kind() == SyntaxKind::$kind);
            let a = it.next().unwrap();
            let b = it.next().unwrap();
            debug_assert!(it.next().is_none());
            (a.into(), b.into())
        }
    };
    (@ 3 $kind:ident) => {
        #[allow(non_snake_case)]
        pub fn $kind(&self) -> ($kind, $kind, $kind) {
            let mut it = self.0.children().filter(|n| n.kind() == SyntaxKind::$kind);
            let a = it.next().unwrap();
            let b = it.next().unwrap();
            let c = it.next().unwrap();
            debug_assert!(it.next().is_none());
            (a.into(), b.into(), c.into())
        }
    };
    (@ $kind:ident) => {
        #[allow(non_snake_case)]
        pub fn $kind(&self) -> $kind {
            self.0.child_node(SyntaxKind::$kind).unwrap().into()
        }
    };

}

/// This macro is invoked once, to declare all the token and syntax kind.
/// The purpose of this macro is to declare the token with its regexp at the same place,
/// and the nodes with their contents.
///
/// This is split into two group: first the tokens, then the nodes.
///
/// # Tokens
///
/// Given as `$token:ident -> $rule:expr`. The rule parameter can be either a string literal or
/// a lexer function. The order of tokens is important because the rules will be run in that order
/// and the first one matching will be chosen.
///
/// # Nodes
///
/// Given as `$(#[$attr:meta])* $nodekind:ident -> [$($children:tt),*] `.
/// Where `children` is a list of sub-nodes (not including tokens).
/// This will allow to self-document and create the structure from the [`syntax_nodes`] module.
/// The children can be prefixed with the following symbol:
///
/// - nothing: The node occurs once and exactly once, the generated accessor returns the node itself
/// - `+`: the node occurs one or several times, the generated accessor returns an `Iterator`
/// - `*`: the node occurs zero or several times, the generated accessor returns an `Iterator`
/// - `?`: the node occurs once or zero times, the generated accessor returns an `Option`
/// - `2` or `3`: the node occurs exactly two or three times, the generated accessor returns a tuple
///
/// Note: the parser must generate the right amount of sub nodes, even if there is a parse error.
///
/// ## The [`syntax_nodes`] module
///
/// Creates one struct for every node with the given accessor.
/// The struct can be converted from and to the node.
macro_rules! declare_syntax {
    ({
        $($token:ident -> $rule:expr ,)*
     }
     {
        $( $(#[$attr:meta])*  $nodekind:ident -> $children:tt ,)*
    })
    => {
        #[repr(u16)]
        #[derive(Debug, Copy, Clone, Eq, PartialEq, num_enum::IntoPrimitive, num_enum::TryFromPrimitive)]
        pub enum SyntaxKind {
            Error,
            Eof,

            // Tokens:
            $(
                /// Token
                $token,
            )*

            // Nodes:
            $(
                $(#[$attr])*
                $nodekind,
            )*
        }

        /// Returns a pair of the matched token type at the beginning of `text`, and its size
        pub fn lex_next_token(text : &str, state: &mut crate::lexer::LexState) -> Option<(usize, SyntaxKind)> {
            use crate::lexer::LexingRule;
            $(
                let len = ($rule).lex(text, state);
                if len > 0 {
                    return Some((len, SyntaxKind::$token));
                }
            )*
            None
        }

        pub mod syntax_nodes {
            use super::*;
            use derive_more::*;
            $(
                #[derive(Debug, Clone, Deref, Into)]
                pub struct $nodekind(SyntaxNode);
                #[cfg(test)]
                impl SyntaxNodeVerify for $nodekind {
                    const KIND: SyntaxKind = SyntaxKind::$nodekind;
                    fn verify(node: SyntaxNode) {
                        assert_eq!(node.kind(), Self::KIND);
                        verify_node!(node, $children);
                    }
                }
                impl $nodekind {
                    node_accessors!{$children}

                    /// Create a new node from a SyntaxNode, if the SyntaxNode is of the correct kind
                    pub fn new(node: SyntaxNode) -> Option<Self> {
                        (node.kind() == SyntaxKind::$nodekind).then(|| Self(node))
                    }
                }

                impl From<SyntaxNode> for $nodekind {
                    fn from(node: SyntaxNode) -> Self {
                        debug_assert_eq!(node.kind(), SyntaxKind::$nodekind);
                        Self(node)
                    }
                }

                impl Spanned for $nodekind {
                    fn span(&self) -> crate::diagnostics::Span {
                        self.0.span()
                    }

                    fn source_file(&self) -> Option<&SourceFile> {
                        self.0.source_file()
                    }
                }
            )*
        }
    }
}
declare_syntax! {
    // Tokens.
    // WARNING: when changing this, do not forget to update the tokenizer in the sixtyfps-rs-macro crate!
    // The order of token is important because the rules will be run in that order
    // and the first one matching will be chosen.
    {
        Whitespace -> &crate::lexer::lex_whitespace,
        Comment -> &crate::lexer::lex_comment,
        StringLiteral -> &crate::lexer::lex_string,
        NumberLiteral -> &crate::lexer::lex_number,
        ColorLiteral -> &crate::lexer::lex_color,
        Identifier -> &crate::lexer::lex_identifier,
        DoubleArrow -> "<=>",
        PlusEqual -> "+=",
        MinusEqual -> "-=",
        StarEqual -> "*=",
        DivEqual -> "/=",
        LessEqual -> "<=",
        GreaterEqual -> ">=",
        EqualEqual -> "==",
        NotEqual -> "!=",
        ColonEqual -> ":=",
        FatArrow -> "=>",
        Arrow -> "->",
        OrOr -> "||",
        AndAnd -> "&&",
        LBrace -> "{",
        RBrace -> "}",
        LParent -> "(",
        RParent -> ")",
        LAngle -> "<",
        RAngle -> ">",
        LBracket -> "[",
        RBracket -> "]",
        Plus -> "+",
        Minus -> "-",
        Star -> "*",
        Div -> "/",
        Equal -> "=",
        Colon -> ":",
        Comma -> ",",
        Semicolon -> ";",
        Bang -> "!",
        Dot -> ".",
        Question -> "?",
        Dollar -> "$",
        At -> "@",
    }
    // syntax kind
    {
        Document -> [ *Component, *ExportsList, *ImportSpecifier, *StructDeclaration ],
        /// `DeclaredIdentifier := Element { ... }`
        Component -> [ DeclaredIdentifier, Element ],
        /// `id := Element { ... }`
        SubElement -> [ Element ],
        Element -> [ ?QualifiedName, *PropertyDeclaration, *Binding, *CallbackConnection,
                     *CallbackDeclaration, *SubElement, *RepeatedElement, *PropertyAnimation,
                     *TwoWayBinding, *States, *Transitions, ?ChildrenPlaceholder ],
        RepeatedElement -> [ ?DeclaredIdentifier, ?RepeatedIndex, Expression , SubElement],
        RepeatedIndex -> [],
        ConditionalElement -> [ Expression , SubElement],
        CallbackDeclaration -> [ DeclaredIdentifier, *Type, ?ReturnType, ?TwoWayBinding ],
        /// `-> type`  (but without the ->)
        ReturnType -> [Type],
        CallbackConnection -> [ *DeclaredIdentifier,  CodeBlock ],
        /// Declaration of a property.
        PropertyDeclaration-> [ ?Type , DeclaredIdentifier, ?BindingExpression, ?TwoWayBinding ],
        /// QualifiedName are the properties name
        PropertyAnimation-> [ *QualifiedName, *Binding ],
        /// wraps Identifiers, like `Rectangle` or `SomeModule.SomeType`
        QualifiedName-> [],
        /// Wraps single identifier (to disambiguate when there are other identifier in the production)
        DeclaredIdentifier -> [],
        ChildrenPlaceholder -> [],
        Binding-> [ BindingExpression ],
        /// `xxx <=> something`
        TwoWayBinding -> [ Expression ],
        /// the right-hand-side of a binding
        // Fixme: the test should be a or
        BindingExpression-> [ ?CodeBlock, ?Expression ],
        CodeBlock-> [ *Expression, *ReturnStatement ],
        ReturnStatement -> [ ?Expression ],
        // FIXME: the test should test that as alternative rather than several of them (but it can also be a literal)
        Expression-> [ ?Expression, ?FunctionCallExpression, ?SelfAssignment,
                       ?ConditionalExpression, ?QualifiedName, ?BinaryExpression, ?Array, ?ObjectLiteral,
                       ?UnaryOpExpression, ?CodeBlock, ?StringTemplate, ?AtImageUrl, ?AtLinearGradient],
        /// Concatenate the Expressions to make a string (usually expended from a template string)
        StringTemplate -> [*Expression],
        /// `@image-url("foo.png")`
        AtImageUrl -> [],
        /// `@linear-gradient(...)`
        AtLinearGradient -> [*Expression],
        /// expression()
        FunctionCallExpression -> [*Expression],
        /// `expression += expression`
        SelfAssignment -> [2 Expression],
        /// `condition ? first : second`
        ConditionalExpression -> [3 Expression],
        /// `expr + expr`
        BinaryExpression -> [2 Expression],
        /// `- expr`
        UnaryOpExpression -> [Expression],
        /// `[ ... ]`
        Array -> [ *Expression ],
        /// `{ foo: bar }`
        ObjectLiteral -> [ *ObjectMember ],
        /// `foo: bar` inside an ObjectLiteral
        ObjectMember -> [ Expression ],
        /// `states: [...]`
        States -> [*State],
        /// The DeclaredIdentifier is the state name. The Expression, if any, is the condition.
        State -> [DeclaredIdentifier, ?Expression, *StatePropertyChange],
        /// binding within a state
        StatePropertyChange -> [ QualifiedName, BindingExpression ],
        /// `transitions: [...]`
        Transitions -> [*Transition],
        /// There is an identifier "in" or "out", the DeclaredIdentifier is the state name
        Transition -> [DeclaredIdentifier, *PropertyAnimation],
        /// Export a set of declared components by name
        ExportsList -> [ *ExportSpecifier, ?Component, *StructDeclaration ],
        /// Declare the first identifier to be exported, either under its name or instead
        /// under the name of the second identifier.
        ExportSpecifier -> [ ExportIdentifier, ?ExportName ],
        ExportIdentifier -> [],
        ExportName -> [],
        /// import { foo, bar, baz } from "blah"; The import uri is stored as string literal.
        ImportSpecifier -> [ ?ImportIdentifierList ],
        ImportIdentifierList -> [ *ImportIdentifier ],
        /// { foo as bar } or just { foo }
        ImportIdentifier -> [ ExternalName, ?InternalName ],
        ExternalName -> [],
        InternalName -> [],
        /// The representation of a type
        Type -> [ ?QualifiedName, ?ObjectType, ?ArrayType ],
        /// `{foo: string, bar: string} `
        ObjectType ->[ *ObjectTypeMember ],
        /// `foo: type` inside an ObjectType
        ObjectTypeMember -> [ Type ],
        /// `[ type ]`
        ArrayType -> [ Type ],
        /// `struct Foo := { ... }
        StructDeclaration -> [DeclaredIdentifier, ObjectType],

    }
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(v: SyntaxKind) -> Self {
        rowan::SyntaxKind(v.into())
    }
}

#[derive(Clone, Debug)]
pub struct Token {
    pub kind: SyntaxKind,
    pub text: SmolStr,
    pub offset: usize,
    #[cfg(feature = "proc_macro_span")]
    pub span: Option<proc_macro::Span>,
}

impl Default for Token {
    fn default() -> Self {
        Token {
            kind: SyntaxKind::Eof,
            text: Default::default(),
            offset: 0,
            #[cfg(feature = "proc_macro_span")]
            span: None,
        }
    }
}

impl Token {
    pub fn as_str(&self) -> &str {
        self.text.as_str()
    }

    pub fn kind(&self) -> SyntaxKind {
        self.kind
    }
}

mod parser_trait {
    //! module allowing to keep implementation details of the node private
    use super::*;

    pub trait Parser: Sized {
        type Checkpoint: Clone;

        /// Enter a new node.  The node is going to be finished when
        /// The return value of this function is dropped
        ///
        /// (do not re-implement this function, re-implement
        /// start_node_impl and finish_node_impl)
        #[must_use = "The node will be finished when it is dropped"]
        fn start_node(&mut self, kind: SyntaxKind) -> Node<Self> {
            self.start_node_impl(kind, None, NodeToken(()));
            Node(self)
        }
        #[must_use = "use start_node_at to use this checkpoint"]
        fn checkpoint(&mut self) -> Self::Checkpoint;
        #[must_use = "The node will be finished when it is dropped"]
        fn start_node_at(&mut self, checkpoint: Self::Checkpoint, kind: SyntaxKind) -> Node<Self> {
            self.start_node_impl(kind, Some(checkpoint), NodeToken(()));
            Node(self)
        }

        /// Can only be called by Node::drop
        fn finish_node_impl(&mut self, token: NodeToken);
        /// Can only be called by Self::start_node
        fn start_node_impl(
            &mut self,
            kind: SyntaxKind,
            checkpoint: Option<Self::Checkpoint>,
            token: NodeToken,
        );

        /// Same as nth(0)
        fn peek(&mut self) -> Token {
            self.nth(0)
        }
        /// Peek the `n`th token, not including whitespace and comments
        fn nth(&mut self, n: usize) -> Token;
        fn consume(&mut self);
        fn error(&mut self, e: impl Into<String>);

        /// Consume the token if it has the right kind, otherwise report a syntax error.
        /// Returns true if the token was consumed.
        fn expect(&mut self, kind: SyntaxKind) -> bool {
            if !self.test(kind) {
                self.error(format!("Syntax error: expected {:?}", kind));
                return false;
            }
            true
        }

        /// If the token if of this type, consume it and return true, otherwise return false
        fn test(&mut self, kind: SyntaxKind) -> bool {
            if self.nth(0).kind() != kind {
                return false;
            }
            self.consume();
            true
        }

        /// consume everything until reaching a token of this kind
        fn until(&mut self, kind: SyntaxKind) {
            // FIXME! match {} () []
            while {
                let k = self.nth(0).kind();
                k != kind && k != SyntaxKind::Eof
            } {
                self.consume();
            }
            self.expect(kind);
        }
    }

    /// A token to proof that start_node_impl and finish_node_impl are only
    /// called from the Node implementation
    ///
    /// Since the constructor is private, it cannot be produced by anything else.
    pub struct NodeToken(());
    /// The return value of `DefaultParser::start_node`. This borrows the parser
    /// and finishes the node on Drop
    #[derive(derive_more::DerefMut)]
    pub struct Node<'a, P: Parser>(&'a mut P);
    impl<'a, P: Parser> Drop for Node<'a, P> {
        fn drop(&mut self) {
            self.0.finish_node_impl(NodeToken(()));
        }
    }
    impl<'a, P: Parser> core::ops::Deref for Node<'a, P> {
        type Target = P;
        fn deref(&self) -> &Self::Target {
            self.0
        }
    }
}
#[doc(inline)]
pub use parser_trait::*;

pub struct DefaultParser<'a> {
    builder: rowan::GreenNodeBuilder<'static>,
    tokens: Vec<Token>,
    cursor: usize,
    diags: &'a mut BuildDiagnostics,
    source_file: SourceFile,
}

impl<'a> DefaultParser<'a> {
    fn from_tokens(tokens: Vec<Token>, diags: &'a mut BuildDiagnostics) -> Self {
        Self {
            builder: Default::default(),
            tokens,
            cursor: 0,
            diags,
            source_file: Default::default(),
        }
    }

    /// Constructor that create a parser from the source code
    pub fn new(source: &str, diags: &'a mut BuildDiagnostics) -> Self {
        Self::from_tokens(crate::lexer::lex(source), diags)
    }

    fn current_token(&self) -> Token {
        self.tokens.get(self.cursor).cloned().unwrap_or_default()
    }

    /// Consume all the whitespace
    pub fn consume_ws(&mut self) {
        while matches!(self.current_token().kind, SyntaxKind::Whitespace | SyntaxKind::Comment) {
            self.consume()
        }
    }
}

impl Parser for DefaultParser<'_> {
    fn start_node_impl(
        &mut self,
        kind: SyntaxKind,
        checkpoint: Option<Self::Checkpoint>,
        _: NodeToken,
    ) {
        match checkpoint {
            None => self.builder.start_node(kind.into()),
            Some(cp) => self.builder.start_node_at(cp, kind.into()),
        }
    }

    fn finish_node_impl(&mut self, _: NodeToken) {
        self.builder.finish_node();
    }

    /// Peek the `n`th token, not including whitespace and comments
    fn nth(&mut self, mut n: usize) -> Token {
        self.consume_ws();
        let mut c = self.cursor;
        while n > 0 {
            n -= 1;
            c += 1;
            while c < self.tokens.len()
                && matches!(self.tokens[c].kind, SyntaxKind::Whitespace | SyntaxKind::Comment)
            {
                c += 1;
            }
        }
        self.tokens.get(c).cloned().unwrap_or_default()
    }

    /// Consume the current token
    fn consume(&mut self) {
        let t = self.current_token();
        self.builder.token(t.kind.into(), t.text.as_str());
        self.cursor += 1;
    }

    /// Reports an error at the current token location
    fn error(&mut self, e: impl Into<String>) {
        let current_token = self.current_token();
        #[allow(unused_mut)]
        let mut span = crate::diagnostics::Span::new(current_token.offset);
        #[cfg(feature = "proc_macro_span")]
        {
            span.span = current_token.span;
        }

        self.diags.push_error_with_span(
            e.into(),
            crate::diagnostics::SourceLocation {
                source_file: Some(self.source_file.clone()),
                span,
            },
        );
    }

    type Checkpoint = rowan::Checkpoint;
    fn checkpoint(&mut self) -> Self::Checkpoint {
        self.builder.checkpoint()
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, Hash, PartialEq, PartialOrd)]
pub enum Language {}
impl rowan::Language for Language {
    type Kind = SyntaxKind;
    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        SyntaxKind::try_from(raw.0).unwrap()
    }
    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        kind.into()
    }
}

#[derive(Debug, Clone, derive_more::Deref)]
pub struct SyntaxNode {
    #[deref]
    pub node: rowan::SyntaxNode<Language>,
    pub source_file: SourceFile,
}

#[derive(Debug, Clone, derive_more::Deref)]
pub struct SyntaxToken {
    #[deref]
    pub token: rowan::SyntaxToken<Language>,
    pub source_file: SourceFile,
}

impl SyntaxToken {
    pub fn parent(&self) -> SyntaxNode {
        SyntaxNode { node: self.token.parent(), source_file: self.source_file.clone() }
    }
}

impl std::fmt::Display for SyntaxToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.token.fmt(f)
    }
}

impl SyntaxNode {
    pub fn child_node(&self, kind: SyntaxKind) -> Option<SyntaxNode> {
        self.node
            .children()
            .find(|n| n.kind() == kind)
            .map(|node| SyntaxNode { node, source_file: self.source_file.clone() })
    }
    pub fn child_token(&self, kind: SyntaxKind) -> Option<SyntaxToken> {
        self.node
            .children_with_tokens()
            .find(|n| n.kind() == kind)
            .and_then(|x| x.into_token())
            .map(|token| SyntaxToken { token, source_file: self.source_file.clone() })
    }
    pub fn child_text(&self, kind: SyntaxKind) -> Option<String> {
        self.node
            .children_with_tokens()
            .find(|n| n.kind() == kind)
            .and_then(|x| x.as_token().map(|x| x.text().to_string()))
    }
    pub fn kind(&self) -> SyntaxKind {
        self.node.kind()
    }
    pub fn children(&self) -> impl Iterator<Item = SyntaxNode> {
        let source_file = self.source_file.clone();
        self.node.children().map(move |node| SyntaxNode { node, source_file: source_file.clone() })
    }
    pub fn children_with_tokens(&self) -> impl Iterator<Item = NodeOrToken> {
        let source_file = self.source_file.clone();
        self.node.children_with_tokens().map(move |token| match token {
            rowan::NodeOrToken::Node(node) => {
                SyntaxNode { node, source_file: source_file.clone() }.into()
            }
            rowan::NodeOrToken::Token(token) => {
                SyntaxToken { token, source_file: source_file.clone() }.into()
            }
        })
    }
    pub fn text(&self) -> rowan::SyntaxText {
        self.node.text()
    }
    pub fn parent(&self) -> Option<SyntaxNode> {
        self.node.parent().map(|node| SyntaxNode { node, source_file: self.source_file.clone() })
    }
}

#[derive(Debug, Clone, derive_more::From)]
pub enum NodeOrToken {
    Node(SyntaxNode),
    Token(SyntaxToken),
}

impl NodeOrToken {
    pub fn kind(&self) -> SyntaxKind {
        match self {
            NodeOrToken::Node(n) => n.kind(),
            NodeOrToken::Token(t) => t.kind(),
        }
    }

    pub fn as_node(&self) -> Option<&SyntaxNode> {
        match self {
            NodeOrToken::Node(n) => Some(n),
            NodeOrToken::Token(_) => None,
        }
    }

    pub fn as_token(&self) -> Option<&SyntaxToken> {
        match self {
            NodeOrToken::Node(_) => None,
            NodeOrToken::Token(t) => Some(t),
        }
    }

    pub fn into_token(self) -> Option<SyntaxToken> {
        match self {
            NodeOrToken::Token(t) => Some(t),
            _ => None,
        }
    }

    pub fn into_node(self) -> Option<SyntaxNode> {
        match self {
            NodeOrToken::Node(n) => Some(n),
            _ => None,
        }
    }
}

impl Spanned for SyntaxNode {
    fn span(&self) -> crate::diagnostics::Span {
        crate::diagnostics::Span::new(self.node.text_range().start().into())
    }

    fn source_file(&self) -> Option<&SourceFile> {
        Some(&self.source_file)
    }
}

impl Spanned for Option<SyntaxNode> {
    fn span(&self) -> crate::diagnostics::Span {
        self.as_ref().map(|n| n.span()).unwrap_or_default()
    }

    fn source_file(&self) -> Option<&SourceFile> {
        self.as_ref().and_then(|n| n.source_file())
    }
}

impl Spanned for SyntaxToken {
    fn span(&self) -> crate::diagnostics::Span {
        crate::diagnostics::Span::new(self.token.text_range().start().into())
    }

    fn source_file(&self) -> Option<&SourceFile> {
        Some(&self.source_file)
    }
}

impl Spanned for NodeOrToken {
    fn span(&self) -> crate::diagnostics::Span {
        match self {
            NodeOrToken::Node(n) => n.span(),
            NodeOrToken::Token(t) => t.span(),
        }
    }

    fn source_file(&self) -> Option<&SourceFile> {
        match self {
            NodeOrToken::Node(n) => n.source_file(),
            NodeOrToken::Token(t) => t.source_file(),
        }
    }
}

impl Spanned for Option<NodeOrToken> {
    fn span(&self) -> crate::diagnostics::Span {
        self.as_ref().map(|t| t.span()).unwrap_or_default()
    }
    fn source_file(&self) -> Option<&SourceFile> {
        self.as_ref().and_then(|t| t.source_file())
    }
}

impl Spanned for Option<SyntaxToken> {
    fn span(&self) -> crate::diagnostics::Span {
        self.as_ref().map(|t| t.span()).unwrap_or_default()
    }
    fn source_file(&self) -> Option<&SourceFile> {
        self.as_ref().and_then(|t| t.source_file())
    }
}

/// return the normalized identifier string of the first SyntaxKind::Identifier in this node
pub fn identifier_text(node: &SyntaxNode) -> Option<String> {
    node.child_text(SyntaxKind::Identifier).map(|x| normalize_identifier(&x))
}

pub fn normalize_identifier(ident: &str) -> String {
    ident.replace('-', "_")
}

// Actual parser
pub fn parse(
    source: String,
    path: Option<&std::path::Path>,
    build_diagnostics: &mut BuildDiagnostics,
) -> SyntaxNode {
    let mut p = DefaultParser::new(&source, build_diagnostics);
    let source_file = if let Some(path) = path {
        p.source_file =
            std::rc::Rc::new(crate::diagnostics::SourceFileInner::new(path.to_path_buf(), source));
        p.source_file.clone()
    } else {
        Default::default()
    };
    document::parse_document(&mut p);
    SyntaxNode { node: rowan::SyntaxNode::new_root(p.builder.finish()), source_file }
}

pub fn parse_file<P: AsRef<std::path::Path>>(
    path: P,
    build_diagnostics: &mut BuildDiagnostics,
) -> Option<SyntaxNode> {
    let source = crate::diagnostics::load_from_path(path.as_ref())
        .map_err(|d| build_diagnostics.push_internal_error(d))
        .ok()?;
    Some(parse(source, Some(path.as_ref()), build_diagnostics))
}

pub fn parse_tokens(
    tokens: Vec<Token>,
    source_file: SourceFile,
    diags: &mut BuildDiagnostics,
) -> SyntaxNode {
    let mut p = DefaultParser::from_tokens(tokens, diags);
    document::parse_document(&mut p);
    SyntaxNode { node: rowan::SyntaxNode::new_root(p.builder.finish()), source_file }
}
