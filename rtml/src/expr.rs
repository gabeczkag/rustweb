use std::cmp::Ordering;

use serde_json::{Number, Value};

use crate::ast::{BinaryOp, Expr, UnaryOp};
use crate::Error;

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Ident(String),
    String(String),
    Number(Number),
    True,
    False,
    Null,
    Dot,
    LParen,
    RParen,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    Not,
    Plus,
    Minus,
    Eof,
}

struct Lexer<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Lexer<'a> {
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

    fn skip_whitespace(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
    }

    fn tokenize(mut self) -> Result<Vec<(Token, usize)>, Error> {
        let mut tokens = Vec::new();

        loop {
            self.skip_whitespace();
            let offset = self.pos;
            let Some(character) = self.peek() else {
                tokens.push((Token::Eof, offset));
                return Ok(tokens);
            };

            let token = match character {
                '"' | '\'' => Token::String(self.lex_string(character)?),
                '.' => {
                    self.bump();
                    Token::Dot
                }
                '(' => {
                    self.bump();
                    Token::LParen
                }
                ')' => {
                    self.bump();
                    Token::RParen
                }
                '=' => {
                    self.bump();
                    if self.peek() == Some('=') {
                        self.bump();
                        Token::Eq
                    } else {
                        return Err(self.error(offset, "expected `==`"));
                    }
                }
                '!' => {
                    self.bump();
                    if self.peek() == Some('=') {
                        self.bump();
                        Token::Ne
                    } else {
                        Token::Not
                    }
                }
                '<' => {
                    self.bump();
                    if self.peek() == Some('=') {
                        self.bump();
                        Token::Le
                    } else {
                        Token::Lt
                    }
                }
                '>' => {
                    self.bump();
                    if self.peek() == Some('=') {
                        self.bump();
                        Token::Ge
                    } else {
                        Token::Gt
                    }
                }
                '&' => {
                    self.bump();
                    if self.peek() == Some('&') {
                        self.bump();
                        Token::And
                    } else {
                        return Err(self.error(offset, "expected `&&`"));
                    }
                }
                '|' => {
                    self.bump();
                    if self.peek() == Some('|') {
                        self.bump();
                        Token::Or
                    } else {
                        return Err(self.error(offset, "expected `||`"));
                    }
                }
                '+' => {
                    self.bump();
                    Token::Plus
                }
                '-' => {
                    self.bump();
                    Token::Minus
                }
                character if character.is_ascii_digit() => Token::Number(self.lex_number()?),
                character if character.is_alphabetic() || character == '_' => {
                    let ident = self.lex_ident();
                    match ident.as_str() {
                        "true" => Token::True,
                        "false" => Token::False,
                        "null" => Token::Null,
                        _ => Token::Ident(ident),
                    }
                }
                other => {
                    return Err(self.error(offset, format!("unexpected character `{other}`")));
                }
            };

            tokens.push((token, offset));
        }
    }

    fn lex_string(&mut self, quote: char) -> Result<String, Error> {
        let start = self.pos;
        self.bump();
        let mut output = String::new();

        loop {
            match self.bump() {
                Some(character) if character == quote => break,
                Some('\\') => match self.bump() {
                    Some('n') => output.push('\n'),
                    Some('r') => output.push('\r'),
                    Some('t') => output.push('\t'),
                    Some('\\') => output.push('\\'),
                    Some('"') => output.push('"'),
                    Some('\'') => output.push('\''),
                    Some(other) => {
                        return Err(self.error(
                            self.pos.saturating_sub(other.len_utf8()),
                            format!("unknown escape `\\{other}`"),
                        ));
                    }
                    None => return Err(self.error(start, "unterminated string")),
                },
                Some(character) => output.push(character),
                None => return Err(self.error(start, "unterminated string")),
            }
        }

        Ok(output)
    }

    fn lex_number(&mut self) -> Result<Number, Error> {
        let start = self.pos;

        while self
            .peek()
            .is_some_and(|character| character.is_ascii_digit())
        {
            self.bump();
        }

        if self.peek() == Some('.')
            && self
                .input
                .get(self.pos + 1..)
                .and_then(|rest| rest.chars().next())
                .is_some_and(|character| character.is_ascii_digit())
        {
            self.bump();
            while self
                .peek()
                .is_some_and(|character| character.is_ascii_digit())
            {
                self.bump();
            }
        }

        let raw = &self.input[start..self.pos];

        if raw.contains('.') {
            raw.parse::<f64>()
                .ok()
                .and_then(Number::from_f64)
                .ok_or_else(|| self.error(start, format!("invalid number `{raw}`")))
        } else {
            raw.parse::<i64>()
                .map(Number::from)
                .or_else(|_| raw.parse::<u64>().map(Number::from))
                .map_err(|_| self.error(start, format!("invalid number `{raw}`")))
        }
    }

    fn lex_ident(&mut self) -> String {
        let start = self.pos;
        while self
            .peek()
            .is_some_and(|character| character.is_alphanumeric() || character == '_')
        {
            self.bump();
        }
        self.input[start..self.pos].to_owned()
    }

    fn error(&self, offset: usize, message: impl Into<String>) -> Error {
        Error::Expr {
            offset,
            message: message.into(),
        }
    }
}

struct Parser {
    tokens: Vec<(Token, usize)>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> &(Token, usize) {
        &self.tokens[self.pos]
    }

    fn advance(&mut self) -> (Token, usize) {
        let token = self.tokens[self.pos].clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        token
    }

    fn parse_binary(&mut self, min_precedence: u8) -> Result<Expr, Error> {
        let mut left = self.parse_unary()?;

        loop {
            let (operator, precedence) = match &self.peek().0 {
                Token::Or => (BinaryOp::Or, 1),
                Token::And => (BinaryOp::And, 2),
                Token::Eq => (BinaryOp::Eq, 3),
                Token::Ne => (BinaryOp::Ne, 3),
                Token::Lt => (BinaryOp::Lt, 4),
                Token::Le => (BinaryOp::Le, 4),
                Token::Gt => (BinaryOp::Gt, 4),
                Token::Ge => (BinaryOp::Ge, 4),
                Token::Plus => (BinaryOp::Add, 5),
                Token::Minus => (BinaryOp::Sub, 5),
                _ => break,
            };

            if precedence < min_precedence {
                break;
            }

            self.advance();
            let right = self.parse_binary(precedence + 1)?;
            left = Expr::Binary(operator, Box::new(left), Box::new(right));
        }

        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, Error> {
        match &self.peek().0 {
            Token::Not => {
                self.advance();
                Ok(Expr::Unary(UnaryOp::Not, Box::new(self.parse_unary()?)))
            }
            Token::Minus => {
                self.advance();
                Ok(Expr::Unary(UnaryOp::Neg, Box::new(self.parse_unary()?)))
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, Error> {
        let (token, offset) = self.advance();

        match token {
            Token::String(value) => Ok(Expr::Literal(Value::String(value))),
            Token::Number(value) => Ok(Expr::Literal(Value::Number(value))),
            Token::True => Ok(Expr::Literal(Value::Bool(true))),
            Token::False => Ok(Expr::Literal(Value::Bool(false))),
            Token::Null => Ok(Expr::Literal(Value::Null)),
            Token::Ident(first) => {
                let mut segments = vec![first];

                while matches!(&self.peek().0, Token::Dot) {
                    self.advance();
                    match self.advance() {
                        (Token::Ident(segment), _) => segments.push(segment),
                        (Token::Number(number), _) => segments.push(number.to_string()),
                        (_, offset) => {
                            return Err(Error::Expr {
                                offset,
                                message: "expected a field name after `.`".to_owned(),
                            });
                        }
                    }
                }

                Ok(Expr::Path(segments))
            }
            Token::LParen => {
                let expression = self.parse_binary(0)?;
                if self.advance().0 != Token::RParen {
                    return Err(Error::Expr {
                        offset,
                        message: "expected `)`".to_owned(),
                    });
                }
                Ok(expression)
            }
            other => Err(Error::Expr {
                offset,
                message: format!("unexpected token `{other:?}`"),
            }),
        }
    }
}

pub fn parse(input: &str) -> Result<Expr, Error> {
    let tokens = Lexer::new(input).tokenize()?;
    let mut parser = Parser { tokens, pos: 0 };
    let expression = parser.parse_binary(0)?;

    if !matches!(&parser.peek().0, Token::Eof) {
        let (_, offset) = parser.peek().clone();
        return Err(Error::Expr {
            offset,
            message: "unexpected trailing tokens".to_owned(),
        });
    }

    Ok(expression)
}

pub fn evaluate(expression: &Expr, context: &Value) -> Value {
    match expression {
        Expr::Literal(value) => value.clone(),
        Expr::Path(path) => resolve(context, path).cloned().unwrap_or(Value::Null),
        Expr::Unary(UnaryOp::Not, inner) => Value::Bool(!truthy(&evaluate(inner, context))),
        Expr::Unary(UnaryOp::Neg, inner) => match evaluate(inner, context) {
            Value::Number(number) => match number.as_i64() {
                Some(value) => Value::Number(Number::from(-value)),
                None => number
                    .as_f64()
                    .and_then(|value| Number::from_f64(-value))
                    .map(Value::Number)
                    .unwrap_or(Value::Null),
            },
            _ => Value::Null,
        },
        Expr::Binary(BinaryOp::Or, left, right) => {
            if truthy(&evaluate(left, context)) {
                Value::Bool(true)
            } else {
                Value::Bool(truthy(&evaluate(right, context)))
            }
        }
        Expr::Binary(BinaryOp::And, left, right) => {
            if !truthy(&evaluate(left, context)) {
                Value::Bool(false)
            } else {
                Value::Bool(truthy(&evaluate(right, context)))
            }
        }
        Expr::Binary(operator, left, right) => {
            let left = evaluate(left, context);
            let right = evaluate(right, context);
            evaluate_binary(*operator, &left, &right)
        }
    }
}

pub fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|value| value != 0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}

pub fn to_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        Value::Array(_) | Value::Object(_) => serde_json::to_string(value).unwrap_or_default(),
    }
}

fn resolve<'a>(context: &'a Value, path: &[String]) -> Option<&'a Value> {
    let mut current = context;

    for segment in path {
        current = match current {
            Value::Object(map) => map.get(segment)?,
            Value::Array(items) => items.get(segment.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }

    Some(current)
}

fn evaluate_binary(operator: BinaryOp, left: &Value, right: &Value) -> Value {
    match operator {
        BinaryOp::Eq => Value::Bool(left == right),
        BinaryOp::Ne => Value::Bool(left != right),
        BinaryOp::Lt => Value::Bool(compare(left, right) == Some(Ordering::Less)),
        BinaryOp::Le => Value::Bool(matches!(
            compare(left, right),
            Some(Ordering::Less | Ordering::Equal)
        )),
        BinaryOp::Gt => Value::Bool(compare(left, right) == Some(Ordering::Greater)),
        BinaryOp::Ge => Value::Bool(matches!(
            compare(left, right),
            Some(Ordering::Greater | Ordering::Equal)
        )),
        BinaryOp::Add => add(left, right),
        BinaryOp::Sub => subtract(left, right),
        BinaryOp::Or | BinaryOp::And => unreachable!("handled before evaluation"),
    }
}

fn compare(left: &Value, right: &Value) -> Option<Ordering> {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => left.as_f64()?.partial_cmp(&right.as_f64()?),
        (Value::String(left), Value::String(right)) => Some(left.cmp(right)),
        _ => None,
    }
}

fn add(left: &Value, right: &Value) -> Value {
    if let (Value::Number(left), Value::Number(right)) = (left, right) {
        if let (Some(left), Some(right)) = (left.as_i64(), right.as_i64()) {
            return Value::Number(Number::from(left + right));
        }

        return left
            .as_f64()
            .zip(right.as_f64())
            .and_then(|(left, right)| Number::from_f64(left + right))
            .map(Value::Number)
            .unwrap_or(Value::Null);
    }

    if matches!(left, Value::String(_)) || matches!(right, Value::String(_)) {
        return Value::String(format!("{}{}", to_text(left), to_text(right)));
    }

    Value::Null
}

fn subtract(left: &Value, right: &Value) -> Value {
    if let (Value::Number(left), Value::Number(right)) = (left, right) {
        if let (Some(left), Some(right)) = (left.as_i64(), right.as_i64()) {
            return Value::Number(Number::from(left - right));
        }

        return left
            .as_f64()
            .zip(right.as_f64())
            .and_then(|(left, right)| Number::from_f64(left - right))
            .map(Value::Number)
            .unwrap_or(Value::Null);
    }

    Value::Null
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn eval(source: &str) -> Value {
        evaluate(
            &parse(source).unwrap(),
            &json!({"user": {"name": "Ada", "age": 36}}),
        )
    }

    #[test]
    fn evaluates_paths_and_literals() {
        assert_eq!(eval("user.name"), json!("Ada"));
        assert_eq!(eval("user.age"), json!(36));
        assert_eq!(eval("\"hello\" + \" world\""), json!("hello world"));
    }

    #[test]
    fn evaluates_boolean_logic_and_comparisons() {
        assert_eq!(eval("user.age >= 18 && user.name == \"Ada\""), json!(true));
        assert_eq!(eval("user.age < 18 || user.name == \"Bob\""), json!(false));
        assert_eq!(eval("!false"), json!(true));
    }

    #[test]
    fn missing_values_are_null() {
        assert_eq!(eval("missing.field"), Value::Null);
        assert!(!truthy(&Value::Null));
    }

    #[test]
    fn precedence_is_left_to_right_for_additive() {
        assert_eq!(eval("1 + 2 - 3"), json!(0));
    }
}
