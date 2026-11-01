use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;

use crate::Error;

/// Bundle an MScript entry and its relative imports into one classic script.
///
/// MScript is a small Node-flavoured module dialect for MHTML:
/// `import { x } from "./file.mscript"`, `export ...`, and relative
/// `require("./file.mscript")` are resolved at build time and wrapped in a
/// tiny module registry. Browser globals remain available; Node built-ins are
/// intentionally not emulated.
pub fn bundle(entry: &Path, root: &Path) -> Result<String, Error> {
    let root = fs::canonicalize(root)?;
    let entry = fs::canonicalize(entry)?;
    if !entry.starts_with(&root) {
        return Err(Error::Compile(
            "MScript entry is outside the project directory".to_owned(),
        ));
    }

    let mut modules = Vec::new();
    let mut visiting = HashSet::new();
    let mut done = HashSet::new();
    let entry_id = collect(&entry, &root, &mut modules, &mut visiting, &mut done)?;

    let mut output = String::from(
        "(()=>{const __m={},__c={};function __r(id){if(__c[id])return __c[id].exports;const m={exports:{}};__c[id]=m;__m[id](m,m.exports,__r);return m.exports}\n",
    );

    for (id, code) in modules {
        output.push_str(&format!(
            "__m[{}]=function(module,exports,__require){{\n{code}\n}};\n",
            json(&id)
        ));
    }

    output.push_str(&format!("__r({});\n}})();", json(&entry_id)));
    Ok(output)
}

fn collect(
    path: &Path,
    root: &Path,
    modules: &mut Vec<(String, String)>,
    visiting: &mut HashSet<String>,
    done: &mut HashSet<String>,
) -> Result<String, Error> {
    let path = fs::canonicalize(path)?;
    if !path.starts_with(root) {
        return Err(Error::Compile(
            "MScript module escapes the project directory".to_owned(),
        ));
    }

    let id = module_id(&path, root);
    if done.contains(&id) {
        return Ok(id);
    }
    if !visiting.insert(id.clone()) {
        return Err(Error::Compile(format!("MScript import cycle: {id}")));
    }

    let source = fs::read_to_string(&path)?;
    let (transformed, dependencies) = transform(&source, &path, root)?;

    for dependency in dependencies {
        collect(&dependency, root, modules, visiting, done)?;
    }

    visiting.remove(&id);
    done.insert(id.clone());
    modules.push((id.clone(), transformed));
    Ok(id)
}

fn transform(source: &str, current: &Path, root: &Path) -> Result<(String, Vec<PathBuf>), Error> {
    let mut output = String::with_capacity(source.len());
    let mut dependencies = Vec::new();
    let mut exported = Vec::new();

    for line in source.lines() {
        let trimmed = line.trim_start();

        if let Some(rest) = trimmed.strip_prefix("import ") {
            output.push_str(&transform_import(rest, current, root, &mut dependencies)?);
        } else if let Some(rest) = trimmed.strip_prefix("export ") {
            output.push_str(&transform_export(rest, &mut exported)?);
        } else {
            output.push_str(&replace_requires(line, current, root, &mut dependencies)?);
        }

        output.push('\n');
    }

    for name in exported {
        output.push_str(&format!("exports.{name} = {name};\n"));
    }

    Ok((output, dependencies))
}

fn transform_import(
    rest: &str,
    current: &Path,
    root: &Path,
    dependencies: &mut Vec<PathBuf>,
) -> Result<String, Error> {
    let rest = rest.trim();

    if rest.starts_with('"') || rest.starts_with('\'') {
        let spec = parse_spec(rest)?;
        let dependency = resolve(&spec, current, root)?;
        let id = module_id(&fs::canonicalize(&dependency)?, root);
        dependencies.push(dependency);
        return Ok(format!("__require({});", json(&id)));
    }

    let Some((clause, tail)) = rest.split_once(" from ") else {
        return Err(Error::Compile(format!(
            "invalid MScript import: import {rest}"
        )));
    };

    let spec = parse_spec(tail)?;
    let dependency = resolve(&spec, current, root)?;
    let id = module_id(&fs::canonicalize(&dependency)?, root);
    dependencies.push(dependency);
    let clause = clause.trim();

    let binding = if clause.starts_with('{') {
        let inner = clause
            .trim_start_matches('{')
            .trim_end_matches('}')
            .replace(" as ", ": ");
        format!("const {{ {inner} }} = __require({});", json(&id))
    } else if let Some(name) = clause.strip_prefix("* as ") {
        format!("const {} = __require({});", name.trim(), json(&id))
    } else {
        format!("const {} = __require({}).default;", clause, json(&id))
    };

    Ok(binding)
}

fn transform_export(rest: &str, exported: &mut Vec<String>) -> Result<String, Error> {
    let rest = rest.trim();

    if rest.starts_with('{') {
        let inner = rest
            .trim_start_matches('{')
            .trim_end_matches('}')
            .trim_end_matches(';');
        let mut output = String::new();

        for item in inner.split(',') {
            let item = item.trim();
            if item.is_empty() {
                continue;
            }
            let (local, public) = item
                .split_once(" as ")
                .map(|(local, public)| (local.trim(), public.trim()))
                .unwrap_or((item, item));
            output.push_str(&format!("exports.{public} = {local};"));
        }

        return Ok(output);
    }

    if let Some(value) = rest.strip_prefix("default ") {
        return Ok(format!(
            "exports.default = {};",
            value.trim_end_matches(';')
        ));
    }

    let declaration = rest.trim_end_matches(';');
    let (name, declaration) = exported_name(declaration)?;
    exported.push(name);
    Ok(declaration)
}

fn exported_name(declaration: &str) -> Result<(String, String), Error> {
    let after_keyword = if let Some(rest) = declaration.strip_prefix("const ") {
        Some(rest)
    } else if let Some(rest) = declaration.strip_prefix("let ") {
        Some(rest)
    } else if let Some(rest) = declaration.strip_prefix("var ") {
        Some(rest)
    } else if let Some(rest) = declaration.strip_prefix("function ") {
        Some(rest)
    } else if let Some(rest) = declaration.strip_prefix("async function ") {
        Some(rest)
    } else {
        declaration.strip_prefix("class ")
    };

    let Some(after_keyword) = after_keyword else {
        return Err(Error::Compile(format!(
            "unsupported MScript export: {declaration}"
        )));
    };

    let name: String = after_keyword
        .chars()
        .take_while(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '$'))
        .collect();

    if name.is_empty() {
        return Err(Error::Compile(format!(
            "cannot determine exported name: {declaration}"
        )));
    }

    Ok((name, declaration.to_owned()))
}

fn replace_requires(
    source: &str,
    current: &Path,
    root: &Path,
    dependencies: &mut Vec<PathBuf>,
) -> Result<String, Error> {
    static REQUIRE: OnceLock<Regex> = OnceLock::new();
    let regex = REQUIRE.get_or_init(|| {
        Regex::new(r#"require\(\s*["']([^"']+)["']\s*\)"#).expect("valid require regex")
    });

    let mut output = String::with_capacity(source.len());
    let mut last = 0usize;

    for captures in regex.captures_iter(source) {
        let Some(matched) = captures.get(0) else {
            continue;
        };
        output.push_str(&source[last..matched.start()]);

        let spec = &captures[1];
        let dependency = resolve(spec, current, root)?;
        let id = module_id(&fs::canonicalize(&dependency)?, root);
        dependencies.push(dependency);
        output.push_str(&format!("__require({})", json(&id)));
        last = matched.end();
    }

    output.push_str(&source[last..]);
    Ok(output)
}

fn resolve(spec: &str, current: &Path, root: &Path) -> Result<PathBuf, Error> {
    if !spec.starts_with('.') {
        return Err(Error::Compile(format!(
            "MScript only supports relative imports, got `{spec}`"
        )));
    }

    let base = current.parent().unwrap_or(root).join(spec);
    let candidates = [
        base.clone(),
        base.with_extension("mscript"),
        base.with_extension("js"),
        base.with_extension("mjs"),
    ];

    for candidate in candidates {
        if candidate.is_file() {
            let candidate = fs::canonicalize(candidate)?;
            if !candidate.starts_with(root) {
                return Err(Error::Compile(
                    "MScript import escapes the project directory".to_owned(),
                ));
            }
            return Ok(candidate);
        }
    }

    Err(Error::Compile(format!(
        "MScript module not found: {spec} (from {})",
        current.display()
    )))
}

fn parse_spec(value: &str) -> Result<String, Error> {
    let value = value
        .trim()
        .trim_end_matches(';')
        .trim()
        .trim_matches('"')
        .trim_matches('\'');

    if value.is_empty() {
        return Err(Error::Compile("empty MScript import path".to_owned()));
    }

    Ok(value.to_owned())
}

fn module_id(path: &Path, root: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let relative = relative.with_extension("");
    relative.to_string_lossy().replace('\\', "/")
}

fn json(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundles_imports_and_exports() {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir_all(directory.path().join("scripts")).unwrap();
        fs::write(
            directory.path().join("scripts/format.mscript"),
            "export function bytes(value) { return value + ' B'; }\n",
        )
        .unwrap();
        fs::write(
            directory.path().join("scripts/app.mscript"),
            "import { bytes } from './format.mscript';\nglobalThis.__size = bytes(2);\n",
        )
        .unwrap();

        let output = bundle(
            &directory.path().join("scripts/app.mscript"),
            directory.path(),
        )
        .unwrap();

        assert!(output.contains("__require(\"scripts/format\")"));
        assert!(output.contains("exports.bytes = bytes;"));
        assert!(output.contains("__r(\"scripts/app\")"));
        assert!(output.contains("globalThis.__size"));
    }

    #[test]
    fn rejects_non_relative_imports() {
        let directory = tempfile::tempdir().unwrap();
        fs::write(
            directory.path().join("app.mscript"),
            "import fs from 'node:fs';\n",
        )
        .unwrap();

        let error = bundle(&directory.path().join("app.mscript"), directory.path()).unwrap_err();
        assert!(matches!(error, Error::Compile(_)));
    }
}
