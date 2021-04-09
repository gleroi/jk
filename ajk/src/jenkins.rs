use anyhow::{anyhow, Result};
use hyper::{Body, Client, Request, Uri};
use hyper_proxy::{Intercept, Proxy, ProxyConnector};
use hyper_tls::HttpsConnector;
use serde::Deserialize;
use std::convert::TryInto;
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWriteExt as _};
use hyper::body::HttpBody as _;

type AResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub async fn run(cfg: &'static Server, args: &[String]) -> AResult<i32> {
    let uuid = uuid::Uuid::new_v4();
    let mut connector = ProxyConnector::new(HttpsConnector::new())?;
    if let Some(ref proxy_url) = cfg.proxy {
        let proxy_uri = proxy_url.parse()?;
        connector.add_proxy(Proxy::new(Intercept::All, proxy_uri));
    }
    let client = Client::builder().build(connector);
    Ok(recv(client, cfg, uuid, args).await?)
}

fn request(cfg: &Server, uuid: &uuid::Uuid) -> Result<hyper::http::request::Builder> {
    let uri = Uri::from_maybe_shared(format!("{}/{}", cfg.url, "cli?remoting=false"))?;
    Ok(Request::builder()
        .method("POST")
        .uri(uri)
        .header(
            "Authorization",
            format!(
                "Basic {}",
                base64::encode(format!("{}:{}", &cfg.username, &cfg.password))
            ),
        )
        .header("Session", format!("{}", uuid))
        .header("Content-Type", "application/octet-stream")
        .header("Transfer-encoding", "chunked"))
}

async fn recv<T>(client: Client<T>, cfg: &'static Server, uuid: uuid::Uuid, args: &[String]) -> AResult<i32>
where
    T: 'static + hyper::client::connect::Connect + Send + Sync + Clone,
{
    let req = request(cfg, &uuid)?
        .header("Side", "download")
        .body(Body::empty())?;
    let mut resp = client.request(req).await?;

    let scfg = cfg.clone();
    let sargs = args.clone();
    let sender = tokio::spawn(async move {
        send(client.clone(), cfg, uuid, args).await
    });

    let (output, input) = tokio::io::duplex(1024);
    let decoder = tokio::spawn(async {
        read_all_frame(output).await
    });
    let copier = tokio::spawn(async move {
        copy_body(resp.body_mut(), input).await
    });
    let res = tokio::try_join!(decoder, copier, sender);
    match res {
        Ok((exit_code, _, _)) => Ok(exit_code?),
        Err(err) => Err(Box::new(err)),
    }
}

async fn copy_body(body: &mut hyper::Body, mut input: tokio::io::DuplexStream) -> AResult<()> {
    while let Some(chunk) = body.data().await {
        input.write_all(&chunk?).await?;
    } 
    Ok(())
}

async fn read_all_frame(mut output: tokio::io::DuplexStream) -> AResult<i32> {
    let mut buf : [u8; 4] = [0; 4];
    output.read_exact(&mut buf[0..1]).await?;
    let mut stdout = tokio::io::stdout();
    loop {
        let f = read_frame(&mut output).await?;
        match &f.op {
            Code::Stderr | Code::Stdout => {
                stdout.write_all(&f.data).await?;
                stdout.flush().await?;
            }
            Code::Exit => {
                let exit_code = i32::from_be_bytes(f.data[0..4].try_into()?);
                return Ok(exit_code);
            }
            _ => {
                stdout
                    .write_all(format!("unexpected {:?}\n", f).as_bytes())
                    .await?;
                stdout.flush().await?;

            }
        }
    }
}

async fn send<T>(
    input_client: Client<T>,
    cfg: &Server,
    uuid: uuid::Uuid,
    args: &[String],
) -> AResult<()>
where
    T: 'static + hyper::client::connect::Connect + Send + Sync + Clone,
{
    let mut buf = Vec::with_capacity(256);
    let mut encoder = Encoder::new(&mut buf);
    if args.is_empty() {
        encoder.string(Code::Arg, "help")?;
    } else {
        for arg in args {
            encoder.string(Code::Arg, arg)?;
        }
    }
    encoder.string(Code::Encoding, "utf-8")?;
    encoder.string(Code::Locale, "en")?;
    encoder.op(Code::Start)?;

    let req = request(&cfg, &uuid)?
        .header("Side", "upload")
        .body(buf.into())?;
    let _resp = input_client.request(req).await?;
    Ok(())
}

async fn read_frame<TReader: AsyncRead + Unpin>(r: &mut TReader) -> Result<Frame> {
    let mut buf = [0; 4];
    r.read_exact(&mut buf).await?;
    let len = u32::from_be_bytes(buf) as usize;

    r.read_exact(&mut buf[0..1]).await?;
    let op = buf[0].try_into()?;

    let mut data = vec![0; len];
    r.read_exact(&mut data).await?;
    Ok(Frame { op, data })
}

#[derive(Debug, Clone, Deserialize)]
pub struct Server {
    pub url: String,
    pub username: String,
    pub password: String,
    pub proxy: Option<String>,
}

pub struct Frame {
    op: Code,
    data: Vec<u8>,
}

use std::fmt;

impl fmt::Debug for Frame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Frame")
            .field("op", &self.op)
            .field("data", &format!("{}", String::from_utf8_lossy(&self.data)))
            .finish()
    }
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
