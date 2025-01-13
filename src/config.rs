use clap::Parser;
use std::fs;

/// Real world application
#[derive(Parser, Debug)]
pub struct Config {
    /// Database URL
    #[arg(long, env)]
    pub database_url: String,
    /// RSA Private Key
    #[arg(long, env, value_parser = load_key)]
    pub rsa_private_key: String,
    /// RSA Public Key
    #[arg(long, env, value_parser = load_key)]
    pub rsa_public_key: String,
}

fn load_key(value: &str) -> std::io::Result<String> {
    fs::read_to_string(value)
}