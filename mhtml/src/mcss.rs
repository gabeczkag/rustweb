use std::collections::BTreeMap;

use crate::Error;

/// MCSS is a small CSS preprocessor: `$variables` plus normal CSS.
///
/// Variable declarations are collected from lines like `$sky: #7dd3fc;`.
/// Every `$name` reference in the remaining stylesheet is replaced before
/// the normal CSS minifier runs.
pub fn compile(source: &str) -> Result<String, Error> {
    let mut variables = BTreeMap::new();
    let mut body = String::with_capacity(source.len());

    for line in source.lines() {
        let trimmed = line.trim();

        if let Some(variable) = trimmed.strip_prefix('$') {
            let Some((name, value)) = variable.split_once(':') else {
                return Err(Error::Compile(format!(
                    "invalid MCSS variable declaration: {trimmed}"
                )));
            };

            let name = name.trim();
            let value = value.trim().trim_end_matches(';').trim();

            if name.is_empty() || value.is_empty() {
                return Err(Error::Compile(format!(
                    "invalid MCSS variable declaration: {trimmed}"
                )));
            }

            variables.insert(name.to_owned(), value.to_owned());
            continue;
        }

        body.push_str(line);
        body.push('\n');
    }

    let mut variables: Vec<_> = variables.into_iter().collect();
    variables.sort_by_key(|(name, _)| std::cmp::Reverse(name.len()));

    for (name, value) in variables {
        body = body.replace(&format!("${name}"), &value);
    }

    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_variables_longest_first() {
        let output = compile(
            "$sky: #7dd3fc;\n$sky-dark: #0284c7;\na { color: $sky; background: $sky-dark; }",
        )
        .unwrap();
        assert!(output.contains("color: #7dd3fc"));
        assert!(output.contains("background: #0284c7"));
        assert!(!output.contains('$'));
    }

    #[test]
    fn rejects_bad_variables() {
        assert!(compile("$broken").is_err());
    }
}
