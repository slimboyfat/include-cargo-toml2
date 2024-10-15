//! This crate provides a macro called [`include_toml!`] which parses properties of `Cargo.toml` at compile time.

extern crate proc_macro;
extern crate proc_macro2;
extern crate quote;
extern crate syn;
extern crate toml;

use crate::{
    proc_macro::TokenStream,
    proc_macro2::{Literal, Span as Span2, TokenStream as TokenStream2},
    quote::{quote, ToTokens},
    syn::{
        parse::{Parse, ParseBuffer},
        parse_macro_input,
        token::Dot,
        Error as SynError, Lit, LitBool,
    },
    toml::Value,
};
use std::env::var;
use std::fs::read_to_string;
use std::path::{Path, PathBuf};

/// Helper that stores either integer or string.
///
/// Used to create vector of indexing items in [`TomlIndex`].
enum Index {
    Int(usize),
    Str(String),
}

/// Struct that parses input of [`include_toml`].
///
/// Input should consist of either string literals or integers separated by dots.
struct TomlIndex(Vec<Index>);

impl Parse for TomlIndex {
    fn parse(input: &ParseBuffer) -> Result<Self, SynError> {
        let mut another_one = true;
        let mut index = Vec::new();
        while another_one {
            index.push(match input.parse::<Lit>() {
                Ok(lit) => match lit {
                    Lit::Str(lit_str) => Index::Str(lit_str.value()),
                    Lit::Int(lit_int) => Index::Int(
                        lit_int
                            .base10_digits()
                            .parse()
                            .expect("Cannot parse literal integer"),
                    ),
                    _ => return Err(SynError::new(input.span(), "Unsupported literal")),
                },
                Err(e) => {
                    return Err(SynError::new(
                        input.span(),
                        format!("Cannot parse index item: {}", e),
                    ))
                }
            });
            if let Err(_) = input.parse::<Dot>() {
                another_one = false;
            }
        }
        Ok(Self(index))
    }
}

/// Converts any TOML value to valid Rust types.
fn translate(input: Value) -> TokenStream2 {
    match input {
        Value::String(s) => Lit::new(Literal::string(&s)).to_token_stream().into(),
        Value::Integer(i) => Lit::new(Literal::i64_suffixed(i)).to_token_stream().into(),
        Value::Float(f) => Lit::new(Literal::f64_suffixed(f)).to_token_stream().into(),
        Value::Datetime(d) => Lit::new(Literal::string(&d.to_string()))
            .to_token_stream()
            .into(),
        Value::Boolean(b) => Lit::Bool(LitBool::new(b, Span2::call_site()))
            .to_token_stream()
            .into(),
        Value::Array(a) => {
            let mut ts = TokenStream2::new();
            for value in a {
                let v = translate(value);
                ts.extend(quote! (#v,));
            }
            quote! ((#ts))
        }
        Value::Table(t) => {
            let mut ts = TokenStream2::new();
            for (key, value) in t {
                let v = translate(value);
                ts.extend(quote! ((#key, #v)));
            }
            quote! ((#ts))
        }
    }
}

/// Parse `Cargo.toml` at compile time.
///
/// # TOML to Rust conversion
///
/// - TOML [string](Value::String) -> Rust [`&str`]
/// - TOML [integer](Value::Integer) -> Rust [`i64`]
/// - TOML [float](Value::Float) -> Rust [`f64`]
/// - TOML [boolean](Value::Boolean) -> Rust [`bool`]
/// - TOML [datetime](Value::Datetime) -> Rust [`&str`]
/// - TOML [array](Value::Array) -> Rust tuple \
///     TOML arrays can hold different types, Rust [`Vec`]s can't.
/// - TOML [table](Value::Table) -> Rust tuple \
///     TOML tables can hold different types, Rust [`Vec`]s can't.
///
/// # Example
///
/// Keys to index `Cargo.toml` are parsed as string literals and array / table indexes are parsed as integer literals:
///
/// ```rust
/// use include_cargo_toml2::include_toml;
///
/// assert_eq!(
///     include_toml!("package"."version"),
///     "0.3.1"
/// );
/// assert_eq!(
///     include_toml!("package"."name"),
///     "include-cargo-toml2"
/// );
/// // indexing array with literal 2
/// assert_eq!(
///     include_toml!("package"."keywords".2),
///     "Cargo-toml"
/// );
/// assert_eq!(
///     include_toml!("lib"."proc-macro"),
///     true
/// );
/// ```
///
/// Because TOML's arrays and tables do not work like [`Vec`] and [`HashMap`](std::collections::HashMap), tuples are used.
///
/// ```rust
/// use include_cargo_toml2::include_toml;
///
/// assert_eq!(
///     include_toml!("package"."keywords"),
///     ("macro", "version", "Cargo-toml", "compile-time", "parse")
/// );
/// ```
///
/// Leading or trailing dots are not allowed:
///
/// ```rust,compile_fail
/// use include_cargo_toml::include_toml;
///
/// let this_fails = include_toml!(."package"."name");
/// let this_fails_too = include_toml!("package"."name".);
/// ```
#[proc_macro]
pub fn include_toml(input: TokenStream) -> TokenStream {
    let dir = var("CARGO_MANIFEST_DIR").expect("Environment variable CARGO_MANIFEST_DIR not set!");
    let path = Path::new(&dir).join("Cargo.toml");

    let cargo_toml = parse(&path);
    let index: TomlIndex = parse_macro_input!(input);
    let result = lookup(index, cargo_toml);

    translate(result).into()
}

fn parse(path: &PathBuf) -> Value {
    let content = read_to_string(path).expect("Cannot read Cargo.toml");
    content.parse::<Value>().expect("Cannot parse Cargo.toml to json")
}

fn lookup(index: TomlIndex, mut toml: Value) -> Value {
    for item in index.0 {
        match item {
            Index::Int(index) => {
                toml = toml[index].clone();
            }
            Index::Str(index) => {
                toml = toml[index].clone();
            }
        }
    }
    toml
}

#[cfg(test)]
mod tests {
    use crate::{lookup, parse};
    use std::env::var;
    use std::path::Path;
    use toml::Value;

    #[test]
    fn should_parse_when_cargo_toml_is_valid() {
        let dir = var("CARGO_MANIFEST_DIR").expect("Environment variable CARGO_MANIFEST_DIR must be set!");

        let path = Path::new(&dir).join("Cargo.toml");
        println!("{}", dir);
        let toml = parse(&path);

        assert_eq!("include-cargo-toml2", toml["package"]["name"].as_str().unwrap());
    }

    #[test]
    fn should_fetch_attribute_when_cargo_toml_is_given() {
        let cargo_toml = r#"
        [package]
        edition="2021"
        version="0.1.0"
        "#;

        let toml: Value = cargo_toml.parse::<Value>().expect("Cannot parse Cargo.toml");
        let index = syn::parse_str(r#""package"."version""#).unwrap();

        let result = lookup(index, toml);

        assert_eq!("0.1.0", result.as_str().unwrap());
    }

    #[test]
    fn should_fetch_custom_attribute_when_cargo_toml_is_given() {
        let cargo_toml = r#"
        [package]
        edition="2021"
        [package.metadata.deb]
        revision=4
        "#;

        let toml: Value = cargo_toml.parse::<Value>().expect("Cannot parse Cargo.toml");
        let index = syn::parse_str(r#""package"."metadata"."deb"."revision""#).unwrap();

        let result = lookup(index, toml);

        assert_eq!(4, result.as_integer().unwrap());
    }
}
