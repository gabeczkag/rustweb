use crate::ast::{Attr, Element, Node};
use crate::expr;
use crate::Error;

const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

const RAW_TEXT_ELEMENTS: &[&str] = &["script", "style"];

/// Parse an RTML template into a node tree.
pub fn parse(input: &str) -> Result<Vec<Node>, Error> {
    Parser::new(input).parse_nodes(Until::Eof)
}

enum Until {
    Eof,
    Brace,
    ClosingTag,
}

struct Parser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    fn peek(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let character = self.peek()?;
        self.pos += character.len_utf8();
        Some(character)
    }

    fn starts_with(&self, prefix: &str) -> bool {
        self.input[self.pos..].starts_with(prefix)
    }

    fn consume(&mut self, prefix: &str) {
        debug_assert!(self.starts_with(prefix));
        self.pos += prefix.len();
    }

    fn skip_whitespace(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
    }

    fn expect(&mut self, expected: char) -> Result<(), Error> {
        match self.bump() {
            Some(character) if character == expected => Ok(()),
            _ => self.error(format!("expected `{expected}`")),
        }
    }

    fn error<T>(&self, message: impl Into<String>) -> Result<T, Error> {
        Err(Error::Parse {
            offset: self.pos,
            message: message.into(),
        })
    }

    fn word_boundary(&self, prefix_len: usize) -> bool {
        self.input
            .get(self.pos + prefix_len..)
            .and_then(|rest| rest.chars().next())
            .is_none_or(|character| {
                character.is_whitespace() || character == '(' || character == '{'
            })
    }

    fn at_directive(&self) -> bool {
        self.starts_with("@*")
            || (self.starts_with("@if") && self.word_boundary(3))
            || (self.starts_with("@for") && self.word_boundary(4))
    }

    fn parse_nodes(&mut self, until: Until) -> Result<Vec<Node>, Error> {
        let mut nodes = Vec::new();

        loop {
            if self.peek().is_none() {
                if matches!(until, Until::Eof) {
                    return Ok(nodes);
                }
                return self.error("unexpected end of template");
            }

            if matches!(until, Until::Brace) && self.peek() == Some('}') {
                return Ok(nodes);
            }

            if matches!(until, Until::ClosingTag) && self.starts_with("</") {
                return Ok(nodes);
            }

            if self.starts_with("<!--") {
                self.skip_html_comment()?;
            } else if self.starts_with("<!") {
                nodes.push(Node::Raw(self.take_doctype()?));
            } else if self.starts_with("{{") {
                nodes.push(Node::Interp(self.parse_interpolation()?));
            } else if self.starts_with("@*") {
                self.skip_template_comment()?;
            } else if self.starts_with("@if") && self.word_boundary(3) {
                nodes.push(self.parse_if()?);
            } else if self.starts_with("@for") && self.word_boundary(4) {
                nodes.push(self.parse_for()?);
            } else if self.peek() == Some('<') {
                nodes.push(self.parse_element()?);
            } else {
                let text = self.take_text(&until);
                if !text.is_empty() {
                    nodes.push(Node::Text(text));
                }
            }
        }
    }

    fn take_text(&mut self, until: &Until) -> String {
        let start = self.pos;

        while let Some(character) = self.peek() {
            if character == '<' || self.starts_with("{{") || self.at_directive() {
                break;
            }
            if matches!(until, Until::Brace) && character == '}' {
                break;
            }
            self.bump();
        }

        self.input[start..self.pos].to_owned()
    }

    fn skip_html_comment(&mut self) -> Result<(), Error> {
        let end = self.input[self.pos + 4..]
            .find("-->")
            .ok_or_else(|| Error::Parse {
                offset: self.pos,
                message: "unterminated HTML comment".to_owned(),
            })?;
        self.pos += 4 + end + 3;
        Ok(())
    }

    fn skip_template_comment(&mut self) -> Result<(), Error> {
        let end = self.input[self.pos + 2..]
            .find("*@")
            .ok_or_else(|| Error::Parse {
                offset: self.pos,
                message: "unterminated RTML comment".to_owned(),
            })?;
        self.pos += 2 + end + 2;
        Ok(())
    }

    fn take_doctype(&mut self) -> Result<String, Error> {
        let start = self.pos;
        let end = self.input[self.pos..]
            .find('>')
            .ok_or_else(|| Error::Parse {
                offset: self.pos,
                message: "unterminated declaration".to_owned(),
            })?;
        self.pos += end + 1;
        Ok(self.input[start..self.pos].to_owned())
    }

    fn parse_interpolation(&mut self) -> Result<crate::ast::Expr, Error> {
        let start = self.pos;
        self.consume("{{");
        let end = self.input[self.pos..]
            .find("}}")
            .ok_or_else(|| Error::Parse {
                offset: start,
                message: "unterminated interpolation".to_owned(),
            })?;
        let source = self.input[self.pos..self.pos + end].trim();
        self.pos += end + 2;

        expr::parse(source)
    }

    fn parse_element(&mut self) -> Result<Node, Error> {
        self.expect('<')?;
        if self.peek() == Some('/') {
            return self.error("unexpected closing tag");
        }

        let tag = self.parse_tag_name()?;
        let mut attrs = Vec::new();
        let mut self_closing = false;

        loop {
            self.skip_whitespace();

            if self.starts_with("/>") {
                self.consume("/>");
                self_closing = true;
                break;
            }

            if self.peek() == Some('>') {
                self.bump();
                break;
            }

            if self.peek().is_none() {
                return self.error(format!("unclosed `<{tag}>` tag"));
            }

            let name = self.parse_attr_name()?;
            self.skip_whitespace();

            let value = if self.peek() == Some('=') {
                self.bump();
                self.skip_whitespace();
                Some(self.parse_attr_value()?)
            } else {
                None
            };

            attrs.push(Attr { name, value });
        }

        let children = if self_closing || is_void(&tag) {
            Vec::new()
        } else if is_raw_text(&tag) {
            let raw = self.take_raw_text(&tag)?;
            self.expect_closing_tag(&tag)?;
            if raw.is_empty() {
                Vec::new()
            } else {
                vec![Node::Raw(raw)]
            }
        } else {
            let children = self.parse_nodes(Until::ClosingTag)?;
            self.expect_closing_tag(&tag)?;
            children
        };

        Ok(Node::Element(Element {
            tag,
            attrs,
            children,
            self_closing,
        }))
    }

    fn parse_tag_name(&mut self) -> Result<String, Error> {
        let start = self.pos;

        while self.peek().is_some_and(|character| {
            character.is_alphanumeric() || matches!(character, '-' | '_' | ':' | '.')
        }) {
            self.bump();
        }

        if self.pos == start {
            return self.error("expected a tag name");
        }

        Ok(self.input[start..self.pos].to_owned())
    }

    fn parse_attr_name(&mut self) -> Result<String, Error> {
        let start = self.pos;

        while self.peek().is_some_and(|character| {
            !character.is_whitespace() && !matches!(character, '=' | '>' | '/')
        }) {
            self.bump();
        }

        if self.pos == start {
            return self.error("expected an attribute name");
        }

        Ok(self.input[start..self.pos].to_owned())
    }

    fn parse_attr_value(&mut self) -> Result<Vec<Node>, Error> {
        match self.peek() {
            Some(quote @ ('"' | '\'')) => {
                self.bump();
                let mut nodes = Vec::new();
                let mut text = String::new();

                loop {
                    match self.peek() {
                        None => return self.error("unterminated attribute value"),
                        Some(character) if character == quote => {
                            self.bump();
                            break;
                        }
                        Some(_) if self.starts_with("{{") => {
                            if !text.is_empty() {
                                nodes.push(Node::Text(std::mem::take(&mut text)));
                            }
                            nodes.push(Node::Interp(self.parse_interpolation()?));
                        }
                        Some(character) => {
                            text.push(character);
                            self.bump();
                        }
                    }
                }

                if !text.is_empty() {
                    nodes.push(Node::Text(text));
                }

                Ok(nodes)
            }
            _ => {
                let start = self.pos;

                while let Some(character) = self.peek() {
                    if character.is_whitespace() || character == '>' {
                        break;
                    }
                    if character == '/' && self.input[self.pos..].starts_with("/>") {
                        break;
                    }
                    self.bump();
                }

                parse_fragment(&self.input[start..self.pos])
            }
        }
    }

    fn take_raw_text(&mut self, tag: &str) -> Result<String, Error> {
        let pattern = format!("</{tag}");
        let end = self.input[self.pos..]
            .find(&pattern)
            .ok_or_else(|| Error::Parse {
                offset: self.pos,
                message: format!("unclosed `<{tag}>` tag"),
            })?;
        let text = self.input[self.pos..self.pos + end].to_owned();
        self.pos += end;
        Ok(text)
    }

    fn expect_closing_tag(&mut self, tag: &str) -> Result<(), Error> {
        if !self.starts_with("</") {
            return self.error(format!("expected closing `</{tag}>`"));
        }
        self.consume("</");
        let closing = self.parse_tag_name()?;
        self.skip_whitespace();
        self.expect('>')?;

        if closing != tag {
            return self.error(format!(
                "mismatched closing tag: expected `</{tag}>`, found `</{closing}>`"
            ));
        }

        Ok(())
    }

    fn parse_condition(&mut self) -> Result<crate::ast::Expr, Error> {
        self.skip_whitespace();
        let start = self.pos;
        let mut quote = None;
        let mut parentheses = 0usize;

        while let Some(character) = self.peek() {
            if let Some(active_quote) = quote {
                self.bump();
                if character == '\\' {
                    self.bump();
                } else if character == active_quote {
                    quote = None;
                }
                continue;
            }

            match character {
                '"' | '\'' => {
                    quote = Some(character);
                    self.bump();
                }
                '(' => {
                    parentheses += 1;
                    self.bump();
                }
                ')' => {
                    parentheses = parentheses.saturating_sub(1);
                    self.bump();
                }
                '{' if parentheses == 0 => break,
                _ => {
                    self.bump();
                }
            }
        }

        let source = self.input[start..self.pos].trim();
        if source.is_empty() {
            return self.error("expected an expression");
        }
        self.expect('{')?;
        expr::parse(source)
    }

    fn parse_if(&mut self) -> Result<Node, Error> {
        self.consume("@if");
        let condition = self.parse_condition()?;
        let then_branch = self.parse_nodes(Until::Brace)?;
        self.expect('}')?;
        let else_branch = self.parse_else()?;

        Ok(Node::If {
            condition,
            then_branch,
            else_branch,
        })
    }

    fn parse_else(&mut self) -> Result<Vec<Node>, Error> {
        let saved = self.pos;
        self.skip_whitespace();

        if !(self.starts_with("@else") && self.word_boundary(5)) {
            self.pos = saved;
            return Ok(Vec::new());
        }

        self.consume("@else");
        self.skip_whitespace();

        if self.starts_with("if") && self.word_boundary(2) {
            self.consume("if");
            let condition = self.parse_condition()?;
            let then_branch = self.parse_nodes(Until::Brace)?;
            self.expect('}')?;
            let else_branch = self.parse_else()?;
            Ok(vec![Node::If {
                condition,
                then_branch,
                else_branch,
            }])
        } else {
            self.expect('{')?;
            let body = self.parse_nodes(Until::Brace)?;
            self.expect('}')?;
            Ok(body)
        }
    }

    fn parse_for(&mut self) -> Result<Node, Error> {
        self.consume("@for");
        self.skip_whitespace();
        let variable = self.parse_ident()?;
        self.skip_whitespace();

        if !(self.starts_with("in") && self.word_boundary(2)) {
            return self.error("expected `in` after the loop variable");
        }
        self.consume("in");

        let iterable = self.parse_condition()?;
        let body = self.parse_nodes(Until::Brace)?;
        self.expect('}')?;

        Ok(Node::For {
            variable,
            iterable,
            body,
        })
    }

    fn parse_ident(&mut self) -> Result<String, Error> {
        let start = self.pos;

        while self
            .peek()
            .is_some_and(|character| character.is_alphanumeric() || character == '_')
        {
            self.bump();
        }

        if self.pos == start {
            return self.error("expected an identifier");
        }

        Ok(self.input[start..self.pos].to_owned())
    }
}

fn parse_fragment(raw: &str) -> Result<Vec<Node>, Error> {
    let mut nodes = Vec::new();
    let mut rest = raw;

    while let Some(start) = rest.find("{{") {
        if start > 0 {
            nodes.push(Node::Text(rest[..start].to_owned()));
        }

        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            return Err(Error::Parse {
                offset: start,
                message: "unterminated interpolation".to_owned(),
            });
        };

        nodes.push(Node::Interp(expr::parse(after[..end].trim())?));
        rest = &after[end + 2..];
    }

    if !rest.is_empty() {
        nodes.push(Node::Text(rest.to_owned()));
    }

    Ok(nodes)
}

fn is_void(tag: &str) -> bool {
    VOID_ELEMENTS.contains(&tag.to_ascii_lowercase().as_str())
}

fn is_raw_text(tag: &str) -> bool {
    RAW_TEXT_ELEMENTS.contains(&tag.to_ascii_lowercase().as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_elements_and_interpolation() {
        let nodes = parse(r#"<h1 class="title">{{ user.name }}</h1>"#).unwrap();
        assert_eq!(nodes.len(), 1);
        let Node::Element(element) = &nodes[0] else {
            panic!("expected element");
        };
        assert_eq!(element.tag, "h1");
        assert_eq!(element.attrs[0].name, "class");
        assert_eq!(element.children.len(), 1);
    }

    #[test]
    fn parses_if_and_for() {
        let nodes = parse(
            r#"@if show {<p>Hi</p>} @else {<p>Bye</p>}
               @for item in items {<span>{{ item }}</span>}"#,
        )
        .unwrap();
        assert!(matches!(nodes[0], Node::If { .. }));
        assert!(nodes.iter().any(|node| matches!(node, Node::For { .. })));
    }

    #[test]
    fn rejects_mismatched_tags() {
        let error = parse("<div><span></div></span>").unwrap_err();
        assert!(matches!(error, Error::Parse { .. }));
    }

    #[test]
    fn void_elements_do_not_need_closing_tags() {
        let nodes = parse(r#"<img src="/a.png"><br>"#).unwrap();
        assert_eq!(nodes.len(), 2);
    }
}
