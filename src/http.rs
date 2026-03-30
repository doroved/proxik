use crate::utils::{format_bytes, resolve_ipv4};
use anyhow::Result;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use http_body_util::{BodyExt, Full, combinators::BoxBody};
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, upgrade::Upgraded};
use hyper_util::rt::TokioIo;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};

/// Compatibility entrypoint for running the HTTP server
pub async fn run(
    bind_addr: &str,
    port: u16,
    username: Option<String>,
    password: Option<String>,
) -> Result<()> {
    HttpServer::new(bind_addr, port, username, password)
        .run()
        .await
}

struct HttpServer {
    bind_addr: String,
    port: u16,
    auth: Option<String>,
}

impl HttpServer {
    pub fn new(
        bind_addr: &str,
        port: u16,
        username: Option<String>,
        password: Option<String>,
    ) -> Self {
        let auth = match (username, password) {
            (Some(u), Some(p)) => Some(format!("{}:{}", u, p)),
            _ => None,
        };

        Self {
            bind_addr: bind_addr.to_string(),
            port,
            auth,
        }
    }

    pub async fn run(self) -> Result<()> {
        let listener = TcpListener::bind(&self.bind_addr).await?;
        tracing::info!(
            "[HTTP:{}] server listening on {}",
            self.port,
            listener.local_addr()?.to_string()
        );

        let server = Arc::new(self);

        loop {
            let (stream, addr) = listener.accept().await?;
            let srv = server.clone();

            tokio::spawn(async move {
                let _ = http1::Builder::new()
                    .preserve_header_case(true)
                    .title_case_headers(true)
                    .serve_connection(
                        TokioIo::new(stream),
                        service_fn(move |req| srv.clone().proxy(req, addr)),
                    )
                    .with_upgrades()
                    .await;
            });
        }
    }

    async fn proxy(
        self: Arc<Self>,
        req: Request<hyper::body::Incoming>,
        client_addr: SocketAddr,
    ) -> Result<Response<BoxBody<Bytes, hyper::Error>>, anyhow::Error> {
        if let Some(expected) = &self.auth {
            let authenticated = req
                .headers()
                .get(hyper::header::PROXY_AUTHORIZATION)
                .and_then(|h| h.to_str().ok())
                .and_then(|s| s.strip_prefix("Basic "))
                .and_then(|e| BASE64.decode(e).ok())
                .and_then(|d| String::from_utf8(d).ok())
                .as_ref()
                == Some(expected);

            if !authenticated {
                let resp = Response::builder()
                    .status(hyper::StatusCode::PROXY_AUTHENTICATION_REQUIRED)
                    .header(hyper::header::PROXY_AUTHENTICATE, "Basic realm=\"proxik\"")
                    .body(BoxBody::new(
                        Full::new(Bytes::from("Proxy Authentication Required"))
                            .map_err(|e| match e {}),
                    ))
                    .unwrap();
                return Ok(resp);
            }
        }

        if Method::CONNECT == req.method() {
            let addr = req
                .uri()
                .authority()
                .map(|a| a.to_string())
                .unwrap_or_default();
            let port = self.port;
            tokio::spawn(async move {
                if let Ok(upgraded) = hyper::upgrade::on(req).await {
                    let _ = Self::tunnel(upgraded, addr, client_addr, port).await;
                }
            });
            Ok(Response::new(BoxBody::new(
                Full::new(Bytes::new()).map_err(|e| match e {}),
            )))
        } else {
            let host = req.uri().host().unwrap_or_default();
            let req_port = req.uri().port_u16().unwrap_or(80);
            let target = format!("{}:{}", host, req_port);

            tracing::info!(
                "[HTTP:{}] {} → connecting to {}",
                self.port,
                client_addr,
                target
            );
            let start = std::time::Instant::now();
            let method = req.method().clone();
            let path = req
                .uri()
                .path_and_query()
                .map(|p| p.as_str())
                .unwrap_or(req.uri().path())
                .to_string();

            let stream = TcpStream::connect(resolve_ipv4(&target).await?).await?;
            let (mut sender, conn) =
                hyper::client::conn::http1::handshake(TokioIo::new(stream)).await?;
            tokio::spawn(async move {
                let _ = conn.await;
            });

            let resp = sender.send_request(req).await?;

            tracing::info!(
                "[HTTP:{}] {} → {} | {} {} | Status: {} | Duration: {}",
                self.port,
                client_addr,
                target,
                method.as_str(),
                path,
                resp.status().as_u16(),
                format!("{:.2?}", start.elapsed())
            );

            Ok(resp.map(|b| b.boxed()))
        }
    }

    async fn tunnel(
        upgraded: Upgraded,
        addr: String,
        client_addr: SocketAddr,
        proxy_port: u16,
    ) -> Result<()> {
        let mut server = TcpStream::connect(resolve_ipv4(&addr).await?).await?;
        tracing::info!(
            "[HTTP:{}] {} → connecting to {}",
            proxy_port,
            client_addr,
            addr
        );
        let start = std::time::Instant::now();
        let (tx, rx) =
            tokio::io::copy_bidirectional(&mut TokioIo::new(upgraded), &mut server).await?;
        tracing::info!(
            "[HTTP:{}] {} → {} | Sent: {}, Received: {} | Duration: {}",
            proxy_port,
            client_addr,
            addr,
            format_bytes(tx),
            format_bytes(rx),
            format!("{:.2?}", start.elapsed())
        );
        Ok(())
    }
}
