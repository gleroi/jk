use super::{Cli, Code};
use anyhow::{anyhow, Result};
use tungstenite::client::AutoStream;
use tungstenite::{client, handshake};
use tungstenite::{Message, WebSocket};

use super::codec::Encoder;
/*
fn websocket(clt: &Cli) -> Result<WebSocket<AutoStream>> {
    let mut url = reqwest::Url::parse(&format!("{}/{}", clt.cfg.url, "cli/ws"))?;
    url.set_scheme("wss").unwrap();
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
    println!("{:#?}", resp);

    if resp.status().is_client_error() || resp.status().is_server_error() {
        Err(anyhow!("error while establishing ws: {}", resp.status()))
    } else {
        Ok(ws)
    }
}

pub fn sendws(clt: &Cli, args: &[String]) -> Result<()> {
    let mut ws = websocket(clt)?;
    for arg in args {
        ws.write_message(Message::Text(arg.to_string()))?;
    }
    {
        let mut buf = Vec::new();
        let mut encoder = Encoder::new(&mut buf);
        encoder.string(Code::Encoding, "utf-8")?;
        ws.write_message(Message::Binary(buf))?;
    }
    {
        let mut buf = Vec::new();
        let mut encoder = Encoder::new(&mut buf);
        encoder.string(Code::Locale, "en")?;
        ws.write_message(Message::Binary(buf))?;
    }
    {
        let mut buf = Vec::new();
        let mut encoder = Encoder::new(&mut buf);
        encoder.op(Code::Start)?;
        ws.write_message(Message::Binary(buf))?;
    }
    ws.write_pending()?;
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
*/
