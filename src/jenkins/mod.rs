use anyhow::{anyhow, Result};
use pipe::{pipe, PipeReader};
use reqwest::blocking;
use serde::Deserialize;
use std::convert::TryFrom;
use std::convert::TryInto;
use std::io::{Read, Write};
use std::thread;

mod codec;
mod http;
mod websocket;

use codec::{Encoder};

#[derive(Debug)]
pub struct Frame {
    op: Code,
    data: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
pub enum Code {
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

pub trait Transport {
    fn write_frame(&mut self, frame: &Frame) -> Result<()>;
    fn read_frame(&mut self) -> Result<Frame>;
}

pub struct Response {
    output: PipeReader,
    decode_thread: thread::JoinHandle<Result<i32>>,
}

impl Response {
    pub fn output(&mut self) -> &mut impl Read {
        &mut self.output
    }

    pub fn wait_exit_code(self) -> Result<i32> {
        match self.decode_thread.join() {
            Ok(code) => Ok(code?),
            Err(err) => Err(anyhow!("error in decoding thread: {:?}", err)),
        }
    }
}

impl Cli {
    pub fn new(cfg: Server) -> Result<Cli> {
        Ok(Cli { cfg })
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

    pub fn send(&self, args: &[String]) -> Result<Response> {
        let mut transport = http::Transport::new(self)?;

        let mut encoder = Encoder::new(&mut transport);
        for arg in args {
            encoder.string(Code::Arg, arg)?;
        }
        encoder.string(Code::Encoding, "utf-8")?;
        encoder.string(Code::Locale, "en")?;
        encoder.op(Code::Start)?;
        transport.flush()?;
        transport.close_input();

        let (output, mut input) = pipe();
        let decode_thread = thread::spawn(move || -> Result<i32> {
            loop {
                let f = transport.read_frame()?;
                match &f.op {
                    Code::Stderr => {
                        input.write_all(&f.data)?;
                        input.flush()?;
                    }
                    Code::Stdout => {
                        input.write_all(&f.data)?;
                        input.flush()?;
                    }
                    Code::Exit => {
                        drop(input);
                        let exit_code = i32::from_be_bytes(f.data[0..4].try_into()?);
                        return Ok(exit_code);
                    }
                    _ => println!("unexpected {:?}", f),
                }
            }
        });
        Ok(Response {
            output,
            decode_thread,
        })
    }
}
