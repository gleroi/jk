struct Transport {
    cfg: Server,
    input: DuplexStream,
    output: DuplexStream,
    input_handle: JoinHandle,
    output_handle: JoinHandle,
}

impl Transport {
    fn new(cfg: &Server) -> Self {
        let req = request(cfg, &uuid)?
            .header("Side", "download")
            .body(Body::empty())?;
        let mut resp = client.request(req).await?;

        let scfg = cfg.clone();
        let sargs = args.clone();
        let sender = tokio::spawn(async move {
            send(client.clone(), &scfg, uuid, &sargs).await
        });

        let (output, input) = tokio::io::duplex(1024);
    }
}
