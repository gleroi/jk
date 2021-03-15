use anyhow::{anyhow, Result};
use pipe::{PipeReader, PipeWriter};
use reqwest::blocking;
use serde::Deserialize;
use std::convert::TryFrom;
use std::convert::TryInto;
use std::io::Write;
use std::sync::{Arc, Barrier};
use std::thread;
use uuid::Uuid;

mod http;
mod websocket;

#[derive(Debug, Clone, Deserialize)]
pub struct Server {
    pub url: String,
    pub username: String,
    pub password: String,
    pub proxy: Option<String>,
}

#[derive(Clone)]
pub struct Cli {
    cfg: Server,
}

pub struct Command {
    pub out: Vec<u8>,
    pub exit_code: i32,
}


// TODO: return in command a pipe, to read the pipe while the transport/thread write to it. and  keep the thread
// in scope.

impl Cli {
    pub fn new(cfg: Server) -> Result<Cli> {
        Ok(Cli { cfg: cfg })
    }

    fn client(&self) -> Result<blocking::Client> {
        let mut builder = blocking::Client::builder()
            .tcp_keepalive(std::time::Duration::from_secs(1))
            .timeout(None)
            .cookie_store(true)
            .danger_accept_invalid_certs(true);
        if let Some(proxy) = &self.cfg.proxy {
            builder = builder.proxy(reqwest::Proxy::all(proxy)?);
        }
        Ok(builder.build()?)
    }

    pub fn send(&self, args: Vec<String>) -> Result<Command> {
        let mut transport = http::Transport::new(self)?;

        let mut encoder = Encoder::new();
        for arg in &args {
            encoder.string(Code::Arg, arg)?;
        }
        encoder.string(Code::Encoding, "utf-8")?;
        encoder.string(Code::Locale, "en")?;
        encoder.op(Code::Start)?;
        println!("write request to http transport");
        transport.write_all(&encoder.buffer())?;
        println!("flush request to http transport");
        transport.flush()?;

        let mut decoder = Decoder { r: &mut transport };
        let mut cmd = Command {
            out: Vec::with_capacity(1024),
            exit_code: 0,
        };
        decoder.skip_initial_zero()?;
        loop {
            let maybe_frame = decoder.frame()?;
            if let Some(f) = maybe_frame {
                match &f.op {
                    Code::Stderr => {
                        std::io::stdout().write_all(&f.data)?;
                        std::io::stdout().flush()?;
                    }
                    Code::Stdout => {
                        std::io::stdout().write_all(&f.data)?;
                        std::io::stdout().flush()?;
                    }
                    Code::Exit => {
                        cmd.exit_code = i32::from_be_bytes(f.data[0..4].try_into()?);
                        break;
                    }
                    _ => println!("{:?}", f),
                }
            } else {
                break;
            }
        }
        Ok(cmd)
    }
}

#[derive(Debug)]
struct Frame {
    op: Code,
    data: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
enum Code {
    Arg = 0,
    Locale = 1,
    Encoding = 2,
    Start = 3,
    Exit = 4,
    Stdin = 5,
    EndStdin = 6,
    Stdout = 7,
    Stderr = 8,
}

impl TryFrom<u8> for Code {
    type Error = anyhow::Error;

    fn try_from(i: u8) -> Result<Self> {
        match i {
            0 => Ok(Code::Arg),
            1 => Ok(Code::Locale),
            2 => Ok(Code::Encoding),
            3 => Ok(Code::Start),
            4 => Ok(Code::Exit),
            5 => Ok(Code::Stdin),
            6 => Ok(Code::EndStdin),
            7 => Ok(Code::Stdout),
            8 => Ok(Code::Stderr),
            _ => Err(anyhow!("Code: unexpected value {}", i)),
        }
    }
}

struct Encoder {
    buf: Vec<u8>,
}

impl Encoder {
    fn new() -> Encoder {
        Encoder {
            buf: Vec::with_capacity(1024),
        }
    }

    fn frame(&mut self, f: &Frame) -> Result<()> {
        self.buf.write(&(f.data.len() as u32).to_be_bytes())?;
        self.buf.write(&(f.op as u8).to_be_bytes())?;
        self.buf.write(&f.data)?;
        Ok(())
    }

    fn op(&mut self, op: Code) -> Result<()> {
        self.frame(&Frame {
            op: op,
            data: Vec::new(),
        })
    }

    fn string<'a>(&mut self, op: Code, s: &'a str) -> Result<()> {
        let str_bytes = s.as_bytes();
        let mut data = Vec::with_capacity(2 + str_bytes.len());
        data.write(&(str_bytes.len() as u16).to_be_bytes())?;
        data.write(str_bytes)?;
        self.frame(&Frame { op: op, data: data })
    }

    fn buffer(&self) -> Vec<u8> {
        self.buf.clone()
    }
}

struct Decoder<'a, T: std::io::Read> {
    r: &'a mut T,
}

impl<T: std::io::Read> Decoder<'_, T> {
    fn skip_initial_zero(&mut self) -> Result<()> {
        let mut buf: [u8; 1] = [42; 1];
        self.r.read_exact(&mut buf)?;
        assert_eq!(buf[0], 0);
        Ok(())
    }

    fn frame(&mut self) -> Result<Option<Frame>> {
        let mut buf = [0; 4];
        self.r.read_exact(&mut buf)?;
        let len = u32::from_be_bytes(buf) as usize;

        self.r.read_exact(&mut buf[0..1])?;
        let op = buf[0].try_into()?;

        let mut data = Vec::with_capacity(len);
        data.resize(len, 0);
        self.r.read_exact(&mut data)?;
        Ok(Some(Frame { op: op, data: data }))
    }
}
