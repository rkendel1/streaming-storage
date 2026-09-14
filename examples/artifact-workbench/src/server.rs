use crate::{ArtifactWorkbench, BuildOptions, WorkbenchError};
use serde::Deserialize;
use serde::Serialize;
use std::env;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

type SharedWorkbench = Arc<Mutex<ArtifactWorkbench>>;

#[derive(Deserialize)]
struct ImportRequest {
    path: String,
}

#[derive(Deserialize)]
struct OutputRequest {
    output: String,
}

#[derive(Deserialize)]
struct TargetRequest {
    target: String,
}

#[derive(Deserialize)]
struct ExecuteRequest {
    executable: String,
}

#[derive(Deserialize)]
struct RecoverRequest {
    identity: String,
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

pub fn run_from_env() -> Result<(), Box<dyn std::error::Error>> {
    let address = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8787".to_string());
    let store_root = env::var_os("ARTIFACT_WORKBENCH_STORE")
        .map(PathBuf::from)
        .unwrap_or_else(|| env::temp_dir().join("artifact-workbench-store"));
    let output_root = env::var_os("ARTIFACT_WORKBENCH_OUTPUTS")
        .map(PathBuf::from)
        .unwrap_or_else(|| env::temp_dir().join("artifact-workbench-outputs"));
    let workbench = ArtifactWorkbench::new(store_root, output_root)?;
    serve(&address, workbench)
}

pub fn serve(
    address: &str,
    workbench: ArtifactWorkbench,
) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(address)?;
    let shared = Arc::new(Mutex::new(workbench));
    eprintln!("Artifact Workbench listening on http://{address}");
    for stream in listener.incoming() {
        let stream = stream?;
        let shared = Arc::clone(&shared);
        if let Err(error) = handle_connection(stream, shared) {
            eprintln!("workbench request failed: {error}");
        }
    }
    Ok(())
}

fn handle_connection(
    mut stream: TcpStream,
    workbench: SharedWorkbench,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if request_complete(&buffer) {
            break;
        }
    }

    let request = HttpRequest::parse(&buffer)?;
    let response = route(request, workbench);
    stream.write_all(&response.to_bytes())?;
    stream.flush()?;
    Ok(())
}

fn request_complete(buffer: &[u8]) -> bool {
    let Some(header_end) = find_header_end(buffer) else {
        return false;
    };
    let headers = String::from_utf8_lossy(&buffer[..header_end]);
    let content_length = headers
        .lines()
        .find_map(|line| line.strip_prefix("Content-Length:"))
        .and_then(|value| value.trim().parse::<usize>().ok())
        .unwrap_or(0);
    buffer.len() >= header_end + 4 + content_length
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

struct HttpRequest {
    method: String,
    path: String,
    body: Vec<u8>,
}

impl HttpRequest {
    fn parse(buffer: &[u8]) -> Result<Self, WorkbenchError> {
        let header_end = find_header_end(buffer).ok_or_else(|| {
            WorkbenchError::InvalidSelection("malformed HTTP request".to_string())
        })?;
        let head = String::from_utf8_lossy(&buffer[..header_end]);
        let mut lines = head.lines();
        let request_line = lines.next().ok_or_else(|| {
            WorkbenchError::InvalidSelection("missing HTTP request line".to_string())
        })?;
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or_default().to_string();
        let path = parts.next().unwrap_or_default().to_string();
        let content_length = lines
            .find_map(|line| line.strip_prefix("Content-Length:"))
            .and_then(|value| value.trim().parse::<usize>().ok())
            .unwrap_or(0);
        let body_start = header_end + 4;
        let body_end = body_start + content_length;
        let body = buffer
            .get(body_start..body_end)
            .ok_or_else(|| WorkbenchError::InvalidSelection("truncated HTTP body".to_string()))?
            .to_vec();
        Ok(Self { method, path, body })
    }
}

fn route(request: HttpRequest, workbench: SharedWorkbench) -> HttpResponse {
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/") => HttpResponse::html(include_str!("ui/index.html")),
        ("GET", "/app.js") => HttpResponse::javascript(include_str!("ui/app.js")),
        ("GET", "/styles.css") => HttpResponse::css(include_str!("ui/styles.css")),
        ("GET", "/api/capabilities") => json_response(Ok(ArtifactWorkbench::capabilities())),
        ("POST", "/api/import") => with_workbench(&request.body, workbench, |body, workbench| {
            let request: ImportRequest = serde_json::from_slice(body)?;
            workbench.import_source(request.path)
        }),
        ("POST", "/api/build") => with_workbench(&request.body, workbench, |body, workbench| {
            let request: BuildOptions = if body.is_empty() {
                BuildOptions::default()
            } else {
                serde_json::from_slice(body)?
            };
            workbench.build_artifact(request)
        }),
        ("POST", "/api/output") => with_workbench(&request.body, workbench, |body, workbench| {
            let request: OutputRequest = serde_json::from_slice(body)?;
            workbench.select_output(&request.output)
        }),
        ("POST", "/api/target") => with_workbench(&request.body, workbench, |body, workbench| {
            let request: TargetRequest = serde_json::from_slice(body)?;
            workbench.select_target(&request.target)
        }),
        ("POST", "/api/execute") => with_workbench(&request.body, workbench, |body, workbench| {
            let request: ExecuteRequest = serde_json::from_slice(body)?;
            workbench.execute(&request.executable)
        }),
        ("POST", "/api/recover") => with_workbench(&request.body, workbench, |body, workbench| {
            let request: RecoverRequest = serde_json::from_slice(body)?;
            workbench.recover_artifact(&request.identity)
        }),
        ("POST", "/api/reset") => {
            let mut workbench = workbench
                .lock()
                .expect("workbench mutex should not be poisoned");
            workbench.reset_operation();
            json_response(Ok(workbench.summary()))
        }
        _ => HttpResponse::not_found(),
    }
}

fn with_workbench<T, F>(body: &[u8], workbench: SharedWorkbench, action: F) -> HttpResponse
where
    T: Serialize,
    F: FnOnce(&[u8], &mut ArtifactWorkbench) -> Result<T, WorkbenchError>,
{
    let mut workbench = workbench
        .lock()
        .expect("workbench mutex should not be poisoned");
    json_response(action(body, &mut workbench))
}

fn json_response<T: Serialize>(result: Result<T, WorkbenchError>) -> HttpResponse {
    match result {
        Ok(value) => HttpResponse::json(200, &value),
        Err(error) => HttpResponse::json(
            400,
            &ErrorBody {
                error: error.to_string(),
            },
        ),
    }
}

struct HttpResponse {
    status: u16,
    reason: &'static str,
    content_type: &'static str,
    body: Vec<u8>,
}

impl HttpResponse {
    fn html(body: &str) -> Self {
        Self::new(
            200,
            "OK",
            "text/html; charset=utf-8",
            body.as_bytes().to_vec(),
        )
    }

    fn css(body: &str) -> Self {
        Self::new(
            200,
            "OK",
            "text/css; charset=utf-8",
            body.as_bytes().to_vec(),
        )
    }

    fn javascript(body: &str) -> Self {
        Self::new(
            200,
            "OK",
            "text/javascript; charset=utf-8",
            body.as_bytes().to_vec(),
        )
    }

    fn json<T: Serialize>(status: u16, body: &T) -> Self {
        let reason = if status == 200 { "OK" } else { "Bad Request" };
        Self::new(
            status,
            reason,
            "application/json; charset=utf-8",
            serde_json::to_vec(body).expect("json response should serialize"),
        )
    }

    fn not_found() -> Self {
        Self::new(
            404,
            "Not Found",
            "text/plain; charset=utf-8",
            b"not found".to_vec(),
        )
    }

    fn new(status: u16, reason: &'static str, content_type: &'static str, body: Vec<u8>) -> Self {
        Self {
            status,
            reason,
            content_type,
            body,
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut response = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            self.status,
            self.reason,
            self.content_type,
            self.body.len()
        )
        .into_bytes();
        response.extend_from_slice(&self.body);
        response
    }
}

impl From<serde_json::Error> for WorkbenchError {
    fn from(value: serde_json::Error) -> Self {
        Self::InvalidSelection(value.to_string())
    }
}
