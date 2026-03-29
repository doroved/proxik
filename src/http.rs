use crate::utils::format_bytes;
use anyhow::Result;
use http_body_util::{BodyExt, Full, combinators::BoxBody};
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, upgrade::Upgraded};
use hyper_util::rt::TokioIo;
use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};

pub async fn run(bind_addr: &str, _: Option<String>, _: Option<String>) -> Result<()> {
    let listener = TcpListener::bind(bind_addr).await?;
    tracing::info!(
        "{} server listening on {}",
        "HTTP",
        listener.local_addr()?.to_string()
    );

    loop {
        let (stream, addr) = listener.accept().await?;
        tokio::spawn(async move {
            let _ = http1::Builder::new()
                .preserve_header_case(true)
                .title_case_headers(true)
                .serve_connection(
                    TokioIo::new(stream),
                    service_fn(move |req| proxy(req, addr)),
                )
                .with_upgrades()
                .await;
        });
    }
}

async fn proxy(
    req: Request<hyper::body::Incoming>,
    client_addr: SocketAddr,
) -> Result<Response<BoxBody<Bytes, std::io::Error>>, Box<dyn std::error::Error + Send + Sync>> {
    if Method::CONNECT == req.method() {
        let addr = req
            .uri()
            .authority()
            .map(|a| a.to_string())
            .unwrap_or_default();
        tokio::spawn(async move {
            if let Ok(upgraded) = hyper::upgrade::on(req).await {
                let _ = tunnel(upgraded, addr, client_addr).await;
            }
        });
        Ok(Response::new(BoxBody::new(
            Full::new(Bytes::new()).map_err(|e| match e {}),
        )))
    } else {
        let host = req.uri().host().unwrap_or_default();
        let port = req.uri().port_u16().unwrap_or(80);
        let target = format!("{}:{}", host, port);

        tracing::info!("{} {} → connecting to {}", "[HTTP]", client_addr, target);
        let start = std::time::Instant::now();
        let method = req.method().clone();
        let path = req
            .uri()
            .path_and_query()
            .map(|p| p.as_str())
            .unwrap_or(req.uri().path())
            .to_string();

        let stream = TcpStream::connect(&target).await?;
        let (mut sender, conn) =
            hyper::client::conn::http1::handshake(TokioIo::new(stream)).await?;
        tokio::spawn(async move {
            let _ = conn.await;
        });

        let resp = sender.send_request(req).await?;

        tracing::info!(
            "{} {} → {} | {} {} | Status: {} | Duration: {}",
            "[HTTP]",
            client_addr,
            target,
            method.as_str(),
            path,
            resp.status().as_u16(),
            format!("{:.2?}", start.elapsed())
        );

        Ok(resp.map(|b| BoxBody::new(b.map_err(std::io::Error::other))))
    }
}

async fn tunnel(upgraded: Upgraded, addr: String, client_addr: SocketAddr) -> Result<()> {
    let mut server = TcpStream::connect(&addr).await?;
    tracing::info!("{} {} → connecting to {}", "[HTTP]", client_addr, addr);
    let start = std::time::Instant::now();
    let (tx, rx) = tokio::io::copy_bidirectional(&mut TokioIo::new(upgraded), &mut server).await?;
    tracing::info!(
        "{} {} → {} | Sent: {}, Received: {} | Duration: {}",
        "[HTTP]",
        client_addr,
        addr,
        format_bytes(tx),
        format_bytes(rx),
        format!("{:.2?}", start.elapsed())
    );
    Ok(())
}
