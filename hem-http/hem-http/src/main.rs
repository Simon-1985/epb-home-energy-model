use axum::{
    extract::Json,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use hem::output::Output;
use hem::read_weather_file::weather_data_to_vec;
use hem::{run_project, ProjectFlags};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufReader, Cursor, ErrorKind, Write};
use std::net::SocketAddr;
use std::str::from_utf8;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

// Embed the UK weather file at compile time
// This is the London Gatwick weather file - good default for SE England
const WEATHER_DATA: &str = include_str!("../weather/GBR_ENG_London.Gatwick.AP.037760_TMYx.2007-2021.epw");

#[derive(Debug, Deserialize)]
struct CalculateRequest {
    input: Value,
    #[serde(default)]
    region: Option<String>,
}

#[derive(Debug, Serialize)]
struct CalculateResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    errors: Option<Vec<ErrorDetail>>,
}

#[derive(Debug, Serialize)]
struct ErrorDetail {
    id: String,
    status: String,
    detail: String,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: String,
    version: String,
}

async fn health() -> impl IntoResponse {
    Json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

async fn calculate(Json(payload): Json<CalculateRequest>) -> impl IntoResponse {
    let input_json = match serde_json::to_string(&payload.input) {
        Ok(json) => json,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(CalculateResponse {
                    success: false,
                    data: None,
                    output: None,
                    errors: Some(vec![ErrorDetail {
                        id: Uuid::new_v4().to_string(),
                        status: "400".to_string(),
                        detail: format!("Invalid input JSON: {}", e),
                    }]),
                }),
            );
        }
    };

    let output = HttpOutput::new();

    // Parse weather data
    let external_conditions = weather_data_to_vec(BufReader::new(Cursor::new(WEATHER_DATA))).ok();

    // Run the HEM calculation
    match run_project(
        input_json.as_bytes(),
        &output,
        external_conditions,
        None,
        &ProjectFlags::empty(),
    ) {
        Ok(Some(result)) => {
            // Convert HemResponse to JSON Value for the response
            let json_value = serde_json::to_value(&result).unwrap_or(json!({}));
            (
                StatusCode::OK,
                Json(CalculateResponse {
                    success: true,
                    data: Some(json_value),
                    output: None,
                    errors: None,
                }),
            )
        }
        Ok(None) => {
            let output_string = Arc::try_unwrap(output.0).unwrap().into_inner();
            (
                StatusCode::OK,
                Json(CalculateResponse {
                    success: true,
                    data: None,
                    output: Some(output_string),
                    errors: None,
                }),
            )
        }
        Err(e) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(CalculateResponse {
                success: false,
                data: None,
                output: None,
                errors: Some(vec![ErrorDetail {
                    id: Uuid::new_v4().to_string(),
                    status: "422".to_string(),
                    detail: e.to_string(),
                }]),
            }),
        ),
    }
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "hem_http=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Build the router
    let app = Router::new()
        .route("/health", get(health))
        .route("/calculate", post(calculate))
        .layer(TraceLayer::new_for_http())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );

    // Get port from environment or default to 8080
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("HEM HTTP API listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

/// Output handler for HTTP responses (mirrors the Lambda pattern)
#[derive(Debug)]
struct HttpOutput(Arc<Mutex<String>>);

impl HttpOutput {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(String::with_capacity(2usize.pow(22)))))
    }
}

impl Output for HttpOutput {
    fn writer_for_location_key(
        &self,
        location_key: &str,
        file_extension: &str,
    ) -> anyhow::Result<impl Write> {
        Ok(FileLikeStringWriter::new(
            self.0.clone(),
            location_key,
            file_extension,
        ))
    }
}

impl Output for &HttpOutput {
    fn writer_for_location_key(
        &self,
        location_key: &str,
        file_extension: &str,
    ) -> anyhow::Result<impl Write> {
        <HttpOutput as Output>::writer_for_location_key(self, location_key, file_extension)
    }
}

struct FileLikeStringWriter {
    string: Arc<Mutex<String>>,
    location_key: String,
    file_extension: String,
    has_output_file_header: bool,
}

impl FileLikeStringWriter {
    fn new(string: Arc<Mutex<String>>, location_key: &str, file_extension: &str) -> Self {
        Self {
            string,
            location_key: location_key.to_string(),
            file_extension: file_extension.to_string(),
            has_output_file_header: false,
        }
    }
}

impl Write for FileLikeStringWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if !self.has_output_file_header {
            let mut output_string = self.string.lock();
            if !output_string.is_empty() {
                output_string.push_str("\n\n");
            }
            output_string.push_str(
                format!(
                    "Writing out file '{}.{}':\n\n",
                    self.location_key, self.file_extension
                )
                .as_str(),
            );
            self.has_output_file_header = true;
        }
        let utf8 = match from_utf8(buf) {
            Ok(utf8) => utf8,
            Err(_) => {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    "Tried to write out invalid UTF-8.",
                ));
            }
        };
        self.string.lock().push_str(utf8);
        Ok(utf8.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
