use serde_json::Value;

/// A node in an RTML template.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    /// Literal template text. Trusted; interpolations are what get escaped.
    Text(String),
    /// Raw markup that is emitted verbatim (`<!doctype html>`, script/style).
    Raw(String),
    /// `{{ expression }}`.
    Interp(Expr),
    /// An HTML-like element.
    Element(Element),
    /// `@if condition { ... } @else { ... }`.
    If {
        condition: Expr,
        then_branch: Vec<Node>,
        else_branch: Vec<Node>,
    },
    /// `@for item in items { ... }`.
    For {
        variable: String,
        iterable: Expr,
        body: Vec<Node>,
    },
}

/// An element such as `<h1 class="title">`.
#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    pub tag: String,
    pub attrs: Vec<Attr>,
    pub children: Vec<Node>,
    pub self_closing: bool,
}

/// An attribute. `None` means a boolean attribute such as `disabled`.
#[derive(Debug, Clone, PartialEq)]
pub struct Attr {
    pub name: String,
    pub value: Option<Vec<Node>>,
}

/// A small expression language over JSON data.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal(Value),
    Path(Vec<String>),
    Unary(UnaryOp, Box<Expr>),
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    Neg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Sub,
}
