use super::Cli;
use crate::jenkins;
use crate::jenkins::Frame;
use anyhow::{anyhow, Result};
use std::convert::TryInto;
use std::io::Write;
use tungstenite::client::AutoStream;
use tungstenite::{client, handshake};
use tungstenite::{Message, WebSocket};

pub struct Transport {
    socket: WebSocket<AutoStream>,
}

impl Transport {
    pub fn new(cli: &Cli) -> Result<Transport> {
        let socket = websocket(cli)?;
        Ok(Transport { socket })
    }
}

impl jenkins::Transport for Transport {
    fn write_frame(&mut self, f: &Frame) -> Result<()> {
        let mut buf = Vec::with_capacity(f.data.len() + 1);
        buf.write_all(&(f.op as u8).to_be_bytes())?;
        buf.write_all(&f.data)?;
        self.socket.write_message(Message::Binary(buf))?;
        Ok(())
    }

    fn read_frame(&mut self) -> Result<Frame> {
        loop {
            let m = self.socket.read_message()?;
            match m {
                Message::Binary(buf) => {
                    let op = buf[0].try_into()?;

                    let data = buf[1..].to_vec();
                    return Ok(Frame { op, data });
                }
                _ => (),
            }
        }
    }

    fn close_input(&mut self) -> Result<()> {
        self.socket.write_pending()?;
        Ok(())
    }
}

fn websocket(clt: &Cli) -> Result<WebSocket<AutoStream>> {
    let url = reqwest::Url::parse(&format!("{}/{}", clt.cfg.url, "cli/ws"))?;
    let req = handshake::client::Request::builder()
        .uri(url.to_string())
        .header(
            "Authorization",
            format!(
                "Basic {}",
                base64::encode(format!("{}:{}", clt.cfg.username, clt.cfg.password))
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
