use anyhow::{anyhow, Result};
use clap::Clap;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use toml;

mod jenkins;

#[derive(Clap)]
#[clap(version = "0.1", author = "LinkyPilot")]
struct Opts {
    /// Select the jenkins instance to run against
    #[clap(short, long)]
    jenkins: Option<String>,
    args: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Config {
    default: String,
    #[serde(flatten)]
    servers: HashMap<String, jenkins::Server>,
}

fn main() -> Result<()> {
    let opts = Opts::parse();
    let config = read_file("jenkins.toml")?;

    let server = opts.jenkins.unwrap_or(config.default);
    let cfg = config
        .servers
        .get(&server)
        .ok_or(anyhow!("no server {} found", server))?;

    let code = run_jenkins(cfg, &opts.args)?;
    std::process::exit(code);
}

fn run_jenkins(cfg: &jenkins::Server, args: &Vec<String>) -> Result<i32> {
    let cli = jenkins::Cli::new(cfg.clone())?;
    let mut resp = cli.send(args.clone())?;
    std::io::copy(resp.output(), &mut std::io::stdout())?;
    resp.wait_exit_code()
}

fn read_file<P: AsRef<Path>>(filepath: P) -> Result<Config> {
    let content = fs::read_to_string(filepath)?;
    let cfg = toml::from_str(&content)?;
    Ok(cfg)
}
