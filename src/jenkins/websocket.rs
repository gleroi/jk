
/*
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
        for arg in args {
            let mut encoder = Encoder::new();
            encoder.string(Code::Arg, arg)?;
            ws.write_message(Message::Binary(encoder.buffer()))?;
        }
        {
            let mut encoder = Encoder::new();
            encoder.string(Code::Encoding, "utf-8")?;
            ws.write_message(Message::Binary(encoder.buffer()))?;
        }
        {
            let mut encoder = Encoder::new();
            encoder.string(Code::Locale, "en")?;
            ws.write_message(Message::Binary(encoder.buffer()))?;
        }
        {
            let mut encoder = Encoder::new();
            encoder.op(Code::Start)?;
            ws.write_message(Message::Binary(encoder.buffer()))?;
        }
        {
            let mut encoder = Encoder::new();
            encoder.op(Code::Stdin)?;
            ws.write_message(Message::Binary(encoder.buffer()))?;
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
