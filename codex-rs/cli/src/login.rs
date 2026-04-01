//! CLI login commands and their direct-user observability surfaces.
//!
//! The TUI path already installs a broader tracing stack with feedback, OpenTelemetry, and other
//! interactive-session layers. Direct `codex login` intentionally does less: it preserves the
//! existing stderr/browser UX and adds only a small file-backed tracing layer for login-specific
//! targets. Keeping that setup local avoids pulling the TUI's session-oriented logging machinery
//! into a one-shot CLI command while still producing a durable `codex-login.log` artifact that
//! support can request from users.

use codex_core::CodexAuth;
use codex_core::auth::AuthCredentialsStoreMode;
use codex_core::auth::AuthMode;
use codex_core::auth::CLIENT_ID;
use codex_core::auth::login_with_api_key;
use codex_core::auth::logout;
use codex_core::config::Config;
use codex_login::default_client::build_reqwest_client;
use codex_login::ServerOptions;
use codex_login::run_device_code_login;
use codex_login::run_login_server;
use codex_protocol::config_types::ForcedLoginMethod;
use codex_utils_cli::CliConfigOverrides;
use reqwest::StatusCode;
use serde::Deserialize;
use std::fs::OpenOptions;
use std::io::IsTerminal;
use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;
use tracing_appender::non_blocking;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

const CHATGPT_LOGIN_DISABLED_MESSAGE: &str =
    "Browser login is disabled. Use Father Paul API key login instead.";
const API_KEY_LOGIN_DISABLED_MESSAGE: &str =
    "API key login is required for FatherPaul Code.";
const LOGIN_SUCCESS_MESSAGE: &str = "Successfully logged in";
const FATHERPAUL_PORTAL_CLIENT_NAME: &str = "fatherpaul-code";

#[derive(Debug, Deserialize)]
struct FatherPaulDeviceStartResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    verification_uri_complete: String,
    expires_in: u64,
    interval: u64,
    polling_endpoint: String,
}

#[derive(Debug, Deserialize)]
struct FatherPaulDevicePollResponse {
    status: String,
    detail: Option<String>,
    interval: Option<u64>,
    access_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FatherPaulCliSessionUser {
    email: String,
    full_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FatherPaulCliSessionSubscription {
    plan: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FatherPaulCliSessionResponse {
    status: String,
    user: FatherPaulCliSessionUser,
    subscription: FatherPaulCliSessionSubscription,
    api_base_url: String,
    api_key: String,
}

/// Installs a small file-backed tracing layer for direct `codex login` flows.
///
/// This deliberately duplicates a narrow slice of the TUI logging setup instead of reusing it
/// wholesale. The TUI stack includes session-oriented layers that are valuable for interactive
/// runs but unnecessary for a one-shot login command. Keeping the direct CLI path local lets this
/// command produce a durable `codex-login.log` artifact without coupling it to the TUI's broader
/// telemetry and feedback initialization.
fn init_login_file_logging(config: &Config) -> Option<WorkerGuard> {
    let log_dir = match codex_core::config::log_dir(config) {
        Ok(log_dir) => log_dir,
        Err(err) => {
            eprintln!("Warning: failed to resolve login log directory: {err}");
            return None;
        }
    };

    if let Err(err) = std::fs::create_dir_all(&log_dir) {
        eprintln!(
            "Warning: failed to create login log directory {}: {err}",
            log_dir.display()
        );
        return None;
    }

    let mut log_file_opts = OpenOptions::new();
    log_file_opts.create(true).append(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        log_file_opts.mode(0o600);
    }

    let log_path = log_dir.join("codex-login.log");
    let log_file = match log_file_opts.open(&log_path) {
        Ok(log_file) => log_file,
        Err(err) => {
            eprintln!(
                "Warning: failed to open login log file {}: {err}",
                log_path.display()
            );
            return None;
        }
    };

    let (non_blocking, guard) = non_blocking(log_file);
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("codex_cli=info,codex_core=info,codex_login=info"));
    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        .with_target(true)
        .with_ansi(false)
        .with_filter(env_filter);

    // Direct `codex login` otherwise relies on ephemeral stderr and browser output.
    // Persist the same login targets to a file so support can inspect auth failures
    // without reproducing them through TUI or app-server.
    if let Err(err) = tracing_subscriber::registry().with(file_layer).try_init() {
        eprintln!(
            "Warning: failed to initialize login log file {}: {err}",
            log_path.display()
        );
        return None;
    }

    Some(guard)
}

fn print_login_server_start(actual_port: u16, auth_url: &str) {
    eprintln!(
        "Starting local login server on http://localhost:{actual_port}.\nIf your browser did not open, navigate to this URL to authenticate:\n\n{auth_url}\n\nOn a remote or headless machine? Use `fatherpaul-code login --device-auth` instead."
    );
}

pub async fn login_with_chatgpt(
    codex_home: PathBuf,
    forced_chatgpt_workspace_id: Option<String>,
    cli_auth_credentials_store_mode: AuthCredentialsStoreMode,
) -> std::io::Result<()> {
    let opts = ServerOptions::new(
        codex_home,
        CLIENT_ID.to_string(),
        forced_chatgpt_workspace_id,
        cli_auth_credentials_store_mode,
    );
    let server = run_login_server(opts)?;

    print_login_server_start(server.actual_port, &server.auth_url);

    server.block_until_done().await
}

pub async fn run_login_with_chatgpt(cli_config_overrides: CliConfigOverrides) -> ! {
    let config = load_config_or_exit(cli_config_overrides).await;
    let _login_log_guard = init_login_file_logging(&config);
    tracing::info!("starting browser login flow");

    if matches!(config.forced_login_method, Some(ForcedLoginMethod::Api)) {
        eprintln!("{CHATGPT_LOGIN_DISABLED_MESSAGE}");
        std::process::exit(1);
    }

    let forced_chatgpt_workspace_id = config.forced_chatgpt_workspace_id.clone();

    match login_with_chatgpt(
        config.codex_home,
        forced_chatgpt_workspace_id,
        config.cli_auth_credentials_store_mode,
    )
    .await
    {
        Ok(_) => {
            eprintln!("{LOGIN_SUCCESS_MESSAGE}");
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("Error logging in: {e}");
            std::process::exit(1);
        }
    }
}

fn fatherpaul_portal_base_url(config: &Config) -> String {
    config.chatgpt_base_url.trim_end_matches('/').to_string()
}

fn print_fatherpaul_device_prompt(
    verification_uri: &str,
    verification_uri_complete: &str,
    user_code: &str,
    expires_in: u64,
    open_browser: bool,
) {
    if open_browser {
        eprintln!(
            "Connexion Father Paul en cours.\n\nSi le navigateur ne s'ouvre pas, autorisez ce terminal ici:\n{verification_uri_complete}\n\nCode: {user_code}\nExpiration: {} minutes\n",
            expires_in / 60
        );
    } else {
        eprintln!(
            "Autorisez ce terminal sur Father Paul AI.\n\n1. Ouvrez: {verification_uri}\n2. Entrez le code: {user_code}\nExpiration: {} minutes\n",
            expires_in / 60
        );
    }
}

async fn start_fatherpaul_device_login(
    portal_base_url: &str,
) -> std::io::Result<FatherPaulDeviceStartResponse> {
    let client = build_reqwest_client();
    let response = client
        .post(format!("{portal_base_url}/api/cli/device/start"))
        .json(&serde_json::json!({ "client_name": FATHERPAUL_PORTAL_CLIENT_NAME }))
        .send()
        .await
        .map_err(std::io::Error::other)?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(std::io::Error::other(format!(
            "Father Paul CLI authorization start failed ({status}): {body}"
        )));
    }

    response.json().await.map_err(std::io::Error::other)
}

async fn poll_fatherpaul_device_login(
    polling_endpoint: &str,
    device_code: &str,
    interval_secs: u64,
) -> std::io::Result<String> {
    let client = build_reqwest_client();
    let mut poll_every = interval_secs.max(2);

    loop {
        let response = client
            .post(polling_endpoint)
            .json(&serde_json::json!({ "device_code": device_code }))
            .send()
            .await
            .map_err(std::io::Error::other)?;

        let status_code = response.status();
        let body: FatherPaulDevicePollResponse =
            response.json().await.map_err(std::io::Error::other)?;

        match body.status.as_str() {
            "authorization_pending" => {
                poll_every = body.interval.unwrap_or(poll_every).max(2);
            }
            "authorized" => {
                let Some(access_token) = body.access_token else {
                    return Err(std::io::Error::other(
                        "Father Paul portal authorized the CLI but did not return an access token.",
                    ));
                };
                return Ok(access_token);
            }
            "expired_token" => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    body.detail.unwrap_or_else(|| "Device code expired".to_string()),
                ));
            }
            "access_denied" => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    body.detail.unwrap_or_else(|| "Authorization denied".to_string()),
                ));
            }
            "already_used" => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    body.detail
                        .unwrap_or_else(|| "Authorization already consumed".to_string()),
                ));
            }
            other => {
                return Err(std::io::Error::other(format!(
                    "Unexpected Father Paul device login status `{other}` (HTTP {status_code})"
                )));
            }
        }

        tokio::time::sleep(Duration::from_secs(poll_every)).await;
    }
}

async fn fetch_fatherpaul_cli_session(
    portal_base_url: &str,
    access_token: &str,
) -> std::io::Result<FatherPaulCliSessionResponse> {
    let client = build_reqwest_client();
    let response = client
        .get(format!("{portal_base_url}/api/cli/session"))
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(std::io::Error::other)?;

    if response.status() == StatusCode::FORBIDDEN {
        let body = response.text().await.unwrap_or_default();
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!("Father Paul account is not eligible for CLI access yet: {body}"),
        ));
    }

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(std::io::Error::other(format!(
            "Father Paul CLI session retrieval failed ({status}): {body}"
        )));
    }

    response.json().await.map_err(std::io::Error::other)
}

async fn run_fatherpaul_browser_login_impl(
    cli_config_overrides: CliConfigOverrides,
    open_browser: bool,
) -> ! {
    let config = load_config_or_exit(cli_config_overrides).await;
    let _login_log_guard = init_login_file_logging(&config);
    tracing::info!("starting fatherpaul portal login flow");

    let portal_base_url = fatherpaul_portal_base_url(&config);
    let device = match start_fatherpaul_device_login(&portal_base_url).await {
        Ok(device) => device,
        Err(err) => {
            eprintln!("Error starting Father Paul login: {err}");
            std::process::exit(1);
        }
    };

    print_fatherpaul_device_prompt(
        &device.verification_uri,
        &device.verification_uri_complete,
        &device.user_code,
        device.expires_in,
        open_browser,
    );

    if open_browser {
        let _ = webbrowser::open(&device.verification_uri_complete);
    }

    let access_token = match poll_fatherpaul_device_login(
        &device.polling_endpoint,
        &device.device_code,
        device.interval,
    )
    .await
    {
        Ok(token) => token,
        Err(err) => {
            eprintln!("Error completing Father Paul login: {err}");
            std::process::exit(1);
        }
    };

    let session = match fetch_fatherpaul_cli_session(&portal_base_url, &access_token).await {
        Ok(session) => session,
        Err(err) => {
            eprintln!("Error retrieving Father Paul CLI session: {err}");
            std::process::exit(1);
        }
    };

    if session.api_key.trim().is_empty() {
        eprintln!("Error: Father Paul portal returned an empty CLI API key.");
        std::process::exit(1);
    }
    if session.status != "ACTIVE" {
        eprintln!(
            "Error: Father Paul portal returned an unexpected CLI session status `{}`.",
            session.status
        );
        std::process::exit(1);
    }

    match login_with_api_key(
        &config.codex_home,
        &session.api_key,
        config.cli_auth_credentials_store_mode,
    ) {
        Ok(_) => {
            let email = session.user.email;
            let full_name = session.user.full_name.unwrap_or_default();
            let plan = session
                .subscription
                .plan
                .unwrap_or_else(|| "FREE".to_string());
            eprintln!(
                "{LOGIN_SUCCESS_MESSAGE}\nCompte: {}{}\nPlan: {plan}\nAPI: {}",
                email,
                if full_name.is_empty() {
                    String::new()
                } else {
                    format!(" ({full_name})")
                },
                session.api_base_url
            );
            std::process::exit(0);
        }
        Err(err) => {
            eprintln!("Error saving Father Paul CLI credentials: {err}");
            std::process::exit(1);
        }
    }
}

pub async fn run_login_with_api_key(
    cli_config_overrides: CliConfigOverrides,
    api_key: String,
) -> ! {
    let config = load_config_or_exit(cli_config_overrides).await;
    let _login_log_guard = init_login_file_logging(&config);
    tracing::info!("starting api key login flow");

    if matches!(config.forced_login_method, Some(ForcedLoginMethod::Chatgpt)) {
        eprintln!("{API_KEY_LOGIN_DISABLED_MESSAGE}");
        std::process::exit(1);
    }

    match login_with_api_key(
        &config.codex_home,
        &api_key,
        config.cli_auth_credentials_store_mode,
    ) {
        Ok(_) => {
            eprintln!("{LOGIN_SUCCESS_MESSAGE}");
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("Error logging in: {e}");
            std::process::exit(1);
        }
    }
}

pub fn read_api_key_from_stdin() -> String {
    let mut stdin = std::io::stdin();

    if stdin.is_terminal() {
        eprintln!(
            "--with-api-key expects the API key on stdin. Try piping it, e.g. `printenv FATHERPAUL_API_KEY | fatherpaul-code login --with-api-key`."
        );
        std::process::exit(1);
    }

    eprintln!("Reading API key from stdin...");

    let mut buffer = String::new();
    if let Err(err) = stdin.read_to_string(&mut buffer) {
        eprintln!("Failed to read API key from stdin: {err}");
        std::process::exit(1);
    }

    let api_key = buffer.trim().to_string();
    if api_key.is_empty() {
        eprintln!("No API key provided via stdin.");
        std::process::exit(1);
    }

    api_key
}

/// Login using the OAuth device code flow.
pub async fn run_login_with_device_code(
    cli_config_overrides: CliConfigOverrides,
    issuer_base_url: Option<String>,
    client_id: Option<String>,
) -> ! {
    if issuer_base_url.is_none() && client_id.is_none() {
        run_fatherpaul_browser_login_impl(cli_config_overrides, false).await;
    }

    let config = load_config_or_exit(cli_config_overrides).await;
    let _login_log_guard = init_login_file_logging(&config);
    tracing::info!("starting device code login flow");
    if matches!(config.forced_login_method, Some(ForcedLoginMethod::Api)) {
        eprintln!("{CHATGPT_LOGIN_DISABLED_MESSAGE}");
        std::process::exit(1);
    }
    let forced_chatgpt_workspace_id = config.forced_chatgpt_workspace_id.clone();
    let mut opts = ServerOptions::new(
        config.codex_home,
        client_id.unwrap_or(CLIENT_ID.to_string()),
        forced_chatgpt_workspace_id,
        config.cli_auth_credentials_store_mode,
    );
    if let Some(iss) = issuer_base_url {
        opts.issuer = iss;
    }
    match run_device_code_login(opts).await {
        Ok(()) => {
            eprintln!("{LOGIN_SUCCESS_MESSAGE}");
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("Error logging in with device code: {e}");
            std::process::exit(1);
        }
    }
}

/// Prefers device-code login (with `open_browser = false`) when headless environment is detected, but keeps
/// `codex login` working in environments where device-code may be disabled/feature-gated.
/// If `run_device_code_login` returns `ErrorKind::NotFound` ("device-code unsupported"), this
/// falls back to starting the local browser login server.
pub async fn run_login_with_device_code_fallback_to_browser(
    cli_config_overrides: CliConfigOverrides,
    issuer_base_url: Option<String>,
    client_id: Option<String>,
) -> ! {
    if issuer_base_url.is_none() && client_id.is_none() {
        run_fatherpaul_browser_login_impl(cli_config_overrides, true).await;
    }

    let config = load_config_or_exit(cli_config_overrides).await;
    let _login_log_guard = init_login_file_logging(&config);
    tracing::info!("starting login flow with device code fallback");
    if matches!(config.forced_login_method, Some(ForcedLoginMethod::Api)) {
        eprintln!("{CHATGPT_LOGIN_DISABLED_MESSAGE}");
        std::process::exit(1);
    }

    let forced_chatgpt_workspace_id = config.forced_chatgpt_workspace_id.clone();
    let mut opts = ServerOptions::new(
        config.codex_home,
        client_id.unwrap_or(CLIENT_ID.to_string()),
        forced_chatgpt_workspace_id,
        config.cli_auth_credentials_store_mode,
    );
    if let Some(iss) = issuer_base_url {
        opts.issuer = iss;
    }
    opts.open_browser = false;

    match run_device_code_login(opts.clone()).await {
        Ok(()) => {
            eprintln!("{LOGIN_SUCCESS_MESSAGE}");
            std::process::exit(0);
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                eprintln!("Device code login is not enabled; falling back to browser login.");
                match run_login_server(opts) {
                    Ok(server) => {
                        print_login_server_start(server.actual_port, &server.auth_url);
                        match server.block_until_done().await {
                            Ok(()) => {
                                eprintln!("{LOGIN_SUCCESS_MESSAGE}");
                                std::process::exit(0);
                            }
                            Err(e) => {
                                eprintln!("Error logging in: {e}");
                                std::process::exit(1);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Error logging in: {e}");
                        std::process::exit(1);
                    }
                }
            } else {
                eprintln!("Error logging in with device code: {e}");
                std::process::exit(1);
            }
        }
    }
}

pub async fn run_login_status(cli_config_overrides: CliConfigOverrides) -> ! {
    let config = load_config_or_exit(cli_config_overrides).await;

    match CodexAuth::from_auth_storage(&config.codex_home, config.cli_auth_credentials_store_mode) {
        Ok(Some(auth)) => match auth.auth_mode() {
            AuthMode::ApiKey => match auth.get_token() {
                Ok(api_key) => {
                    eprintln!(
                        "Logged in to Father Paul AI using an API key - {}",
                        safe_format_key(&api_key)
                    );
                    std::process::exit(0);
                }
                Err(e) => {
                    eprintln!("Unexpected error retrieving API key: {e}");
                    std::process::exit(1);
                }
            },
            AuthMode::Chatgpt | AuthMode::ChatgptAuthTokens => {
                eprintln!("Logged in using a stored browser session");
                std::process::exit(0);
            }
        },
        Ok(None) => {
            eprintln!("Not logged in");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("Error checking login status: {e}");
            std::process::exit(1);
        }
    }
}

pub async fn run_logout(cli_config_overrides: CliConfigOverrides) -> ! {
    let config = load_config_or_exit(cli_config_overrides).await;

    match logout(&config.codex_home, config.cli_auth_credentials_store_mode) {
        Ok(true) => {
            eprintln!("Successfully logged out");
            std::process::exit(0);
        }
        Ok(false) => {
            eprintln!("Not logged in");
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("Error logging out: {e}");
            std::process::exit(1);
        }
    }
}

async fn load_config_or_exit(cli_config_overrides: CliConfigOverrides) -> Config {
    let cli_overrides = match cli_config_overrides.parse_overrides() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error parsing -c overrides: {e}");
            std::process::exit(1);
        }
    };

    match Config::load_with_cli_overrides(cli_overrides).await {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Error loading configuration: {e}");
            std::process::exit(1);
        }
    }
}

fn safe_format_key(key: &str) -> String {
    if key.len() <= 13 {
        return "***".to_string();
    }
    let prefix = &key[..8];
    let suffix = &key[key.len() - 5..];
    format!("{prefix}***{suffix}")
}

#[cfg(test)]
mod tests {
    use super::safe_format_key;

    #[test]
    fn formats_long_key() {
        let key = "sk-proj-1234567890ABCDE";
        assert_eq!(safe_format_key(key), "sk-proj-***ABCDE");
    }

    #[test]
    fn short_key_returns_stars() {
        let key = "sk-proj-12345";
        assert_eq!(safe_format_key(key), "***");
    }
}
