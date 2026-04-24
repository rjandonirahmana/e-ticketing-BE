use anyhow::{Ok, Result};

mod config;
mod models;
mod utils;
fn main() -> Result<()> {
    let config = config::config::AppConfig::from_env();
    println!("Hello, world!");

    Ok(())
}
