use super::Cli;
use anyhow::{Context, Result};
use pipe::{pipe, PipeReader, PipeWriter};
use std::io::{Read, Write};
use std::sync::{Arc, Barrier};
use std::thread;
use uuid::Uuid;

pub struct Transport {
    _server_thread: thread::JoinHandle<Result<()>>,
    server_output: PipeReader,

    _client_thread: thread::JoinHandle<Result<()>>,
    client_input: PipeWriter,
}

impl Transport {
    pub fn new(clt: &Cli) -> Result<Transport> {
        // - a thread is spawn to read command output, and wait for main thread to be ready
        // - main thread prepare the command and wait first thread to be ready to listen
        let ready = Arc::new(Barrier::new(2));
        let uuid = Uuid::new_v4();
        let (server, output) = recv(clt.clone(), uuid, ready.clone());
        let (client, input) = send(clt.clone(), uuid, ready);
        Ok(Transport {
            _server_thread: server,
            server_output: output,
            _client_thread: client,
            client_input: input,
        })
    }

    pub fn close_input(&mut self) {
        // try close input so that client_thread leave bytes()
        // by replacing it, it should drop the previous one
        // then output is drop and should close the new input too
        // it's ugly, but could not find an another way to do it :'(
        let (_output, input) = pipe();
        self.client_input = input;
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

fn recv(
    clt_server: Cli,
    uuid: Uuid,
    server_ready: Arc<Barrier>,
) -> (thread::JoinHandle<Result<()>>, PipeReader) {
    let (output, mut input) = pipe::pipe();

    let server = thread::spawn(move || -> Result<()> {
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
        let clt = clt_client.client()?;
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
        req.send().with_context(|| "while sending request... ")?;
        Ok(())
    });
    (client, input)
}
