use anyhow::{anyhow, Result};
use reqwest::blocking;
use serde::Deserialize;
use std::convert::TryFrom;
use std::convert::TryInto;
use std::io::Write;
use std::thread;
use tungstenite::client::*;
use tungstenite::*;
use uuid::Uuid;

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
    pub err: Vec<u8>,
    pub exit_code: i32,
}

impl Cli {
    pub fn new(cfg: Server) -> Result<Cli> {
        Ok(Cli { cfg: cfg })
    }

    fn client(&self) -> Result<blocking::Client> {
        let mut builder = blocking::Client::builder()
            .tcp_keepalive(std::time::Duration::from_secs(1))
            .cookie_store(true)
            .danger_accept_invalid_certs(true);
        if let Some(proxy) = &self.cfg.proxy {
            builder = builder.proxy(reqwest::Proxy::all(proxy)?);
        }
        Ok(builder.build()?)
    }

    pub fn send(&self, args: Vec<String>) -> Result<Command> {
        use std::sync::{Arc, Barrier};

        let uuid = Uuid::new_v4();
        // - a thread is spawn to read command output, and wait for main thread to be ready
        // - main thread prepare the command and wait first thread to be ready to listen
        let ready = Arc::new(Barrier::new(2));

        let clt_server = self.clone();
        let server_ready = ready.clone();
        let server = thread::spawn(move || -> Result<Command> {
            let clt = clt_server.client()?;
            let url = reqwest::Url::parse(&format!("{}/{}", &clt_server.cfg.url, "cli"))?;
            let mut server_output = clt
                .post(url)
                .query(&[("remoting", "false")])
                .basic_auth(clt_server.cfg.username, Some(clt_server.cfg.password))
                .header("Session", format!("{}", &uuid))
                .header("Side", "download")
                .send()?;
            server_ready.wait(); // wait for main thread to send the command
            let mut buf: Vec<u8> = Vec::with_capacity(1024);
            server_output.copy_to(&mut buf)?;
            let mut decoder = Decoder { buf: &buf[1..] };
            let mut cmd = Command {
                out: Vec::with_capacity(buf.len()),
                err: Vec::with_capacity(buf.len()),
                exit_code: 0,
            };
            loop {
                let maybe_frame = decoder.frame()?;
                if let Some(f) = maybe_frame {
                    match &f.op {
                        Code::Stderr => {
                            cmd.err.write_all(&f.data)?;
                        }
                        Code::Stdout => {
                            cmd.out.write_all(&f.data)?;
                        }
                        Code::Exit => {
                            cmd.exit_code = i32::from_be_bytes(f.data.try_into()?);
                        }
                        _ => println!("{:?}", f),
                    }
                } else {
                    break;
                }
            }
            Ok(cmd)
        });

        let clt = self.client()?;
        let url = reqwest::Url::parse(&format!("{}/{}", &self.cfg.url, "cli"))?;
        let mut req = clt
            .post(url)
            .query(&[("remoting", "false")])
            .basic_auth(&self.cfg.username, Some(&self.cfg.password))
            .header("Content-Type", "application/octet-stream")
            .header("Transfer-encoding", "chunked")
            .header("Session", format!("{}", &uuid))
            .header("Side", "upload");
        let mut encoder = Encoder::new();
        for arg in &args {
            encoder.string(Code::Arg, arg)?;
        }
        encoder.string(Code::Encoding, "utf-8")?;
        encoder.string(Code::Locale, "en")?;
        encoder.op(Code::Start)?;

        req = req.body(encoder.buffer());
        ready.wait(); // wait for thread to be ready to read the result
        req.send()?;

        Ok(server.join().expect("error on while reading response")?)
    }

    fn websocket(&self) -> Result<WebSocket<AutoStream>> {
        let mut url = reqwest::Url::parse(&format!("{}/{}", &self.cfg.url, "cli/ws"))?;
        url.set_scheme("ws").unwrap();
        let req = handshake::client::Request::builder()
            .uri(url.to_string())
            .header(
                "Authorization",
                format!(
                    "Basic {}",
                    base64::encode(format!("{}:{}", &self.cfg.username, &self.cfg.password))
                ),
            )
            .body(())?;
        let (ws, resp) = client::connect(req)?;
        if resp.status().is_client_error() || resp.status().is_server_error() {
            Err(anyhow!("error while establishing ws: {}", resp.status()))
        } else {
            Ok(ws)
        }
    }

    pub fn sendws(&self, args: &Vec<String>) -> Result<()> {
        let mut ws = self.websocket()?;
        let mut encoder = Encoder::new();
        for arg in args {
            encoder.string(Code::Arg, arg)?;
        }
        encoder.string(Code::Encoding, "utf-8")?;
        encoder.string(Code::Locale, "en")?;
        encoder.op(Code::Start)?;
        ws.write_message(Message::Binary(encoder.buffer()))?;
        loop {
            let resp = ws.read_message()?;
            match resp {
                Message::Text(str) => println!("{}", str),
                Message::Binary(data) => println!("{}", String::from_utf8_lossy(&data)),
                Message::Ping(ref data) => {
                    println!("expected: {:?}", resp);
                    ws.write_message(Message::Pong(data.to_vec()))?;
                }
                _ => println!("unexpected: {:?}", resp),
            }
        }
    }
}

#[derive(Debug)]
struct Frame<'a> {
    op: Code,
    data: &'a [u8],
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
        self.buf.write(f.data)?;
        Ok(())
    }

    fn op(&mut self, op: Code) -> Result<()> {
        self.frame(&Frame {
            op: op,
            data: &[0; 0],
        })
    }

    fn string<'a>(&mut self, op: Code, s: &'a str) -> Result<()> {
        let str_bytes = s.as_bytes();
        let mut data = Vec::with_capacity(2 + str_bytes.len());
        data.write(&(str_bytes.len() as u16).to_be_bytes())?;
        data.write(str_bytes)?;
        self.frame(&Frame {
            op: op,
            data: &data,
        })
    }

    fn buffer(&self) -> Vec<u8> {
        self.buf.clone()
    }
}

struct Decoder<'a> {
    buf: &'a [u8],
}

impl Decoder<'_> {
    fn frame(&mut self) -> Result<Option<Frame>> {
        if self.buf.len() < 4 {
            return Ok(None);
        }
        let len = u32::from_be_bytes(self.buf[0..4].try_into()?) as usize;
        let op = self.buf[4].try_into()?;
        let data = &self.buf[5..5 + len];
        if 5 + len >= self.buf.len() {
            self.buf = &[0; 0]
        } else {
            self.buf = &self.buf[5 + len..];
        }
        Ok(Some(Frame { op: op, data: data }))
    }
}
