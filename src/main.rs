use anyhow::{anyhow, Context, Result};
use clap::Clap;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::Command;
use toml;

mod jenkins;
mod utf8;

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

    Ok(run_jenkins(cfg, &opts.args)?)
}

fn run_jenkins(cfg: &jenkins::Server, args: &Vec<String>) -> Result<()> {
    /*
    let mut cmd = Command::new("java");
    cmd.arg("-jar")
        .arg("c:/Bin/jenkins-cli.jar")
        .args(&["-s", &cfg.url]);

    if let Some(proxy) = &cfg.proxy {
        cmd.args(&["-p", &proxy])
            .env("http_proxy", format!("http://{}", &proxy))
            .env("https_proxy", format!("http://{}", &proxy))
            .env("no_proxy", "");
    }
    cmd.arg("-noCertificateCheck")
        .args(&["-auth", &format!("{}:{}", cfg.username, cfg.password)]);

    cmd.args(args);
    let output = cmd.output()?;
    io::stdout().write_all(String::from_utf8_lossy(&output.stdout).as_bytes())?;
    io::stdout().write_all(String::from_utf8_lossy(&output.stderr).as_bytes())?;
    */

    let cli = jenkins::Cli::new(cfg.clone())?;
    cli.send(args.clone())?;

    Ok(())
}

fn read_file<P: AsRef<Path>>(filepath: P) -> Result<Config> {
    let content = fs::read_to_string(filepath)?;
    let cfg = toml::from_str(&content)?;
    Ok(cfg)
}
