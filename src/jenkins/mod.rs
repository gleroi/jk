use anyhow::{anyhow, Result};
use reqwest::blocking;
use serde::Deserialize;
use pipe::{pipe, PipeReader};
use std::convert::TryFrom;
use std::convert::TryInto;
use std::io::{Read, Write};
use std::thread;

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

pub struct Response {
    output: PipeReader,
    decode_thread: thread::JoinHandle<Result<i32>>,
}

impl Response {
    pub fn output(&mut self) -> &mut impl Read {
        return &mut self.output;
    }

    pub fn wait_exit_code(self) -> Result<i32> {
        match self.decode_thread.join() {
            Ok(code) => Ok(code?),
            Err(err) => Err(anyhow!("error in decoding thread: {:?}", err))
        }
    }
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

    pub fn send(&self, args: Vec<String>) -> Result<Response> {
        let mut transport = http::Transport::new(self)?;

        let mut encoder = Encoder::new();
        for arg in &args {
            encoder.string(Code::Arg, arg)?;
        }
        encoder.string(Code::Encoding, "utf-8")?;
        encoder.string(Code::Locale, "en")?;
        encoder.op(Code::Start)?;
        transport.write_all(&encoder.buffer())?;
        transport.flush()?;
        transport.close_input();

        let (output, mut input) = pipe();
        let decode_thread = thread::spawn(move || -> Result<i32> {
            let mut decoder = Decoder { r: &mut transport };
            decoder.skip_initial_zero()?;
            loop {
                let f = decoder.frame()?;
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

//TODO: split Frame & code in ServerFrame/ClientFrame

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

    fn frame(&mut self) -> Result<Frame> {
        let mut buf = [0; 4];
        self.r.read_exact(&mut buf)?;
        let len = u32::from_be_bytes(buf) as usize;

        self.r.read_exact(&mut buf[0..1])?;
        let op = buf[0].try_into()?;

        let mut data = Vec::with_capacity(len);
        data.resize(len, 0);
        self.r.read_exact(&mut data)?;
        Ok(Frame { op: op, data: data })
    }
}
