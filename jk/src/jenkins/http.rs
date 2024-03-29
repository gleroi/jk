use super::Cli;
use crate::jenkins;
use crate::jenkins::Frame;
use anyhow::{anyhow, Context, Result};
use log::debug;
use pipe::{pipe, PipeReader, PipeWriter};
use reqwest::blocking;
use std::convert::TryInto;
use std::io::{Read, Write};
use std::sync::{Arc, Barrier};
use std::thread;
use uuid::Uuid;

pub struct Transport {
    server_thread: Option<thread::JoinHandle<Result<()>>>,
    server_output: PipeReader,

    client_thread: Option<thread::JoinHandle<Result<()>>>,
    client_input: PipeWriter,

    initial_zero_skipped: bool,
}

impl Transport {
    pub fn new(clt: &Cli) -> Result<Transport> {
        // - a thread is spawn to read command output, and wait for main thread to be ready
        // - main thread prepare the command and wait first thread to be ready to listen
        let ready = Arc::new(Barrier::new(2));
        let uuid = Uuid::new_v4();
        let (server, output) = Self::recv(clt.clone(), uuid, ready.clone());
        let (client, input) = Self::send(clt.clone(), uuid, ready);
        Ok(Transport {
            server_thread: Some(server),
            server_output: output,
            client_thread: Some(client),
            client_input: input,
            initial_zero_skipped: false,
        })
    }

    fn client(clt: &Cli) -> Result<blocking::Client> {
        let mut builder = blocking::Client::builder()
            .tcp_keepalive(std::time::Duration::from_secs(1))
            .timeout(None)
            .cookie_store(true)
            .danger_accept_invalid_certs(true);
        if let Some(proxy) = &clt.cfg.proxy {
            builder = builder.proxy(reqwest::Proxy::all(proxy)?);
        }
        Ok(builder.build()?)
    }

    fn recv(
        clt_server: Cli,
        uuid: Uuid,
        server_ready: Arc<Barrier>,
    ) -> (thread::JoinHandle<Result<()>>, PipeReader) {
        let (output, mut input) = pipe::pipe();

        let server = thread::spawn(move || -> Result<()> {
            let clt = Self::client(&clt_server)?;
            let url = reqwest::Url::parse(&format!("{}/{}", &clt_server.cfg.url, "cli"))?;
            let mut server_output = clt
                .post(url)
                .query(&[("remoting", "false")])
                .basic_auth(clt_server.cfg.username, Some(clt_server.cfg.password))
                .header("Session", format!("{}", &uuid))
                .header("Side", "download")
                .send()?;
            if !server_output.status().is_success() {
                return Err(anyhow!(
                    "RECV {}: {}",
                    server_output.url(),
                    server_output.status()
                ));
            }
            server_ready.wait(); // wait for main thread to send the command
            server_output.copy_to(&mut input)?;
            input.flush()?;
            Ok(())
        });
        (server, output)
    }

    fn send(
        clt_client: Cli,
        uuid: Uuid,
        ready: Arc<Barrier>,
    ) -> (thread::JoinHandle<Result<()>>, PipeWriter) {
        let (output, input) = pipe::pipe();

        let client = thread::spawn(move || -> Result<()> {
            let clt = Self::client(&clt_client)?;
            let url = reqwest::Url::parse(&format!("{}/{}", &clt_client.cfg.url, "cli"))?;
            let mut req = clt
                .post(url)
                .query(&[("remoting", "false")])
                .basic_auth(&clt_client.cfg.username, Some(&clt_client.cfg.password))
                .header("Content-Type", "application/octet-stream")
                .header("Transfer-encoding", "chunked")
                .header("Session", format!("{}", &uuid))
                .header("Side", "upload");

            // this works because:
            // - bytes() return a Iterator of Result,
            // - Result implements FromIterator trait
            // - which collect() calls to produce a value from its input
            let input = output
                .bytes()
                .collect::<Result<Vec<u8>, std::io::Error>>()?;
            req = req.body(input);
            ready.wait(); // wait for thread to be ready to read the result
            let rep = req.send().with_context(|| "while sending request... ")?;
            if !rep.status().is_success() {
                return Err(anyhow!("SEND {}: {}", rep.url(), rep.status()));
            }
            Ok(())
        });
        (client, input)
    }
}

impl jenkins::Transport for Transport {
    fn write_frame(&mut self, f: &Frame) -> Result<()> {
        self.write_all(&(f.data.len() as u32).to_be_bytes())?;
        self.write_all(&(f.op as u8).to_be_bytes())?;
        self.write_all(&f.data)?;
        Ok(())
    }

    fn read_frame(&mut self) -> Result<Frame> {
        let mut buf = [0; 4];
        if !self.initial_zero_skipped {
            self.read_exact(&mut buf[0..1])?;
            self.initial_zero_skipped = true;
            debug!("read initial zero");
        }

        let mut buf = [0; 4];
        self.read_exact(&mut buf)?;
        let len = u32::from_be_bytes(buf) as usize;
        debug!("read frame len: {}", len);

        self.read_exact(&mut buf[0..1])?;
        debug!("read frame op: {}", buf[0]);
        let op = buf[0].try_into()?;

        let mut data = vec![0; len];
        self.read_exact(&mut data)?;
        debug!("read frame data: {:?}", data);
        Ok(Frame { op, data })
    }

    fn close_input(&mut self) -> Result<()> {
        self.flush()?;

        // try close input so that client_thread leave bytes()
        // by replacing it, it should drop the previous one
        // then output is drop and should close the new input too
        // it's ugly, but could not find an another way to do it :'(
        let (_output, input) = pipe();
        self.client_input = input;
        Ok(())
    }
}

impl std::io::Write for Transport {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.client_input.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.client_input.flush()
    }
}

impl std::io::Read for Transport {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.server_output.read(buf)
    }
}

impl Drop for Transport {
    fn drop(&mut self) {
        self.server_thread
            .take()
            .unwrap()
            .join()
            .unwrap()
            .expect("error in server thread");
        self.client_thread
            .take()
            .unwrap()
            .join()
            .unwrap()
            .expect("error in client thread");
    }
}
