//! Puts the Tauri commands behind plain HTTP, for the Playwright suite.
//!
//! Playwright cannot drive a Tauri window — the app runs inside WKWebView on
//! macOS, and there is no WebDriver for it. So the end-to-end suite loads the
//! real front end in Chromium and points its IPC at this process, which calls
//! the same `AppState` methods `src/commands.rs` exposes to Tauri, against a
//! real migrated SQLite database.
//!
//! What that does and does not prove: the React app, the command layer, the
//! repository, the schema and the domain rules are all genuinely exercised.
//! Tauri's own IPC transport and the platform webviews are not.
//!
//! Dev-only. It is an example, not a binary, so `tiny_http` never reaches the
//! shipped app. Run it with:
//!
//! ```sh
//! cargo run --example devserver          # 127.0.0.1:4599
//! TYMIO_DEVSERVER_PORT=5000 cargo run --example devserver
//! ```

use std::io::Read;
use std::sync::{Arc, RwLock};

use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use tokio::runtime::Runtime;

use tymio_lib::commands::AppState;
use tymio_lib::db::Db;
use tymio_lib::error::AppError;

const DEFAULT_PORT: u16 = 4599;

fn main() {
    let port: u16 = std::env::var("TYMIO_DEVSERVER_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_PORT);

    let runtime = Runtime::new().expect("tokio runtime starts");
    let state = Arc::new(RwLock::new(fresh_state(&runtime)));

    let address = format!("127.0.0.1:{port}");
    let server = tiny_http::Server::http(&address).expect("the dev server can bind");
    // The Playwright config waits for this line before starting Vite.
    println!("tymio devserver listening on http://{address}");

    for mut request in server.incoming_requests() {
        let path = request.url().trim_start_matches('/').to_owned();
        let command = path.strip_prefix("ipc/").unwrap_or(&path).to_owned();

        if request.method() == &tiny_http::Method::Options {
            let _ = request.respond(empty_response());
            continue;
        }

        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);
        let args: Value = serde_json::from_str(&body).unwrap_or_else(|_| json!({}));

        let outcome = dispatch(&runtime, &state, &command, &args);
        let (status, payload) = match outcome {
            Ok(value) => (200, value),
            Err(error) => (
                400,
                serde_json::to_value(&error).unwrap_or_else(|_| json!({ "message": "unknown" })),
            ),
        };

        let _ = request.respond(json_response(status, &payload));
    }
}

fn fresh_state(runtime: &Runtime) -> AppState {
    // In-memory: every run of the suite starts from an empty database, and
    // nothing is left on disk afterwards.
    let db = runtime.block_on(Db::in_memory()).expect("in-memory database opens");
    AppState::new(db)
}

/// Pulls one named argument out of the JSON body.
///
/// The keys are the camelCase ones the front end sends; Tauri does that
/// conversion itself, and this stands in for it.
fn arg<T: DeserializeOwned + Default>(args: &Value, key: &str) -> Result<T, AppError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(T::default()),
        Some(value) => serde_json::from_value(value.clone())
            .map_err(|e| AppError::Storage(format!("bad argument {key:?}: {e}"))),
    }
}

fn required<T: DeserializeOwned>(args: &Value, key: &str) -> Result<T, AppError> {
    let value = args
        .get(key)
        .ok_or_else(|| AppError::Storage(format!("missing argument {key:?}")))?;
    serde_json::from_value(value.clone())
        .map_err(|e| AppError::Storage(format!("bad argument {key:?}: {e}")))
}

fn dispatch(
    runtime: &Runtime,
    state: &Arc<RwLock<AppState>>,
    command: &str,
    args: &Value,
) -> Result<Value, AppError> {
    // Test-only: drops the database and migrates a new one, so each spec file
    // starts from a known-empty state.
    if command == "__reset" {
        *state.write().expect("state lock") = fresh_state(runtime);
        return Ok(Value::Null);
    }

    let state = state.read().expect("state lock").clone();

    let value = match command {
        "list_projects" => encode(runtime.block_on(state.list_projects(arg(args, "filter")?))?),
        "get_project" => encode(runtime.block_on(state.get_project(required(args, "id")?))?),
        "create_project" => {
            encode(runtime.block_on(state.create_project(required(args, "draft")?))?)
        }
        "update_project" => encode(
            runtime.block_on(state.update_project(required(args, "id")?, required(args, "draft")?))?,
        ),
        "delete_project" => encode(runtime.block_on(state.delete_project(required(args, "id")?))?),
        "portfolio_stats" => encode(runtime.block_on(state.portfolio_stats())?),
        "project_stats" => encode(
            runtime.block_on(state.project_stats(required(args, "id")?, arg(args, "asOf")?))?,
        ),
        "project_holidays" => {
            encode(runtime.block_on(state.project_holidays(required(args, "id")?))?)
        }
        "add_project_holiday" => encode(
            runtime.block_on(
                state.add_project_holiday(required(args, "id")?, required(args, "holiday")?),
            )?,
        ),
        "remove_project_holiday" => encode(
            runtime.block_on(
                state.remove_project_holiday(required(args, "id")?, required(args, "holiday")?),
            )?,
        ),
        "recent_activity" => encode(runtime.block_on(state.recent_activity(arg(args, "limit")?))?),
        "create_employee" => encode(
            runtime.block_on(
                state.create_employee(required(args, "project")?, required(args, "draft")?),
            )?,
        ),
        "get_employee" => encode(runtime.block_on(state.get_employee(required(args, "id")?))?),
        "list_employees" => {
            encode(runtime.block_on(state.list_employees(arg(args, "filter")?))?)
        }
        "update_employee" => encode(
            runtime
                .block_on(state.update_employee(required(args, "id")?, required(args, "draft")?))?,
        ),
        "delete_employee" => {
            encode(runtime.block_on(state.delete_employee(required(args, "id")?))?)
        }
        "attendance_sheet" => encode(
            runtime.block_on(
                state.attendance_sheet(required(args, "project")?, required(args, "period")?),
            )?,
        ),
        "attendance_entry" => encode(
            runtime.block_on(
                state.attendance_entry(required(args, "employee")?, required(args, "period")?),
            )?,
        ),
        "record_attendance" => encode(
            runtime.block_on(state.record_attendance(
                required(args, "employee")?,
                required(args, "period")?,
                required(args, "draft")?,
            ))?,
        ),
        "clear_attendance" => encode(
            runtime.block_on(
                state.clear_attendance(required(args, "employee")?, required(args, "period")?),
            )?,
        ),
        "fill_attendance_from_schedule" => encode(
            runtime.block_on(state.fill_attendance_from_schedule(
                required(args, "project")?,
                required(args, "period")?,
            ))?,
        ),
        "employee_attendance" => {
            encode(runtime.block_on(state.employee_attendance(required(args, "employee")?))?)
        }
        "employee_stats" => encode(
            runtime.block_on(state.employee_stats(required(args, "id")?, arg(args, "asOf")?))?,
        ),
        unknown => {
            return Err(AppError::Storage(format!("no such command: {unknown}")));
        }
    };

    value
}

fn encode<T: serde::Serialize>(value: T) -> Result<Value, AppError> {
    serde_json::to_value(value).map_err(|e| AppError::Storage(format!("cannot encode reply: {e}")))
}

fn header(name: &str, value: &str) -> tiny_http::Header {
    tiny_http::Header::from_bytes(name.as_bytes(), value.as_bytes())
        .expect("static header is well formed")
}

fn cors() -> Vec<tiny_http::Header> {
    vec![
        header("Access-Control-Allow-Origin", "*"),
        header("Access-Control-Allow-Headers", "content-type"),
        header("Access-Control-Allow-Methods", "POST, OPTIONS"),
    ]
}

fn empty_response() -> tiny_http::Response<std::io::Empty> {
    tiny_http::Response::empty(204).with_headers(cors())
}

fn json_response(status: u16, payload: &Value) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    let body = serde_json::to_vec(payload).unwrap_or_else(|_| b"null".to_vec());
    let mut response = tiny_http::Response::from_data(body).with_status_code(status);
    for h in cors() {
        response.add_header(h);
    }
    response.add_header(header("Content-Type", "application/json"));
    response
}

trait WithHeaders {
    fn with_headers(self, headers: Vec<tiny_http::Header>) -> Self;
}

impl<R: Read> WithHeaders for tiny_http::Response<R> {
    fn with_headers(mut self, headers: Vec<tiny_http::Header>) -> Self {
        for h in headers {
            self.add_header(h);
        }
        self
    }
}
