use hyper::body::{HttpBody as _, Buf};
use hyper::{Client, Request, Uri, Body};
use tokio::{join, try_join, io::{stdout, AsyncWriteExt as _}};
use anyhow::{anyhow, Result};
use std::io::Read;
use std::convert::TryInto;

type AResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[tokio::main]
async fn main() -> AResult<()> {
    let uuid = uuid::Uuid::new_v4();
    let client = Client::new();
    recv(client, uuid).await?;
    Ok(())
}

async fn recv<T>(client: Client<T>, uuid: uuid::Uuid) -> AResult<i32>
    where T: 'static + hyper::client::connect::Connect + Send + Sync + Clone {
    let uri = Uri::builder()
        .scheme("http")
        .authority("127.0.0.1:8080")
        .path_and_query("/cli?remoting=false")
        .build()?;
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header(
            "Authorization",
            format!(
                "Basic {}",
                base64::encode(format!("{}:{}", "gleroi", "gleroi"))
            ),
        )
        .header("Session", format!("{}", &uuid))
        .header("Side", "download")
        .body(Body::empty())?;
    let mut resp = client.request(req).await?;
    println!("Response: {}", resp.status());
    send(client.clone(), uuid).await?;
    println!("awaiting input done");
    let mut output = hyper::body::to_bytes(resp.body_mut()).await?.reader();
    let mut buf = [0; 4];
    output.read_exact(&mut buf[0..1])?;
    loop {
        let f = read_frame(&mut output)?;
        //stdout().write_all(format!("{:?} ", f).as_bytes()).await?;
        match &f.op {
            Code::Stderr | Code::Stdout => {
                stdout().write_all(&f.data).await?;
                stdout().flush().await?;
            }
            Code::Exit => {
                let exit_code = i32::from_be_bytes(f.data[0..4].try_into()?);
                return Ok(exit_code);
            }
            _ => println!("unexpected {:?}", f),
        }
    }
}

async fn send<T>(input_client: Client<T>, uuid: uuid::Uuid) -> AResult<()>
    where T: 'static + hyper::client::connect::Connect + Send + Sync + Clone {
    println!("start request");
    let uri = Uri::builder()
        .scheme("http")
        .authority("127.0.0.1:8080")
        .path_and_query("/cli?remoting=false")
        .build()?;
    let mut buf = Vec::with_capacity(256);
    let mut encoder = Encoder { w: &mut buf };
    encoder.string(Code::Arg, "help")?;
    encoder.string(Code::Encoding, "utf-8")?;
    encoder.string(Code::Locale, "en")?;
    encoder.op(Code::Start)?;
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header(
            "Authorization",
            format!(
                "Basic {}",
                base64::encode(format!("{}:{}", "gleroi", "gleroi"))
            ),
        )
        .header("Content-Type", "application/octet-stream")
        .header("Transfer-encoding", "chunked")
        .header("Session", format!("{}", &uuid))
        .header("Side", "upload")
        .body(buf.into())?;
    println!("sending request");
    let mut resp = input_client.request(req).await?;
    println!("Request: {}", resp.status());
    Ok(())
}

fn read_frame(r: &mut impl std::io::Read) -> Result<Frame> {
    let mut buf = [0; 4];
    r.read_exact(&mut buf)?;
    let len = u32::from_be_bytes(buf) as usize;

    r.read_exact(&mut buf[0..1])?;
    let op = buf[0].try_into()?;

    let mut data = vec![0; len];
    r.read_exact(&mut data)?;
    Ok(Frame { op, data })
}

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

use std::convert::TryFrom;

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

pub struct Encoder<'a, T: std::io::Write> {
    w: &'a mut T,
}

impl<T: std::io::Write> Encoder<'_, T> {
    pub fn new(writer: &mut T) -> Encoder<T> {
        Encoder { w: writer }
    }

    fn frame(&mut self, f: &Frame) -> Result<()> {
        std::io::Write::write_all(&mut self.w, &(f.data.len() as u32).to_be_bytes())?;
        std::io::Write::write_all(&mut self.w, &(f.op as u8).to_be_bytes())?;
        std::io::Write::write_all(&mut self.w, &f.data)?;
        Ok(())
    }

    pub fn op(&mut self, op: Code) -> Result<()> {
        self.frame(&Frame {
            op,
            data: vec![0; 0],
        })
    }

    pub fn string(&mut self, op: Code, s: &str) -> Result<()> {
        let str_bytes = s.as_bytes();
        let mut data = Vec::with_capacity(2 + str_bytes.len());
        std::io::Write::write_all(&mut data, &(str_bytes.len() as u16).to_be_bytes())?;
        std::io::Write::write_all(&mut data, str_bytes)?;
        self.frame(&Frame { op, data })
    }
}
