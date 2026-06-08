// valkyrin-server/src/lib.rs
use axum::{
    body::Body,
    http::{header, Response, StatusCode},
    response::IntoResponse,
    routing::get,
    Router,
};
// Fix: Bring the explicit structural Embed trait module scope forward
use rust_embed::{Embed, RustEmbed};

#[derive(RustEmbed)]
#[folder = "../valkyrin-ui/dist/"]
struct Assets;

pub fn create_router() -> Router {
    Router::new()
        .route("/", get(serve_index))
        .route("/{*path}", get(serve_assets))
}

async fn serve_index() -> impl IntoResponse {
    serve_asset("index.html")
}

async fn serve_assets(axum::extract::Path(path): axum::extract::Path<String>) -> impl IntoResponse {
    serve_asset(&path)
}

fn serve_asset(path: &str) -> Response<Body> {
    // Fix: Validated through Embed trait linkage interface logic
    match Assets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(content.data))
                .unwrap()
        }
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Asset not found"))
            .unwrap(),
    }
}
