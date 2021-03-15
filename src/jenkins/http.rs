use super::{Cli, Code, Encoder};
use anyhow::{Context, Result};
use pipe::PipeReader;
use std::io::Write;
use std::sync::{Arc, Barrier};
use std::thread;
use uuid::Uuid;

pub fn recv(
    clt_server: Cli,
    uuid: Uuid,
    server_ready: Arc<Barrier>,
) -> Result<(thread::JoinHandle<Result<()>>, PipeReader)> {
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
    Ok((server, output))
}

pub fn send(
    clt_client: Cli,
    uuid: Uuid,
    ready: Arc<Barrier>,
    args: Vec<String>,
) -> Result<thread::JoinHandle<Result<()>>> {
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
        let mut encoder = Encoder::new();
        for arg in &args {
            encoder.string(Code::Arg, arg)?;
        }
        encoder.string(Code::Encoding, "utf-8")?;
        encoder.string(Code::Locale, "en")?;
        encoder.op(Code::Start)?;

        req = req.body(encoder.buffer());
        ready.wait(); // wait for thread to be ready to read the result
        req.send().with_context(|| "while sending request... ")?;
        Ok(())
    });
    Ok(client)
}
