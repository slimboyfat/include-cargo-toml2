use include_cargo_toml2::include_toml;

pub const CRATE_NAME: &str = include_toml!("package"."name");
