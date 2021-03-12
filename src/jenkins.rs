use reqwest::blocking;
use serde::Deserialize;
use anyhow::Result;
use std::io::Write;
use std::thread;
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

impl Cli {
    pub fn new(cfg: Server) -> Result<Cli> {
        Ok(Cli {
            cfg: cfg,
        })
    }

    
    fn client(&self) -> Result<blocking::Client> {
        let mut builder = blocking::Client::builder()
                .connect_timeout(Some(std::time::Duration::from_secs(10)))
                .connection_verbose(true)
                .cookie_store(true)
                .danger_accept_invalid_certs(true);
        if let Some(proxy) = &self.cfg.proxy {
            builder = builder.proxy(reqwest::Proxy::all(proxy)?);
        }
        Ok(builder.build()?)
    }


    pub fn send(&self, args: Vec<String>) -> Result<()> {
        let uuid = Uuid::new_v4();

        let clt_server = self.clone();
        let server = thread::spawn(move || -> Result<()> {
            let clt = clt_server.client()?;
            let url = reqwest::Url::parse(&format!("{}/{}", &clt_server.cfg.url, "cli"))?;
            let mut server_output = clt.post(url)
                .query(&[("remoting", "false")])
                .basic_auth(clt_server.cfg.username, Some(clt_server.cfg.password))
                .header("Session", format!("{}", &uuid))
                .header("Side", "download")
                .send()?;
            // server output ...
            let mut buf : Vec<u8> = Vec::with_capacity(1024);
            server_output.copy_to(&mut buf)?;
            println!("server: {:?}", buf);
            Ok(())
        });

        let clt_client = self.clone();
        let client = thread::spawn(move || -> Result<()> {
            let clt = clt_client.client()?;
            let url = reqwest::Url::parse(&format!("{}/{}", &clt_client.cfg.url, "cli"))?;
            let mut req = clt.post(url)
                .query(&[("remoting", "false")])
                .basic_auth(clt_client.cfg.username, Some(clt_client.cfg.password))
                .header("Content-Type", "application/octet-stream")
                .header("Transfer-encoding", "chunked")
                .header("Session", format!("{}", &uuid))
                .header("Side", "upload");
            let mut encoder = Encoder::new();
            for arg in &args {
                encoder.string(Code::Arg, arg)?;
            }
            encoder.string(Code::Encoding, "utf-8");
            encoder.string(Code::Locale, "en-US");
            encoder.op(Code::Start);

            req = req.body(encoder.buffer());
            let client_output = req.send()?;
            println!("client: {:?}", client_output.bytes());
            Ok(())
        });
        println!("client thread: {:?}", client.join().expect("error on client thread"));
        println!("server thread: {:?}", server.join().expect("error on server thread"));
        Ok(())
    }
}

struct Encoder {
    buf: Vec<u8>,
}

struct Frame<'a> {
    op: Code,
    data: &'a [u8],
}

#[derive(Clone, Copy)]
enum Code {
    Arg = 0,
    Locale = 1,
    Encoding = 2,
    Start= 3,
    Exit = 4,
    Stdin = 5,
    EndStdin = 6,
    Stdout = 7,
    Stderr = 8,
}

impl Encoder {
    fn new() -> Encoder {
        Encoder {
            buf: Vec::with_capacity(1024),
        }
    }

    fn frame(&mut self, f: &Frame) -> Result<()> {
        self.buf.write(&f.data.len().to_be_bytes())?;
        self.buf.write(&(f.op as u32).to_be_bytes())?;
        self.buf.write(f.data)?;
        Ok(())
    }

    fn op(&mut self, op: Code) -> Result<()> {
        self.frame(&Frame {
            op: op,
            data: &[0;0],
        })
    }

    fn string<'a>(&mut self, op: Code, s: &'a str) -> Result<()> {
        self.frame(&Frame{
            op: op,
            data: s.as_bytes(),
        })
    }

    fn buffer(&self) -> Vec<u8> {
        self.buf.clone()
    }
}
