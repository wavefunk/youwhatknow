use std::path::Path;

use streaming_iterator::StreamingIterator as _;
use tree_sitter::{Language, Parser, Query, QueryCursor};

/// Extract public symbols from a source file based on its extension.
pub fn extract_symbols(path: &Path, source: &[u8]) -> Vec<String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();

    match ext {
        "rs" => extract_rust_symbols(source),
        "ts" | "tsx" => extract_typescript_symbols(source),
        "js" | "jsx" => extract_javascript_symbols(source),
        "py" => extract_python_symbols(source),
        "go" => extract_go_symbols(source),
        _ => Vec::new(),
    }
}

fn extract_rust_symbols(source: &[u8]) -> Vec<String> {
    let lang: Language = tree_sitter_rust::LANGUAGE.into();
    // Match pub items at any level
    let query_src = r#"
        (function_item
            (visibility_modifier) @vis
            name: (identifier) @name)

        (struct_item
            (visibility_modifier) @vis
            name: (type_identifier) @name)

        (enum_item
            (visibility_modifier) @vis
            name: (type_identifier) @name)

        (trait_item
            (visibility_modifier) @vis
            name: (type_identifier) @name)

        (type_item
            (visibility_modifier) @vis
            name: (type_identifier) @name)
    "#;
    run_query(lang, query_src, source, "name")
}

fn extract_typescript_symbols(source: &[u8]) -> Vec<String> {
    let lang: Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
    // TS export declarations
    let query_src = r#"
        (export_statement
            declaration: (function_declaration
                name: (identifier) @name))

        (export_statement
            declaration: (class_declaration
                name: (type_identifier) @name))

        (export_statement
            declaration: (interface_declaration
                name: (type_identifier) @name))

        (export_statement
            declaration: (type_alias_declaration
                name: (type_identifier) @name))

        (export_statement
            declaration: (lexical_declaration
                (variable_declarator
                    name: (identifier) @name)))
    "#;
    run_query(lang, query_src, source, "name")
}

fn extract_javascript_symbols(source: &[u8]) -> Vec<String> {
    let lang: Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
    // JS uses same grammar patterns via TS parser for simplicity
    let query_src = r#"
        (export_statement
            declaration: (function_declaration
                name: (identifier) @name))

        (export_statement
            declaration: (class_declaration
                name: (identifier) @name))

        (export_statement
            declaration: (lexical_declaration
                (variable_declarator
                    name: (identifier) @name)))
    "#;
    run_query(lang, query_src, source, "name")
}

fn extract_python_symbols(source: &[u8]) -> Vec<String> {
    let lang: Language = tree_sitter_python::LANGUAGE.into();
    // Top-level function and class definitions
    let query_src = r#"
        (module
            (function_definition
                name: (identifier) @name))

        (module
            (class_definition
                name: (identifier) @name))
    "#;
    run_query(lang, query_src, source, "name")
}

fn extract_go_symbols(source: &[u8]) -> Vec<String> {
    let lang: Language = tree_sitter_go::LANGUAGE.into();
    let query_src = r#"
        (function_declaration
            name: (identifier) @name)

        (method_declaration
            name: (field_identifier) @name)

        (type_declaration
            (type_spec
                name: (type_identifier) @name))
    "#;
    // Go exports are capitalized — filter after extraction
    let symbols = run_query(lang, query_src, source, "name");
    symbols
        .into_iter()
        .filter(|s| {
            s.starts_with(|c: char| c.is_uppercase())
        })
        .collect()
}

/// Run a tree-sitter query and extract capture values for the given capture name.
fn run_query(
    lang: Language,
    query_src: &str,
    source: &[u8],
    capture_name: &str,
) -> Vec<String> {
    let mut parser = Parser::new();
    if parser.set_language(&lang).is_err() {
        return Vec::new();
    }

    let Some(tree) = parser.parse(source, None) else {
        return Vec::new();
    };

    let Ok(query) = Query::new(&lang, query_src) else {
        return Vec::new();
    };

    let capture_idx = query
        .capture_names()
        .iter()
        .position(|n| *n == capture_name);
    let Some(idx) = capture_idx else {
        return Vec::new();
    };
    let idx = idx as u32;

    let mut cursor = QueryCursor::new();
    let mut symbols = Vec::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source);

    while let Some(m) = matches.next() {
        for capture in m.captures {
            if capture.index == idx
                && let Ok(text) = capture.node.utf8_text(source)
            {
                let name = text.to_owned();
                if !symbols.contains(&name) {
                    symbols.push(name);
                }
            }
        }
    }
    symbols
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn rust_extracts_pub_items() {
        let source = br#"
pub fn hello() {}
fn private() {}
pub struct Foo;
pub enum Bar { A, B }
pub trait Baz {}
pub type Alias = u32;
struct Hidden;
"#;
        let symbols = extract_symbols(Path::new("lib.rs"), source);
        assert_eq!(symbols, vec!["hello", "Foo", "Bar", "Baz", "Alias"]);
    }

    #[test]
    fn rust_no_private_items() {
        let source = br#"
fn private() {}
struct Hidden;
enum Secret { X }
"#;
        let symbols = extract_symbols(Path::new("lib.rs"), source);
        assert!(symbols.is_empty());
    }

    #[test]
    fn typescript_extracts_exports() {
        let source = br#"
export function greet(name: string): string { return name; }
export class UserService {}
export interface Config { port: number; }
export type ID = string;
export const VERSION = "1.0";
function internal() {}
"#;
        let symbols = extract_symbols(Path::new("index.ts"), source);
        assert!(symbols.contains(&"greet".to_owned()));
        assert!(symbols.contains(&"UserService".to_owned()));
        assert!(symbols.contains(&"Config".to_owned()));
        assert!(symbols.contains(&"ID".to_owned()));
        assert!(symbols.contains(&"VERSION".to_owned()));
        assert!(!symbols.contains(&"internal".to_owned()));
    }

    #[test]
    fn python_extracts_top_level() {
        let source = br#"
def hello():
    pass

class MyClass:
    def method(self):
        pass

def world():
    pass
"#;
        let symbols = extract_symbols(Path::new("mod.py"), source);
        assert_eq!(symbols, vec!["hello", "MyClass", "world"]);
    }

    #[test]
    fn go_extracts_exported_only() {
        let source = br#"
package main

func Hello() {}
func private() {}

type Config struct {}
type internal struct {}

func (c *Config) Method() {}
func (c *Config) helper() {}
"#;
        let symbols = extract_symbols(Path::new("main.go"), source);
        assert!(symbols.contains(&"Hello".to_owned()));
        assert!(symbols.contains(&"Config".to_owned()));
        assert!(symbols.contains(&"Method".to_owned()));
        assert!(!symbols.contains(&"private".to_owned()));
        assert!(!symbols.contains(&"internal".to_owned()));
        assert!(!symbols.contains(&"helper".to_owned()));
    }

    #[test]
    fn unknown_extension_returns_empty() {
        let symbols = extract_symbols(Path::new("data.csv"), b"a,b,c");
        assert!(symbols.is_empty());
    }
}
