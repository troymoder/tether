use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};
use axum::Router;
use axum::extract::State;
use axum::http::header::{CACHE_CONTROL, HOST, IF_MODIFIED_SINCE, LAST_MODIFIED, ORIGIN};
use axum::http::{HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::post;
use protocol::{Admitted, Connect, access, encode};
use tokio::net::TcpListener;
use tower_http::services::ServeDir;

use crate::app::App;
use crate::rtc::{self, Hub};
use crate::trust::{self, Standing};

const NONCE_LIFE: Duration = Duration::from_secs(60);

/// The page is served from `web` unless another server hands it out
/// (`origins`), as rspack's dev server does in the dev stack. A request for the
/// page then redirects to that server, whose copy is the current one.
pub(crate) fn router(app: &Arc<App>, web: Option<PathBuf>) -> Result<Router> {
    let router = Router::new()
        .route(protocol::HELLO, post(hello))
        .route(protocol::CONNECT, post(connect));

    let router = match app.origins.first() {
        Some(origin) => {
            let page = format!("http://{origin}/");

            router.fallback(move || async move { Redirect::temporary(&page) })
        }
        None => {
            let web = web.context("web is not set: the folder with the built page")?;
            // The nix build puts compressed copies beside each file. Over
            // plain HTTP away from localhost, browsers only accept gzip.
            let page = Router::new()
                .fallback_service(
                    ServeDir::new(web)
                        .precompressed_br()
                        .precompressed_zstd()
                        .precompressed_gzip(),
                )
                .layer(middleware::from_fn(caching));

            router.fallback_service(page)
        }
    };

    Ok(router
        .layer(middleware::from_fn_with_state(app.clone(), same_origin))
        .with_state(app.clone()))
}

pub(crate) async fn serve(listener: TcpListener, router: Router, hub: Hub) -> Result<()> {
    loop {
        let (stream, from) = listener.accept().await?;
        let router = router.clone();
        let hub = hub.clone();

        let _ = stream.set_nodelay(true);

        tokio::spawn(async move {
            if !rtc::is_http(&stream).await.unwrap_or(false) {
                hub.tcp(stream, from).await;
                return;
            }

            let service = hyper_util::service::TowerToHyperService::new(router);
            let io = hyper_util::rt::TokioIo::new(stream);

            if let Err(error) = hyper::server::conn::http1::Builder::new()
                .serve_connection(io, service)
                .await
            {
                tracing::debug!("http failed: {error}");
            }
        });
    }
}

/// Refuses a request that another site's page made in the user's browser.
/// Browsers send the origin of such a request, and for this page it is the
/// address it was asked on. Without this check any page the user visits
/// could open the socket and drive the devices.
///
/// A site that points its own name at 127.0.0.1 (DNS rebinding) passes this
/// check, but has no key tetherd trusts, so it only gets as far as asking in
/// Approvals.
async fn same_origin(
    State(app): State<Arc<App>>,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    let headers = request.headers();
    let header = |name| headers.get(name).and_then(|value| value.to_str().ok());
    let foreign = header(ORIGIN).is_some_and(|origin| {
        let from = origin.split_once("://").map(|(_, host)| host);
        let known = from.is_some_and(|from| app.origins.iter().any(|origin| origin == from));

        from.is_none() || (from != header(HOST) && !known)
    });

    if foreign {
        return (StatusCode::FORBIDDEN, "This request came from another site").into_response();
    }

    next.run(request).await
}

/// The web build puts a hash of a file's content in its name
/// (crates/web/rspack.config.ts), so those are kept for good and the rest are
/// revalidated each time. Dates are ignored on both sides because the nix build
/// dates every file 1970, which would make a new build look unchanged.
async fn caching(mut request: axum::extract::Request, next: Next) -> Response {
    request.headers_mut().remove(IF_MODIFIED_SINCE);

    let fresh = unhashed(request.uri().path());
    let mut response = next.run(request).await;
    let headers = response.headers_mut();

    headers.remove(LAST_MODIFIED);
    headers.insert(
        CACHE_CONTROL,
        HeaderValue::from_static(if fresh {
            "no-cache"
        } else {
            "public, max-age=31536000, immutable"
        }),
    );

    response
}

fn unhashed(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or_default();

    name.is_empty() || name == "index.html"
}

async fn hello(State(app): State<Arc<App>>) -> Vec<u8> {
    let nonce = trust::random::<32>().to_vec();
    let mut nonces = app.nonces.lock().unwrap();

    // Anyone who reaches the port can ask for one, so stale nonces are
    // dropped and at most 256 are kept.
    nonces.retain(|_, at| at.elapsed() < NONCE_LIFE);

    if nonces.len() < 256 {
        nonces.insert(nonce.clone(), Instant::now());
    }

    encode(&protocol::Hello {
        nonce,
        stun: app.hub.stun().map(str::to_owned),
    })
}

async fn connect(State(app): State<Arc<App>>, body: axum::body::Bytes) -> Response {
    match admit(&app, &body).await {
        Ok(admitted) => encode(&admitted).into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, format!("{error:#}")).into_response(),
    }
}

async fn admit(app: &Arc<App>, body: &[u8]) -> Result<Admitted> {
    let asked = access::<Connect>(body).map_err(anyhow::Error::msg)?;
    let fresh = app
        .nonces
        .lock()
        .unwrap()
        .remove(asked.nonce.as_slice())
        .is_some_and(|at| at.elapsed() < NONCE_LIFE);

    ensure!(fresh, "trust failed: stale nonce");

    let proof = asked.proof.as_ref().map(|proof| proof.as_slice());
    let signed = protocol::signed(&asked.nonce, &asked.offer, proof);
    // The name and kind are the client's own claim and are only displayed.
    let short = |text: &str| text.chars().take(80).collect::<String>();
    let client = protocol::Client {
        print: trust::check(&asked.key, &signed, &asked.signature)?,
        name: short(&asked.name),
        kind: short(&asked.kind),
        at: 0,
    };
    // The setup code is never sent. The proof only shows the client knows it,
    // bound to this nonce and this offer.
    let missed = match proof {
        Some(proof) => {
            let knows =
                |code: &str| protocol::code_proof(code, &asked.nonce, &asked.offer) == proof;

            !app.trust.claim(&client, knows).await?
        }
        None => false,
    };

    match app.trust.standing(&client, missed) {
        Standing::Gate(gate) => Ok(Admitted::Gate(gate)),
        Standing::Trusted => {
            app.trust.used(&client.print).await;

            let (peer, answer, hearing) = app.hub.accept(&asked.offer)?;
            let you = client.print.clone();

            tracing::debug!("webrtc {peer}: {} let in", client.name);
            tokio::spawn(crate::client::converse(
                app.clone(),
                Arc::new(client),
                peer,
                hearing,
            ));

            Ok(Admitted::Answer { answer, you })
        }
    }
}
