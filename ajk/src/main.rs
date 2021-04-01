use anyhow::{anyhow, Result};
use clap::Clap;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

mod jenkins;

type AResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Clap)]
#[clap(version = "1.0", author = "Guillaume Leroi")]
struct Opts {
    /// Select the jenkins instance to run against
    #[clap(short, long)]
    jenkins: Option<String>,
    /// path to config file, default to "~/.config/jk/jenkins.toml"
    #[clap(short, long)]
    config: Option<String>,
    args: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Config {
    default: String,
    #[serde(flatten)]
    servers: HashMap<String, jenkins::Server>,
}

#[tokio::main]
async fn main() -> AResult<()> {
    let opts = Opts::parse();
    let config = if let Some(ref path) = opts.config {
        Path::new(path).to_path_buf()
    } else {
        let mut home = dirs::home_dir().ok_or_else(|| anyhow!("no HOME dir found"))?;
        home.push(".config/jk/jenkins.toml");
        home
    };
    let config = read_file(config)?;

    let server = opts.jenkins.unwrap_or(config.default);
    let cfg = config
        .servers
        .get(&server)
        .ok_or_else(|| anyhow!("no server {} found", server))?;
    let exit_code = jenkins::run(cfg, &opts.args).await?;
    std::process::exit(exit_code);
}

fn read_file<P: AsRef<Path>>(filepath: P) -> Result<Config> {
    let content = fs::read_to_string(filepath)?;
    let cfg = toml::from_str(&content)?;
    Ok(cfg)
}
