use axum::{
    Router,
    extract::{ConnectInfo, Request, State},
    http::{HeaderValue, StatusCode, Uri, header, uri::PathAndQuery},
    middleware::{self, Next},
    response::{IntoResponse, Response},
};
use hyper_util::rt::{TokioIo, TokioTimer};
use std::{collections::HashSet, io::Write, net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};
use tower::ServiceExt;
use tower_http::{
    services::{ServeDir, ServeFile},
    timeout::TimeoutLayer,
    trace::{DefaultOnFailure, DefaultOnResponse, TraceLayer},
};
use tracing::Level;

const HEADER_READ_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECTION_LIFETIME: Duration = Duration::from_secs(300);
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);

struct Config {
    public: PathBuf,
    canonical_host: Option<String>,
    pages: HashSet<Box<str>>,
}

const LEGACY_FILES: &[(&str, &str)] = &[
    ("63febd168a37babca623bf6b", "/files/cts-voip-wellington-www-cts-co-nz.pdf"),
    ("63febd152b5c50d6b4886674", "/files/cts-cloud-wellington-www-cts-co-nz.pdf"),
    ("63febd162b5c50d6a488662f", "/files/cts-voice-cts-business-lines-and-cts-broadband-wellington-www-cts-co-nz.pdf"),
    ("63febd15278f67ce68078684", "/files/cts-data-cabling-networks-wellington-www-cts-co-nz.pdf"),
    ("63fe5d84278f67cb0b0c733b", "/files/ipecs-ucp-datasheet-web-100-600.pdf"),
    ("63fe5f4f278f67cb0b0c7346", "/files/ip-dect-100-series-v1-datasheet-cts2.pdf"),
    ("63fe5fbf278f67caf30c7363", "/files/ipecs-1000i-series-datasheet-v-1-0-cts3.pdf"),
    ("63fe68038a37bab792e74e6c", "/files/ip-dect-100-series-v1-datasheet-cts4.pdf"),
    ("63fe6862278f67cae30c73fa", "/files/grandstream-datasheet-gxp1625-cts5.pdf"),
    ("63fe68bf2b5c50d372a9f1d2", "/files/gxp2170-datasheet-cts6.pdf"),
    ("63fe6b722b5c50d372a9f1dc", "/files/datasheet-dp752-cts9.pdf"),
    ("63fe6bb72b5c50d35aa9f179", "/files/datasheet-dp722-cts10.pdf"),
    ("63fe6bed2b5c50d35aa9f17e", "/files/cs500-product-brochure-cts11.pdf"),
    ("63fe6c412b5c50d372a9f1e3", "/files/74088-cv-ds-2cd2347g1-l-colourvu-4mp-fixed-turret-12.pdf"),
];

const PAGE_EXTENSIONS: [&str; 6] = [".html", ".htm", ".php", ".asp", ".aspx", ".shtml"];
const PRECOMPRESSED_EXTENSIONS: [&str; 6] = ["html", "css", "js", "svg", "xml", "txt"];

async fn site(State(config): State<Arc<Config>>, mut request: Request, next: Next) -> Response {
    let redirect = |location: HeaderValue| {
        (
            StatusCode::MOVED_PERMANENTLY,
            [(header::LOCATION, location), (header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=86400"))],
        )
            .into_response()
    };
    let uri = request.uri();
    let path = uri.path();
    let query = uri.query();

    if let Some(canonical) = &config.canonical_host {
        let host_matches = request
            .headers()
            .get(header::HOST)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.split_once(':').map_or(v, |(host, _)| host).eq_ignore_ascii_case(canonical));
        if !host_matches {
            let path_and_query = uri.path_and_query().map_or("/", |pq| pq.as_str());
            return redirect(HeaderValue::try_from(format!("https://{canonical}{path_and_query}")).unwrap());
        }
    }

    if path == "/" {
        if let Some(action) = query.unwrap_or("").split('&').find_map(|pair| pair.strip_prefix("Action=")) {
            return redirect(HeaderValue::from_static(match action {
                "PrivPol" => "/privacy-policy",
                "SiteMap" => "/site-map",
                _ => "/",
            }));
        }
    }

    if let Some(id) = path.strip_prefix("/files/") {
        if let Some((_, target)) = LEGACY_FILES.iter().find(|(legacy, _)| *legacy == id) {
            return redirect(HeaderValue::from_static(target));
        }
    }

    let last = path.rsplit('/').next().unwrap_or("");
    let has_page_extension = |name: &str| {
        PAGE_EXTENSIONS.iter().any(|ext| {
            name.len() >= ext.len() && name.as_bytes()[name.len() - ext.len()..].eq_ignore_ascii_case(ext.as_bytes())
        })
    };
    let mut rewritten = None;
    if path != "/" && (!last.contains('.') || has_page_extension(last)) {
        let already_clean = !last.is_empty()
            && !last.contains('.')
            && !path.bytes().any(|b| b.is_ascii_uppercase())
            && !matches!(path, "/index" | "/default" | "/home");
        if already_clean {
            let mut target = String::with_capacity(path.len() + 6 + query.map_or(0, str::len));
            target.push_str(path);
            target.push_str(".html");
            if let Some(query) = query {
                target.push('?');
                target.push_str(query);
            }
            rewritten = Some(target);
        } else {
            let trimmed = path.trim_matches('/');
            let mut clean = String::with_capacity(trimmed.len() + 1);
            clean.push('/');
            clean.push_str(trimmed);
            clean.make_ascii_lowercase();
            if let Some(ext) = PAGE_EXTENSIONS.iter().find(|ext| clean.ends_with(*ext)) {
                clean.truncate(clean.len() - ext.len());
            }
            if ["/", "/index", "/default", "/home"].contains(&clean.as_str()) {
                return redirect(HeaderValue::from_static("/"));
            }
            if config.pages.contains(clean.as_str()) {
                return redirect(HeaderValue::try_from(clean).unwrap());
            }
        }
    }

    if let Some(target) = rewritten {
        let mut parts = std::mem::take(request.uri_mut()).into_parts();
        parts.path_and_query = Some(PathAndQuery::try_from(target).unwrap());
        *request.uri_mut() = Uri::from_parts(parts).unwrap();
    }

    let path = request.uri().path();
    let page_request = path.ends_with('/') || path.ends_with(".html");
    let immutable = path.starts_with("/assets/fonts/") && path.ends_with(".woff2");
    let precompressed = page_request
        || path
            .rsplit_once('.')
            .is_some_and(|(_, extension)| PRECOMPRESSED_EXTENSIONS.contains(&extension));
    if !precompressed {
        request.headers_mut().remove(header::ACCEPT_ENCODING);
    }
    let mut response = next.run(request).await;
    let status = response.status();
    let cache_control = if page_request || !(status.is_success() || status == StatusCode::NOT_MODIFIED) {
        "no-cache"
    } else if immutable {
        "public, max-age=31536000, immutable"
    } else {
        "public, max-age=86400"
    };
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache_control));
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(header::REFERRER_POLICY, HeaderValue::from_static("strict-origin-when-cross-origin"));
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("SAMEORIGIN"));
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(
            "default-src 'self'; script-src 'self' 'inline-speculation-rules'; style-src 'self'; img-src 'self' data:; font-src 'self'; object-src 'none'; base-uri 'self'; form-action 'self'; frame-ancestors 'self'",
        ),
    );
    if config.canonical_host.is_some() {
        headers.insert(
            header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=31536000; includeSubDomains"),
        );
    }
    response
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stdout()))
        .init();
    let addr = std::env::var("CTS_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let public = PathBuf::from(std::env::var("CTS_PUBLIC").unwrap_or_else(|_| "public".into()));
    let started = std::time::Instant::now();
    let (mut compressed, mut failed) = (0usize, 0usize);
    let mut pages = HashSet::new();
    let mut pending = vec![public.clone()];
    while let Some(dir) = pending.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) => {
                tracing::warn!(dir = %dir.display(), %error, "cannot read directory for precompression");
                continue;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                pending.push(path);
                continue;
            }
            let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            if matches!(extension, "br" | "gz") {
                if !path.with_extension("").is_file() {
                    let _ = std::fs::remove_file(&path);
                }
                continue;
            }
            if extension == "html"
                && let Some(page) = path.strip_prefix(&public).ok().and_then(|p| p.to_str())
            {
                let mut name = String::with_capacity(page.len() - 4);
                name.push('/');
                name.push_str(&page[..page.len() - 5]);
                pages.insert(name.into_boxed_str());
            }
            if !PRECOMPRESSED_EXTENSIONS.contains(&extension) {
                continue;
            }
            let Ok(modified) = entry.metadata().and_then(|m| m.modified()) else {
                continue;
            };
            let mut source = None;
            for suffix in [".br", ".gz"] {
                let mut target = path.clone().into_os_string();
                target.push(suffix);
                let target = PathBuf::from(target);
                if std::fs::metadata(&target).and_then(|m| m.modified()).is_ok_and(|m| m == modified) {
                    continue;
                }
                if source.is_none() {
                    match std::fs::read(&path) {
                        Ok(data) => source = Some(data),
                        Err(error) => {
                            tracing::warn!(file = %path.display(), %error, "cannot read file for precompression");
                            failed += 1;
                            break;
                        }
                    }
                }
                let data = source.as_deref().unwrap();
                let mut output = Vec::with_capacity(data.len());
                if suffix == ".br" {
                    let params = brotli::enc::BrotliEncoderParams { quality: 11, size_hint: data.len(), ..Default::default() };
                    brotli::BrotliCompress(&mut &data[..], &mut output, &params).unwrap();
                } else {
                    let mut encoder = flate2::write::GzEncoder::new(output, flate2::Compression::best());
                    encoder.write_all(data).unwrap();
                    output = encoder.finish().unwrap();
                }
                if output.len() >= data.len() {
                    let _ = std::fs::remove_file(&target);
                    continue;
                }
                let result = std::fs::File::create(&target).and_then(|mut file| {
                    file.write_all(&output)?;
                    file.set_modified(modified)
                });
                match result {
                    Ok(()) => compressed += 1,
                    Err(error) => {
                        tracing::warn!(file = %target.display(), %error, "cannot write precompressed file");
                        let _ = std::fs::remove_file(&target);
                        failed += 1;
                    }
                }
            }
        }
    }
    tracing::info!(compressed, failed, elapsed = ?started.elapsed(), "precompressed assets");
    let config = Arc::new(Config {
        public,
        canonical_host: std::env::var("CTS_CANONICAL_HOST").ok().filter(|v| !v.is_empty()),
        pages,
    });
    let app = Router::new()
        .fallback_service(
            ServeDir::new(&config.public)
                .precompressed_br()
                .precompressed_gzip()
                .not_found_service(ServeFile::new(config.public.join("404.html")).precompressed_br().precompressed_gzip()),
        )
        .layer(middleware::from_fn_with_state(config.clone(), site))
        .layer(TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, REQUEST_TIMEOUT))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &Request| {
                    let value_of = |name: &header::HeaderName| request.headers().get(name).and_then(|v| v.to_str().ok()).unwrap_or("");
                    tracing::info_span!(
                        "request",
                        method = %request.method(),
                        uri = %request.uri(),
                        peer = %request.extensions().get::<ConnectInfo<SocketAddr>>().map_or(SocketAddr::from(([0, 0, 0, 0], 0)), |c| c.0),
                        forwarded_for = %value_of(&header::HeaderName::from_static("x-forwarded-for")),
                        host = %value_of(&header::HOST),
                        user_agent = %value_of(&header::USER_AGENT),
                    )
                })
                .on_response(DefaultOnResponse::new().level(Level::INFO))
                .on_failure(DefaultOnFailure::new().level(Level::WARN)),
        );
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    tracing::info!("listening on http://{addr}");
    let mut http = hyper::server::conn::http1::Builder::new();
    http.timer(TokioTimer::new()).header_read_timeout(HEADER_READ_TIMEOUT);
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(());
    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);
    loop {
        let (stream, peer) = tokio::select! {
            accepted = listener.accept() => match accepted {
                Ok(accepted) => accepted,
                Err(error) => {
                    tracing::warn!(%error, "accept failed");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
            },
            _ = &mut ctrl_c => break,
        };
        let _ = stream.set_nodelay(true);
        let app = app.clone();
        let http = http.clone();
        let mut shutdown_rx = shutdown_rx.clone();
        tokio::spawn(async move {
            let service = hyper::service::service_fn(move |mut request: hyper::Request<hyper::body::Incoming>| {
                request.extensions_mut().insert(ConnectInfo(peer));
                app.clone().oneshot(request)
            });
            let connection = http.serve_connection(TokioIo::new(stream), service);
            tokio::pin!(connection);
            let lifetime = tokio::time::sleep(CONNECTION_LIFETIME);
            tokio::pin!(lifetime);
            tokio::select! {
                result = connection.as_mut() => {
                    if let Err(error) = result {
                        tracing::debug!(%peer, %error, "connection closed");
                    }
                    return;
                }
                _ = &mut lifetime => {}
                _ = shutdown_rx.changed() => {}
            }
            connection.as_mut().graceful_shutdown();
            let _ = tokio::time::timeout(SHUTDOWN_GRACE, connection).await;
        });
    }
    drop(listener);
    drop(shutdown_rx);
    let _ = shutdown_tx.send(());
    shutdown_tx.closed().await;
}
