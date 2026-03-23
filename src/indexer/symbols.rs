use std::path::Path;

use streaming_iterator::StreamingIterator as _;
use tree_sitter::{Language, Node, Parser, Query, QueryCursor, Tree};

use crate::types::{FileAnalysis, LineRange};

/// Analyze a source file: extract symbols, line ranges, and line count.
pub fn analyze_file(path: &Path, source: &[u8]) -> FileAnalysis {
    let line_count = count_lines(source);

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();

    match ext {
        "rs" => analyze_rust(source, line_count),
        "ts" | "tsx" => FileAnalysis {
            symbols: extract_typescript_symbols(source),
            line_ranges: Vec::new(),
            line_count,
        },
        "js" | "jsx" => FileAnalysis {
            symbols: extract_javascript_symbols(source),
            line_ranges: Vec::new(),
            line_count,
        },
        "py" => FileAnalysis {
            symbols: extract_python_symbols(source),
            line_ranges: Vec::new(),
            line_count,
        },
        "go" => FileAnalysis {
            symbols: extract_go_symbols(source),
            line_ranges: Vec::new(),
            line_count,
        },
        _ => FileAnalysis {
            symbols: Vec::new(),
            line_ranges: Vec::new(),
            line_count,
        },
    }
}

/// Thin wrapper so callers that only need symbols still work.
pub fn extract_symbols(path: &Path, source: &[u8]) -> Vec<String> {
    analyze_file(path, source).symbols
}

// ── Rust analysis (single parse, two extractions) ──

fn analyze_rust(source: &[u8], line_count: u32) -> FileAnalysis {
    let lang: Language = tree_sitter_rust::LANGUAGE.into();
    let mut parser = Parser::new();
    if parser.set_language(&lang).is_err() {
        return FileAnalysis {
            symbols: Vec::new(),
            line_ranges: Vec::new(),
            line_count,
        };
    }

    let Some(tree) = parser.parse(source, None) else {
        return FileAnalysis {
            symbols: Vec::new(),
            line_ranges: Vec::new(),
            line_count,
        };
    };

    let symbols = extract_rust_symbols_from_tree(&lang, &tree, source);
    let line_ranges = extract_rust_line_ranges(&tree, source);

    FileAnalysis {
        symbols,
        line_ranges,
        line_count,
    }
}

/// Extract public Rust symbol names from an already-parsed tree.
fn extract_rust_symbols_from_tree(lang: &Language, tree: &Tree, source: &[u8]) -> Vec<String> {
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

    let Ok(query) = Query::new(lang, query_src) else {
        return Vec::new();
    };

    let capture_idx = query
        .capture_names()
        .iter()
        .position(|n| *n == "name");
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

/// Walk top-level AST children and produce labelled line ranges.
fn extract_rust_line_ranges(tree: &Tree, source: &[u8]) -> Vec<LineRange> {
    let root = tree.root_node();
    let mut cursor = root.walk();
    let children: Vec<Node<'_>> = root.children(&mut cursor).collect();

    let mut ranges: Vec<LineRange> = Vec::new();

    // Track consecutive use_declaration nodes to collapse them
    let mut use_start: Option<u32> = None;
    let mut use_end: Option<u32> = None;

    for child in &children {
        let kind = child.kind();

        if kind == "use_declaration" {
            let start = child.start_position().row as u32 + 1;
            let end = child.end_position().row as u32 + 1;
            match use_start {
                None => {
                    use_start = Some(start);
                    use_end = Some(end);
                }
                Some(_) => {
                    use_end = Some(end);
                }
            }
            continue;
        }

        // Flush any pending use block before processing a non-use node
        if let (Some(start), Some(end)) = (use_start, use_end) {
            ranges.push(LineRange {
                start,
                end,
                label: "Imports".to_owned(),
            });
            use_start = None;
            use_end = None;
        }

        let start = child.start_position().row as u32 + 1;
        let end = child.end_position().row as u32 + 1;

        let label = match kind {
            "function_item" => {
                let name = node_name(child, source).unwrap_or("?");
                format!("{name}() function")
            }
            "struct_item" => {
                let name = node_name(child, source).unwrap_or("?");
                format!("{name} struct")
            }
            "enum_item" => {
                let name = node_name(child, source).unwrap_or("?");
                format!("{name} enum")
            }
            "impl_item" => {
                let type_name = node_type_name(child, source).unwrap_or("?");
                format!("impl {type_name}")
            }
            "trait_item" => {
                let name = node_name(child, source).unwrap_or("?");
                format!("trait {name}")
            }
            "mod_item" => {
                let name = node_name(child, source).unwrap_or("?");
                if name == "tests" {
                    "Tests".to_owned()
                } else {
                    format!("mod {name}")
                }
            }
            "type_item" => {
                let name = node_name(child, source).unwrap_or("?");
                format!("type {name}")
            }
            // Skip comments, attributes, blank lines, etc.
            _ => continue,
        };

        ranges.push(LineRange { start, end, label });
    }

    // Flush trailing use block
    if let (Some(start), Some(end)) = (use_start, use_end) {
        ranges.push(LineRange {
            start,
            end,
            label: "Imports".to_owned(),
        });
    }

    ranges
}

/// Extract the `name` field from a tree-sitter node.
fn node_name<'a>(node: &Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    let name_node = node.child_by_field_name("name")?;
    name_node.utf8_text(source).ok()
}

/// Extract the type name from an impl_item node.
/// In `impl Foo { ... }`, the type is the `type` field.
fn node_type_name<'a>(node: &Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    let type_node = node.child_by_field_name("type")?;
    type_node.utf8_text(source).ok()
}

// ── Line counting ──

fn count_lines(source: &[u8]) -> u32 {
    if source.is_empty() {
        return 0;
    }
    let newlines = bytecount::count(source, b'\n');
    if source.last() == Some(&b'\n') {
        newlines as u32
    } else {
        newlines as u32 + 1
    }
}

// ── Non-Rust language extractors (unchanged, use run_query) ──

fn extract_typescript_symbols(source: &[u8]) -> Vec<String> {
    let lang: Language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
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

    // ── New analyze_file tests ──

    #[test]
    fn rust_analyze_file_returns_line_ranges() {
        let source = br#"use std::path::Path;
use std::io;

pub fn hello() {
    println!("hello");
}

fn private() {}

pub struct Foo {
    x: u32,
}

pub enum Bar {
    A,
    B,
}

impl Foo {
    pub fn new() -> Self {
        Self { x: 0 }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {}
}
"#;
        let analysis = analyze_file(Path::new("lib.rs"), source);

        // Symbols should still work
        assert!(analysis.symbols.contains(&"hello".to_owned()));
        assert!(analysis.symbols.contains(&"Foo".to_owned()));
        assert!(analysis.symbols.contains(&"Bar".to_owned()));

        // Line count
        assert_eq!(analysis.line_count, 29);

        // Line ranges should have entries
        assert!(!analysis.line_ranges.is_empty());

        // Check we got an imports range
        let imports = analysis.line_ranges.iter().find(|r| r.label == "Imports");
        assert!(imports.is_some(), "should have Imports range");

        // Check we got a function range
        let hello_fn = analysis
            .line_ranges
            .iter()
            .find(|r| r.label == "hello() function");
        assert!(hello_fn.is_some(), "should have hello() function range");

        // Check struct
        let foo = analysis
            .line_ranges
            .iter()
            .find(|r| r.label == "Foo struct");
        assert!(foo.is_some(), "should have Foo struct range");

        // Check impl
        let impl_foo = analysis
            .line_ranges
            .iter()
            .find(|r| r.label == "impl Foo");
        assert!(impl_foo.is_some(), "should have impl Foo range");

        // Check tests module
        let tests = analysis.line_ranges.iter().find(|r| r.label == "Tests");
        assert!(tests.is_some(), "should have Tests range");
    }

    #[test]
    fn rust_consecutive_use_collapsed() {
        let source = br#"use std::path::Path;
use std::io;
use std::collections::HashMap;

pub fn main() {}
"#;
        let analysis = analyze_file(Path::new("main.rs"), source);
        let imports: Vec<_> = analysis
            .line_ranges
            .iter()
            .filter(|r| r.label == "Imports")
            .collect();
        assert_eq!(
            imports.len(),
            1,
            "consecutive use statements should collapse to one Imports range"
        );
        assert_eq!(imports[0].start, 1);
        assert_eq!(imports[0].end, 3);
    }

    #[test]
    fn unsupported_language_returns_empty_ranges() {
        let analysis = analyze_file(Path::new("data.csv"), b"a,b,c\n1,2,3\n");
        assert!(analysis.symbols.is_empty());
        assert!(analysis.line_ranges.is_empty());
        assert_eq!(analysis.line_count, 2);
    }

    #[test]
    fn analyze_preserves_existing_symbol_extraction() {
        let source = br#"
pub fn hello() {}
fn private() {}
pub struct Foo;
pub enum Bar { A, B }
pub trait Baz {}
pub type Alias = u32;
struct Hidden;
"#;
        let analysis = analyze_file(Path::new("lib.rs"), source);
        assert_eq!(
            analysis.symbols,
            vec!["hello", "Foo", "Bar", "Baz", "Alias"]
        );
    }
}
