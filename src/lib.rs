#![forbid(unsafe_code)]

mod cli;
mod config;
mod engine;
mod fingerprint;
mod minimize;
mod model;
mod oracle;
mod platform;
mod redaction;
mod report;
mod runner;
mod scenarios;
mod workspace;

pub use cli::run_cli;
