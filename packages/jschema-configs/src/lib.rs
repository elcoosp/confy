#![feature(doc_cfg)]
#![doc = document_features::document_features!()]
#[cfg(feature = "pyproject")]
/// `pyproject.toml` [Config](confique::Config) feature enablable, entry point at [pyproject `ConfigRoot`](pyproject::ConfigRoot)
pub mod pyproject {
    include!(concat!(env!("OUT_DIR"), "/pyproject.rs"));
}
#[cfg(feature = "deno")]
/// `deno.json` [Config](confique::Config) feature enablable, entry point at [deno `ConfigRoot`](deno::ConfigRoot)
pub mod deno {
    include!(concat!(env!("OUT_DIR"), "/deno.rs"));
}
#[cfg(feature = "package")]
/// `package.json` [Config](confique::Config) feature enablable, entry point at [package `ConfigRoot`](package::ConfigRoot)
pub mod package {
    include!(concat!(env!("OUT_DIR"), "/package.rs"));
}
#[cfg(feature = "cargo")]
/// `Cargo.toml` [Config](confique::Config) feature enablable, entry point at [cargo `ConfigRoot`](cargo::ConfigRoot)

pub mod cargo {
    include!(concat!(env!("OUT_DIR"), "/cargo.rs"));
}
