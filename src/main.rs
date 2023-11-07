use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use axum::extract::Host;
use axum::handler::HandlerWithoutStateExt;
use axum::http::{StatusCode, Uri};
use axum::http::uri::{PathAndQuery, Scheme};
use axum::response::Redirect;
use axum::Router;
use axum::routing::get;
use axum_server::tls_rustls::RustlsConfig;
use tracing_subscriber::prelude::__tracing_subscriber_SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// 端口号
#[derive(Copy, Clone, Debug)]
pub struct Ports {
    pub http: u16,
    pub https: u16,
}

#[tokio::main]
async fn main() {
    // 注册日志
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "https_example".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
    // 事先定义好http和https的两个端口
    let ports = Ports {
        http: 7878,
        https: 3000,
    };
    // 将http重定向为https
    tokio::spawn(redirect_http_to_https(ports));
    // 加入openssl 证书
    let config = RustlsConfig::from_pem_file(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("self_signed_certs")
            .join("cert.pem"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("self_signed_certs")
            .join("key.pem"),
    )
        .await
        .unwrap();

    let app = Router::new().route("/",get(handler));
    // 启动https server
    let addr = SocketAddr::from(([127, 0, 0, 1], ports.https));
    tracing::debug!("listening on {addr}");
    axum_server::bind_rustls(addr, config)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn handler<'a>() -> &'a str {
    "Hello, World!"
}

/// 将http重定向为https
async fn redirect_http_to_https(ports: Ports) {
    // 将http的消息头转换为https
    fn make_https(host: String, uri: Uri, ports: Ports) -> anyhow::Result<Uri> {
        // 设置https的消息头
        let mut parts = uri.into_parts();
        parts.scheme = Some(Scheme::HTTPS);
        // 如果没有传入访问route，就默认为 /
        if parts.path_and_query.is_none() {
            parts.path_and_query = Some("/".parse().unwrap())
        }
        // 将http端口替换为https端口
        let https_host = host.replace(&ports.http.to_string(), &ports.https.to_string());
        parts.authority = Some(https_host.parse()?);
        Ok(Uri::from_parts(parts)?)
    }
    // 重定向
    let redirect = move |Host(host):Host,uri:Uri| async move {
        match make_https(host,uri,ports) {
            Ok(uri) => {
                Ok(Redirect::permanent(&uri.to_string()))
            }
            Err(e) => {
                // 写入日志
                tracing::warn!(%e, "failed to convert URI to HTTPS");
                Err(StatusCode::BAD_REQUEST)
            }
        }
    };
    // 在http的路由上重定向到https，相当于还要启动一个https的服务
    let addr = SocketAddr::new(IpAddr::from([127,0,0,1]),ports.http);
    axum::Server::bind(&addr)
        .serve(redirect.into_make_service()).await.unwrap();
}