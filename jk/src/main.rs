use anyhow::{anyhow, Result};
use bytes::{Buf, BufMut, BytesMut};
use clap::Parser;
use pretty_env_logger;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::io::BufRead;
use std::path::Path;

mod jenkins;

#[derive(Parser)]
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

fn main() -> Result<()> {
    pretty_env_logger::init();

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

    let code = run_jenkins(cfg, &opts.args)?;
    std::process::exit(code);
}

fn run_jenkins(cfg: &jenkins::Server, args: &[String]) -> Result<i32> {
    let cli = jenkins::Cli::new(cfg.clone())?;
    if args[0] == "tree" {
        run_tree_cmd(cli, &args[1..])
    } else {
        let mut resp = cli.send(args)?;
        std::io::copy(resp.output(), &mut std::io::stdout())?;
        resp.wait_exit_code()
    }
}

fn run_tree_cmd(cli: jenkins::Cli, args: &[String]) -> Result<i32> {
    let mut folder = None;
    if !args.is_empty() {
        folder = Some(args[0].as_str())
    }
    let mut lines = list_jobs(&cli, folder)?;
    lines.reverse();
    let mut stack = Vec::with_capacity(lines.len());
    stack.append(&mut lines);

    while !stack.is_empty() {
        let folder = stack.pop().unwrap();
        let content = list_jobs(&cli, Some(folder.as_str()));

        match content {
            Ok(mut subitems) => {
                subitems.reverse();
                stack.append(&mut subitems)
            }
            Err(err) => match err {
                ListJobError::NotFolder { path, code: _ } => println!("{}", path),
                ListJobError::Other(inner_err) => return Err(inner_err),
            },
        }
    }
    Ok(0)
}

#[derive(Debug)]
enum ListJobError {
    NotFolder { path: String, code: i32 },
    Other(anyhow::Error),
}

impl std::fmt::Display for ListJobError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ListJobError::NotFolder { path, code } => {
                write!(f, "cannot list job on {} (code {})", path, code)
            }
            ListJobError::Other(err) => write!(f, "{}", err),
        }
    }
}
impl std::error::Error for ListJobError {}

fn list_jobs(
    cli: &jenkins::Cli,
    folder: Option<&str>,
) -> std::result::Result<Vec<String>, ListJobError> {
    let mut list_args = Vec::with_capacity(2);
    list_args.push("list-jobs".to_string());
    if let Some(str) = folder {
        list_args.push(str.to_string());
    }
    let mut resp = cli.send(&list_args[..]).map_err(ListJobError::Other)?;
    let mut output = BytesMut::new().writer();
    std::io::copy(resp.output(), &mut output).map_err(|err| ListJobError::Other(err.into()))?;

    let code = resp.wait_exit_code().map_err(ListJobError::Other)?;
    let folder_base = folder.map_or("", |l| l);
    if code != 0 {
        return Err(ListJobError::NotFolder {
            path: folder_base.to_string(),
            code,
        });
    }

    let input = output.get_mut().reader();
    Ok(input
        .lines()
        .filter(|l| l.is_ok())
        .map(|l| l.unwrap())
        .map(|l| append_path(folder_base, &l))
        .collect())
}

fn append_path(root: &str, end: &str) -> String {
    if root.ends_with('/') {
        format!("{}{}", root, end)
    } else {
        format!("{}/{}", root, end)
    }
}

fn read_file<P: AsRef<Path>>(filepath: P) -> Result<Config> {
    let content = fs::read_to_string(filepath)?;
    let cfg = toml::from_str(&content)?;
    Ok(cfg)
}
