use hyper::body::HttpBody as _;
use hyper::{Client, Request, Uri, Body};
use tokio::io::{stdout, AsyncWriteExt as _};

type AResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[tokio::main]
async fn main() -> AResult<()> {
    let uuid = uuid::Uuid::new_v4();
    let client = Client::new();
    let ouput = tokio::spawn(async move {
        let uri = Uri::builder()
            .scheme("http")
            .authority("127.0.0.1:8080")
            .path_and_query("/cli?remoting=false")
            .build()
            .expect("invalid uri");
        let req = Request::builder()
            .method("POST")
            .uri(uri)
            .header("Session", format!("{}", &uuid))
            .header("Side", "download")
            .body(Body::empty())
            .expect("request builder");

        let mut resp = client.request(req).await.expect("request");
        println!("Response: {}", resp.status());
        while let Some(chunk) = resp.body_mut().data().await {
            stdout().write_all(&chunk.expect("invalid chunk")).await.expect("stdout");
        }
    });

    let input = tokio::spawn(async move {
        let uri = Uri::builder()
            .scheme("http")
            .authority("127.0.0.1:8080")
            .path_and_query("/cli?remoting=false")
            .build()
            .expect("invalid uri");
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
            .body(())
            .expect("request builder");
    });
    Ok(())
}
