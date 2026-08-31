#![forbid(unsafe_code)]

mod app;
mod arguments;
mod auth;
mod config;
mod error;
mod live_arguments;
mod live_commands;
mod live_error;

use std::env;

#[tokio::main]
async fn main() {
    if let Err(error) = app::run(env::args().skip(1)).await {
        eprintln!("footballctl failed: {error}");
        std::process::exit(1);
    }
}
