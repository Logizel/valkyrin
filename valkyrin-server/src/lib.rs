// valkyrin-server/src/lib.rs
use axum::{
    Router,
    body::Body,
    extract::Json,
    http::{Response, StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use rust_embed::RustEmbed;
use std::fs;

#[derive(RustEmbed)]
#[folder = "../valkyrin-ui/dist/"]
struct Assets;

pub fn create_router() -> Router {
    Router::new()
        .route("/api/save", post(save_blueprint)) // NEW: The API bridge endpoint
        .route("/", get(serve_index))
        .route("/api/load", get(load_blueprint))
        .route("/{*path}", get(serve_assets))
}
async fn load_blueprint() -> impl IntoResponse {
    match fs::read_to_string("schema.vdb.json") {
        Ok(content) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(content))
            .unwrap(),
        Err(_) => {
            // If the file doesn't exist yet, return an empty canvas
            let empty = r#"{"tables":[],"relations":[]}"#;
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(empty))
                .unwrap()
        }
    }
}
/// Receives the JSON payload from React and writes it to the local disk.
async fn save_blueprint(Json(payload): Json<serde_json::Value>) -> impl IntoResponse {
    // Format the JSON beautifully so it looks clean in the user's Git commits
    let pretty_json = match serde_json::to_string_pretty(&payload) {
        Ok(json) => json,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to serialize payload",
            )
                .into_response();
        }
    };

    // Write the JSON directly to the user's current directory
    match fs::write("schema.vdb.json", pretty_json) {
        Ok(_) => {
            println!("💾 Schema successfully saved to disk.");
            (StatusCode::OK, "Saved").into_response()
        }
        Err(e) => {
            eprintln!("❌ Failed to write schema to disk: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to write to disk").into_response()
        }
    }
}

async fn serve_index() -> impl IntoResponse {
    serve_asset("index.html")
}

async fn serve_assets(axum::extract::Path(path): axum::extract::Path<String>) -> impl IntoResponse {
    serve_asset(&path)
}

fn serve_asset(path: &str) -> Response<Body> {
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

pub async fn start_server(port: u16) -> std::io::Result<()> {
    let app = create_router();
    let addr = format!("127.0.0.1:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await
}
