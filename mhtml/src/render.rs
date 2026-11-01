use std::borrow::Cow;

use serde_json::{json, Map, Value};

use crate::ast::{Element, Node};
use crate::expr::{evaluate, to_text, truthy};

const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// Render a node tree into HTML.
///
/// Literal template text is trusted and emitted as written; interpolated
/// values are escaped. `minify` collapses runs of whitespace outside
/// `pre`/`textarea`.
pub fn render(nodes: &[Node], context: &Value, output: &mut String, minify: bool) {
    render_nodes(nodes, context, output, minify, false);
}

fn render_nodes(
    nodes: &[Node],
    context: &Value,
    output: &mut String,
    minify: bool,
    preserve_whitespace: bool,
) {
    for (index, node) in nodes.iter().enumerate() {
        if minify
            && !preserve_whitespace
            && is_whitespace_only(node)
            && !space_is_meaningful(nodes, index)
        {
            continue;
        }

        render_node(node, context, output, minify, preserve_whitespace);
    }
}

fn is_whitespace_only(node: &Node) -> bool {
    matches!(node, Node::Text(text) if text.trim().is_empty())
}

fn space_is_meaningful(nodes: &[Node], index: usize) -> bool {
    let previous = nodes[..index]
        .iter()
        .rev()
        .find(|node| !is_whitespace_only(node));
    let next = nodes[index + 1..]
        .iter()
        .find(|node| !is_whitespace_only(node));

    previous.is_some_and(is_inline_node) || next.is_some_and(is_inline_node)
}

fn is_inline_node(node: &Node) -> bool {
    match node {
        Node::Interp(_) | Node::RawInterp(_) | Node::Text(_) => true,
        Node::Element(element) => is_inline_tag(&element.tag),
        Node::Raw(_) => false,
        Node::If { .. } | Node::For { .. } => true,
    }
}

fn is_inline_tag(tag: &str) -> bool {
    matches!(
        tag.to_ascii_lowercase().as_str(),
        "a" | "span"
            | "strong"
            | "em"
            | "b"
            | "i"
            | "code"
            | "small"
            | "label"
            | "button"
            | "img"
            | "input"
            | "select"
            | "textarea"
            | "br"
            | "svg"
            | "path"
    )
}

fn render_node(
    node: &Node,
    context: &Value,
    output: &mut String,
    minify: bool,
    preserve_whitespace: bool,
) {
    match node {
        Node::Text(text) => {
            if minify && !preserve_whitespace {
                output.push_str(&collapse_whitespace(text));
            } else {
                output.push_str(text);
            }
        }
        Node::Raw(raw) => output.push_str(raw),
        Node::Interp(expression) => {
            let value = evaluate(expression, context);
            escape_text(output, &to_text(&value));
        }
        Node::RawInterp(expression) => {
            let value = evaluate(expression, context);
            output.push_str(&to_text(&value));
        }
        Node::Element(element) => {
            render_element(element, context, output, minify, preserve_whitespace)
        }
        Node::If {
            condition,
            then_branch,
            else_branch,
        } => {
            if truthy(&evaluate(condition, context)) {
                render_nodes(then_branch, context, output, minify, preserve_whitespace);
            } else {
                render_nodes(else_branch, context, output, minify, preserve_whitespace);
            }
        }
        Node::For {
            variable,
            iterable,
            body,
        } => {
            let value = evaluate(iterable, context);

            match value {
                Value::Array(items) => {
                    let last = items.len().saturating_sub(1);
                    for (index, item) in items.iter().enumerate() {
                        let loop_value = json!({
                            "index": index,
                            "index1": index + 1,
                            "first": index == 0,
                            "last": index == last,
                        });
                        let child = with_binding(context, variable, item, loop_value);
                        render_nodes(body, &child, output, minify, preserve_whitespace);
                    }
                }
                Value::Object(items) => {
                    let last = items.len().saturating_sub(1);
                    for (index, (key, item)) in items.iter().enumerate() {
                        let loop_value = json!({
                            "index": index,
                            "index1": index + 1,
                            "first": index == 0,
                            "last": index == last,
                            "key": key,
                        });
                        let child = with_binding(context, variable, item, loop_value);
                        render_nodes(body, &child, output, minify, preserve_whitespace);
                    }
                }
                _ => {}
            }
        }
    }
}

fn render_element(
    element: &Element,
    context: &Value,
    output: &mut String,
    minify: bool,
    preserve_parent: bool,
) {
    output.push('<');
    output.push_str(&element.tag);

    for attribute in &element.attrs {
        output.push(' ');
        output.push_str(&attribute.name);

        if let Some(value) = &attribute.value {
            output.push_str("=\"");
            for node in value {
                match node {
                    Node::Text(text) => output.push_str(text),
                    Node::Interp(expression) => {
                        let value = evaluate(expression, context);
                        escape_attribute(output, &to_text(&value));
                    }
                    Node::Raw(raw) => output.push_str(raw),
                    _ => {}
                }
            }
            output.push('"');
        }
    }

    if element.self_closing {
        output.push_str(" />");
        return;
    }

    if is_void(&element.tag) {
        output.push('>');
        return;
    }

    output.push('>');

    let preserve_whitespace = preserve_parent || is_preserve_whitespace(&element.tag);
    let raw_text = is_raw_text(&element.tag);

    if raw_text {
        for child in &element.children {
            if let Node::Raw(raw) = child {
                output.push_str(raw);
            }
        }
    } else {
        render_nodes(
            &element.children,
            context,
            output,
            minify,
            preserve_whitespace,
        );
    }

    output.push_str("</");
    output.push_str(&element.tag);
    output.push('>');
}

fn with_binding(context: &Value, variable: &str, value: &Value, loop_value: Value) -> Value {
    let mut map = match context {
        Value::Object(map) => map.clone(),
        _ => Map::new(),
    };

    map.insert(variable.to_owned(), value.clone());
    map.insert("loop".to_owned(), loop_value);
    Value::Object(map)
}

fn collapse_whitespace(text: &str) -> Cow<'_, str> {
    if !text.chars().any(char::is_whitespace) {
        return Cow::Borrowed(text);
    }

    let mut output = String::with_capacity(text.len());
    let mut in_whitespace = false;

    for character in text.chars() {
        if character.is_whitespace() {
            if !in_whitespace {
                output.push(' ');
                in_whitespace = true;
            }
        } else {
            output.push(character);
            in_whitespace = false;
        }
    }

    Cow::Owned(output)
}

fn escape_text(output: &mut String, text: &str) {
    for character in text.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            _ => output.push(character),
        }
    }
}

fn escape_attribute(output: &mut String, text: &str) {
    for character in text.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#39;"),
            _ => output.push(character),
        }
    }
}

fn is_void(tag: &str) -> bool {
    VOID_ELEMENTS.contains(&tag.to_ascii_lowercase().as_str())
}

fn is_raw_text(tag: &str) -> bool {
    matches!(tag.to_ascii_lowercase().as_str(), "script" | "style")
}

fn is_preserve_whitespace(tag: &str) -> bool {
    matches!(tag.to_ascii_lowercase().as_str(), "pre" | "textarea")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;
    use serde_json::json;

    fn render_str(template: &str, context: Value) -> String {
        let nodes = parse(template).unwrap();
        let mut output = String::new();
        render(&nodes, &context, &mut output, true);
        output
    }

    #[test]
    fn escapes_interpolated_values() {
        let html = render_str("<p>{{ name }}</p>", json!({"name": "<script>"}));
        assert_eq!(html, "<p>&lt;script&gt;</p>");
    }

    #[test]
    fn renders_trusted_raw_html() {
        let html = render_str("<div>{{{ html }}}</div>", json!({"html": "<b>ok</b>"}));
        assert_eq!(html, "<div><b>ok</b></div>");
    }

    #[test]
    fn renders_if_and_for() {
        let template = r#"@if show {<p>{{ title }}</p>} @else {<span>hidden</span>}
            @for item in items {<li>{{ item }}</li>}"#;
        let html = render_str(
            template,
            json!({"show": true, "title": "Hello", "items": ["a", "b"]}),
        );
        assert!(html.contains("<p>Hello</p>"));
        assert!(html.contains("<li>a</li><li>b</li>"));
        assert!(!html.contains("hidden"));
    }

    #[test]
    fn preserves_pre_whitespace() {
        let html = render_str(
            "<pre>line 1\n  line 2 {{ value }}</pre>",
            json!({"value": "<x>"}),
        );
        assert_eq!(html, "<pre>line 1\n  line 2 &lt;x&gt;</pre>");
    }

    #[test]
    fn preserves_nested_code_inside_pre() {
        let html = render_str("<pre><code>line 1\n  line 2</code></pre>", json!({}));
        assert_eq!(html, "<pre><code>line 1\n  line 2</code></pre>");
    }

    #[test]
    fn collapses_whitespace_in_normal_text() {
        let html = render_str("<p>hello\n\n   world</p>", json!({}));
        assert_eq!(html, "<p>hello world</p>");
    }
}
