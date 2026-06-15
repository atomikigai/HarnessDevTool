//! n8n MCP tools.
//!
//! The tools intentionally store workflow JSON and instance connection
//! metadata, but not API keys. Agents receive an environment variable name and
//! the MCP process reads the value locally when it needs to call n8n.

use std::fs;
use std::io::Read;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const DEFAULT_API_KEY_ENV: &str = "N8N_API_KEY";
const DEFAULT_IMAGE: &str = "docker.n8n.io/n8nio/n8n:latest";
const HTTP_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct N8nConfig {
    base_url: Option<String>,
    api_key_env: Option<String>,
    container_name: Option<String>,
    image: Option<String>,
    port: Option<u16>,
}

pub fn configure(harness_home: &Path, profile: &str, args: &Value) -> Result<Value, String> {
    let mut config = load_config(harness_home, profile)?;
    if let Some(base_url) = args.get("base_url").and_then(Value::as_str) {
        config.base_url = Some(trim_base_url(base_url)?);
    }
    if let Some(api_key_env) = args.get("api_key_env").and_then(Value::as_str) {
        config.api_key_env = Some(validate_env_name(api_key_env)?.to_string());
    }
    if let Some(container_name) = args.get("container_name").and_then(Value::as_str) {
        config.container_name = Some(validate_container_name(container_name)?.to_string());
    }
    save_config(harness_home, profile, &config)?;
    Ok(public_config(&config))
}

pub fn status(harness_home: &Path, profile: &str, args: &Value) -> Result<Value, String> {
    let config = effective_config(harness_home, profile, args)?;
    let api_status = match api_request(&config, "GET", "/workflows?limit=1", None) {
        Ok(value) => json!({ "ok": true, "sample": value }),
        Err(e) => json!({ "ok": false, "error": e }),
    };
    let docker_status = container_status(&config.container_name());
    Ok(json!({
        "config": public_config(&config.into_stored()),
        "api": api_status,
        "docker": docker_status,
    }))
}

pub fn local_start(harness_home: &Path, profile: &str, args: &Value) -> Result<Value, String> {
    require_approved(args, "n8n_local_start")?;
    let docker =
        which::which("docker").map_err(|_| "n8n_local_start: docker not found".to_string())?;
    let mut config = load_config(harness_home, profile)?;
    let image = args
        .get("image")
        .and_then(Value::as_str)
        .unwrap_or_else(|| config.image.as_deref().unwrap_or(DEFAULT_IMAGE));
    let image = validate_image_ref(image)?;
    let port = args
        .get("port")
        .and_then(Value::as_u64)
        .map(|port| validate_port(port, "port"))
        .transpose()?
        .unwrap_or_else(free_local_port);
    let container_name = args
        .get("container_name")
        .and_then(Value::as_str)
        .map(validate_container_name)
        .transpose()?
        .map(str::to_string)
        .or(config.container_name.clone())
        .unwrap_or_else(|| default_container_name(profile));
    let data_dir = module_root(harness_home, profile).join("local-instance");
    fs::create_dir_all(&data_dir).map_err(|e| format!("n8n_local_start: create data dir: {e}"))?;
    let encryption_key = ensure_encryption_key(harness_home, profile)?;

    if container_is_running(&container_name) {
        config.base_url = Some(format!("http://127.0.0.1:{port}"));
        config.api_key_env = config.api_key_env.or(Some(DEFAULT_API_KEY_ENV.to_string()));
        config.container_name = Some(container_name.clone());
        config.image = Some(image.to_string());
        config.port = Some(port);
        save_config(harness_home, profile, &config)?;
        return Ok(json!({
            "status": "already_running",
            "base_url": config.base_url,
            "container_name": container_name,
            "api_key_env": config.api_key_env,
            "note": "Create an API key in n8n Settings > n8n API and export it in the configured env var before API tools can import or activate workflows."
        }));
    }

    let _ = Command::new(&docker)
        .args(["rm", "-f", &container_name])
        .output();
    let output = Command::new(&docker)
        .args([
            "run",
            "-d",
            "--name",
            &container_name,
            "-p",
            &format!("127.0.0.1:{port}:5678"),
            "-v",
            &format!("{}:/home/node/.n8n", data_dir.display()),
            "-e",
            "N8N_HOST=localhost",
            "-e",
            "N8N_PORT=5678",
            "-e",
            "N8N_PROTOCOL=http",
            "-e",
            "N8N_SECURE_COOKIE=false",
            "-e",
            "N8N_PUBLIC_API_DISABLED=false",
            "-e",
            "N8N_DIAGNOSTICS_ENABLED=false",
            "-e",
            "N8N_VERSION_NOTIFICATIONS_ENABLED=false",
            "-e",
            &format!("N8N_ENCRYPTION_KEY={encryption_key}"),
            image,
        ])
        .output()
        .map_err(|e| format!("n8n_local_start: run docker: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "n8n_local_start: docker run failed: {}",
            stderr_or_stdout(&output)
        ));
    }

    config.base_url = Some(format!("http://127.0.0.1:{port}"));
    config.api_key_env = config.api_key_env.or(Some(DEFAULT_API_KEY_ENV.to_string()));
    config.container_name = Some(container_name.clone());
    config.image = Some(image.to_string());
    config.port = Some(port);
    save_config(harness_home, profile, &config)?;

    Ok(json!({
        "status": "started",
        "base_url": config.base_url,
        "container_name": container_name,
        "port": port,
        "data_dir": data_dir,
        "api_key_env": config.api_key_env,
        "note": "Open n8n, finish owner setup if needed, create an API key in Settings > n8n API, then export it in the configured env var for this harness process."
    }))
}

pub fn local_stop(harness_home: &Path, profile: &str, args: &Value) -> Result<Value, String> {
    require_approved(args, "n8n_local_stop")?;
    let config = load_config(harness_home, profile)?;
    let container_name = args
        .get("container_name")
        .and_then(Value::as_str)
        .map(validate_container_name)
        .transpose()?
        .map(str::to_string)
        .or(config.container_name)
        .unwrap_or_else(|| default_container_name(profile));
    let docker =
        which::which("docker").map_err(|_| "n8n_local_stop: docker not found".to_string())?;
    let output = Command::new(docker)
        .args(["rm", "-f", &container_name])
        .output()
        .map_err(|e| format!("n8n_local_stop: run docker: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "n8n_local_stop: docker rm failed: {}",
            stderr_or_stdout(&output)
        ));
    }
    Ok(json!({ "status": "stopped", "container_name": container_name }))
}

pub fn save_workflow(harness_home: &Path, profile: &str, args: &Value) -> Result<Value, String> {
    let name = str_arg(args, "name")?;
    let safe_name = validate_file_stem(name)?;
    let workflow = workflow_arg(args)?;
    let validation = validate_workflow_value(&workflow);
    let dir = workflows_dir(harness_home, profile);
    fs::create_dir_all(&dir).map_err(|e| format!("n8n_save_workflow: create dir: {e}"))?;
    let path = dir.join(format!("{safe_name}.json"));
    let bytes =
        serde_json::to_vec_pretty(&workflow).map_err(|e| format!("serialize workflow: {e}"))?;
    fs::write(&path, bytes).map_err(|e| format!("n8n_save_workflow: write workflow: {e}"))?;
    Ok(json!({
        "saved": true,
        "name": safe_name,
        "path": path,
        "validation": validation,
    }))
}

pub fn list_saved_workflows(harness_home: &Path, profile: &str) -> Result<Value, String> {
    let dir = workflows_dir(harness_home, profile);
    if !dir.exists() {
        return Ok(json!({ "workflows": [] }));
    }
    let mut workflows = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|e| format!("n8n_list_saved_workflows: {e}"))? {
        let entry = entry.map_err(|e| format!("n8n_list_saved_workflows: {e}"))?;
        if !entry.file_type().map_err(|e| e.to_string())?.is_file() {
            continue;
        }
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let meta = entry.metadata().map_err(|e| e.to_string())?;
        workflows.push(json!({ "name": name, "path": path, "size": meta.len() }));
    }
    workflows.sort_by(|a, b| {
        a["name"]
            .as_str()
            .unwrap_or("")
            .cmp(b["name"].as_str().unwrap_or(""))
    });
    Ok(json!({ "workflows": workflows }))
}

pub fn read_workflow(harness_home: &Path, profile: &str, args: &Value) -> Result<Value, String> {
    let workflow = load_named_workflow(harness_home, profile, str_arg(args, "name")?)?;
    Ok(json!({
        "workflow": workflow,
        "validation": validate_workflow_value(&workflow),
    }))
}

pub fn validate_workflow(args: &Value) -> Result<Value, String> {
    Ok(validate_workflow_value(&workflow_arg(args)?))
}

pub fn import_workflow(harness_home: &Path, profile: &str, args: &Value) -> Result<Value, String> {
    require_approved(args, "n8n_import_workflow")?;
    let config = effective_config(harness_home, profile, args)?;
    let workflow = workflow_or_saved(harness_home, profile, args)?;
    let validation = validate_workflow_value(&workflow);
    if !validation["valid"].as_bool().unwrap_or(false) {
        return Err(format!(
            "n8n_import_workflow: invalid workflow: {}",
            validation
        ));
    }
    let response = api_request(&config, "POST", "/workflows", Some(workflow))?;
    Ok(json!({ "imported": true, "response": response, "validation": validation }))
}

pub fn list_remote_workflows(
    harness_home: &Path,
    profile: &str,
    args: &Value,
) -> Result<Value, String> {
    let config = effective_config(harness_home, profile, args)?;
    let active = args.get("active").and_then(Value::as_bool);
    let endpoint = match active {
        Some(true) => "/workflows?active=true",
        Some(false) => "/workflows?active=false",
        None => "/workflows",
    };
    api_request(&config, "GET", endpoint, None)
}

pub fn activate_workflow(
    harness_home: &Path,
    profile: &str,
    args: &Value,
) -> Result<Value, String> {
    require_approved(args, "n8n_activate_workflow")?;
    let config = effective_config(harness_home, profile, args)?;
    let id = str_arg(args, "id")?;
    api_request(
        &config,
        "POST",
        &format!("/workflows/{}/activate", url_segment(id)?),
        None,
    )
}

pub fn deactivate_workflow(
    harness_home: &Path,
    profile: &str,
    args: &Value,
) -> Result<Value, String> {
    require_approved(args, "n8n_deactivate_workflow")?;
    let config = effective_config(harness_home, profile, args)?;
    let id = str_arg(args, "id")?;
    api_request(
        &config,
        "POST",
        &format!("/workflows/{}/deactivate", url_segment(id)?),
        None,
    )
}

pub fn webhook_request(harness_home: &Path, profile: &str, args: &Value) -> Result<Value, String> {
    require_approved(args, "n8n_webhook_request")?;
    let config = effective_config(harness_home, profile, args)?;
    let method = args
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("POST")
        .to_ascii_uppercase();
    if !matches!(method.as_str(), "GET" | "POST") {
        return Err("n8n_webhook_request: method must be GET or POST".into());
    }
    let path = str_arg(args, "path")?;
    if !path.starts_with('/') || path.contains("..") || path.contains('\\') {
        return Err(
            "n8n_webhook_request: path must be an absolute webhook path without traversal".into(),
        );
    }
    let url = format!("{}{}", config.base_url.trim_end_matches('/'), path);
    let body = args.get("body").cloned();
    let mut request = match method.as_str() {
        "GET" => ureq::get(&url).timeout(HTTP_TIMEOUT),
        _ => ureq::post(&url).timeout(HTTP_TIMEOUT),
    };
    if let Some(api_key) = config.api_key_value.as_deref() {
        request = request.set("X-N8N-API-KEY", api_key);
    }
    let response = match body {
        Some(body) if method == "POST" => request.send_json(body),
        _ if method == "POST" => request.send_string(""),
        _ => request.call(),
    };
    parse_http_response("n8n_webhook_request", response)
}

#[derive(Debug, Clone)]
struct EffectiveConfig {
    base_url: String,
    api_key_env: Option<String>,
    api_key_value: Option<String>,
    container_name: String,
    image: String,
    port: Option<u16>,
}

impl EffectiveConfig {
    fn container_name(&self) -> String {
        self.container_name.clone()
    }

    fn into_stored(self) -> N8nConfig {
        N8nConfig {
            base_url: Some(self.base_url),
            api_key_env: self.api_key_env,
            container_name: Some(self.container_name),
            image: Some(self.image),
            port: self.port,
        }
    }
}

fn effective_config(
    harness_home: &Path,
    profile: &str,
    args: &Value,
) -> Result<EffectiveConfig, String> {
    let stored = load_config(harness_home, profile)?;
    let base_url = args
        .get("base_url")
        .and_then(Value::as_str)
        .map(trim_base_url)
        .transpose()?
        .or(stored.base_url)
        .ok_or_else(|| {
            "n8n base_url is not configured; call n8n_configure or n8n_local_start".to_string()
        })?;
    let api_key_env = args
        .get("api_key_env")
        .and_then(Value::as_str)
        .map(validate_env_name)
        .transpose()?
        .map(str::to_string)
        .or(stored.api_key_env)
        .or(Some(DEFAULT_API_KEY_ENV.to_string()));
    let api_key_value = match api_key_env.as_deref() {
        Some(env) => std::env::var(env).ok(),
        None => None,
    };
    Ok(EffectiveConfig {
        base_url,
        api_key_env,
        api_key_value,
        container_name: stored
            .container_name
            .unwrap_or_else(|| default_container_name(profile)),
        image: stored.image.unwrap_or_else(|| DEFAULT_IMAGE.to_string()),
        port: stored.port,
    })
}

fn api_request(
    config: &EffectiveConfig,
    method: &str,
    endpoint: &str,
    body: Option<Value>,
) -> Result<Value, String> {
    let api_key = config.api_key_value.as_deref().ok_or_else(|| {
        format!(
            "n8n API key not available; export {} for this harness process",
            config.api_key_env.as_deref().unwrap_or(DEFAULT_API_KEY_ENV)
        )
    })?;
    let url = format!(
        "{}{}",
        api_base_url(&config.base_url),
        endpoint
            .strip_prefix('/')
            .map(|p| format!("/{p}"))
            .unwrap_or_else(|| format!("/{endpoint}"))
    );
    let request = match method {
        "GET" => ureq::get(&url).timeout(HTTP_TIMEOUT),
        "POST" => ureq::post(&url).timeout(HTTP_TIMEOUT),
        _ => return Err(format!("unsupported n8n API method: {method}")),
    }
    .set("accept", "application/json")
    .set("X-N8N-API-KEY", api_key);

    let response = match body {
        Some(body) => request.send_json(body),
        None if method == "POST" => request.send_string(""),
        None => request.call(),
    };
    parse_http_response("n8n_api", response)
}

fn parse_http_response(
    context: &str,
    response: Result<ureq::Response, ureq::Error>,
) -> Result<Value, String> {
    match response {
        Ok(resp) => {
            let status = resp.status();
            let text = resp
                .into_string()
                .map_err(|e| format!("{context}: read response: {e}"))?;
            let parsed =
                serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({ "text": text }));
            Ok(json!({ "status": status, "body": parsed }))
        }
        Err(ureq::Error::Status(code, resp)) => {
            let text = resp.into_string().unwrap_or_default();
            Err(format!("{context}: HTTP {code}: {text}"))
        }
        Err(e) => Err(format!("{context}: {e}")),
    }
}

fn validate_workflow_value(workflow: &Value) -> Value {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    if !workflow.is_object() {
        errors.push("workflow must be a JSON object".to_string());
    }
    if workflow
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .is_empty()
    {
        errors.push("workflow.name is required".to_string());
    }
    match workflow.get("nodes").and_then(Value::as_array) {
        Some(nodes) if !nodes.is_empty() => {
            for (idx, node) in nodes.iter().enumerate() {
                if node
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .is_empty()
                {
                    errors.push(format!("nodes[{idx}].name is required"));
                }
                if node
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .is_empty()
                {
                    errors.push(format!("nodes[{idx}].type is required"));
                }
                if node.get("position").and_then(Value::as_array).is_none() {
                    warnings.push(format!(
                        "nodes[{idx}].position is missing; n8n UI layout may be poor"
                    ));
                }
            }
        }
        _ => errors.push("workflow.nodes must be a non-empty array".to_string()),
    }
    if workflow.get("connections").is_none() {
        errors.push("workflow.connections is required".to_string());
    }
    collect_secret_warnings("$", workflow, &mut warnings);
    json!({
        "valid": errors.is_empty(),
        "errors": errors,
        "warnings": warnings,
    })
}

fn collect_secret_warnings(path: &str, value: &Value, warnings: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let key_lc = key.to_ascii_lowercase();
                if matches!(
                    key_lc.as_str(),
                    "password"
                        | "passwd"
                        | "token"
                        | "access_token"
                        | "refresh_token"
                        | "apikey"
                        | "api_key"
                        | "authorization"
                        | "clientsecret"
                        | "client_secret"
                ) && child
                    .as_str()
                    .map(|s| !s.trim().is_empty())
                    .unwrap_or(!child.is_null())
                {
                    warnings.push(format!("{path}.{key} may contain a raw secret; prefer n8n credentials or environment variables"));
                }
                collect_secret_warnings(&format!("{path}.{key}",), child, warnings);
            }
        }
        Value::Array(items) => {
            for (idx, child) in items.iter().enumerate() {
                collect_secret_warnings(&format!("{path}[{idx}]"), child, warnings);
            }
        }
        _ => {}
    }
}

fn workflow_or_saved(harness_home: &Path, profile: &str, args: &Value) -> Result<Value, String> {
    if args.get("workflow").is_some() {
        workflow_arg(args)
    } else {
        load_named_workflow(harness_home, profile, str_arg(args, "name")?)
    }
}

fn workflow_arg(args: &Value) -> Result<Value, String> {
    args.get("workflow")
        .cloned()
        .ok_or_else(|| "missing arg `workflow`".to_string())
}

fn load_named_workflow(harness_home: &Path, profile: &str, name: &str) -> Result<Value, String> {
    let safe_name = validate_file_stem(name)?;
    let path = workflows_dir(harness_home, profile).join(format!("{safe_name}.json"));
    let text =
        fs::read_to_string(&path).map_err(|e| format!("read workflow `{safe_name}`: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("parse workflow `{safe_name}`: {e}"))
}

fn load_config(harness_home: &Path, profile: &str) -> Result<N8nConfig, String> {
    let path = config_path(harness_home, profile);
    match fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!("parse n8n config: {e}")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(N8nConfig::default()),
        Err(e) => Err(format!("read n8n config: {e}")),
    }
}

fn save_config(harness_home: &Path, profile: &str, config: &N8nConfig) -> Result<(), String> {
    let path = config_path(harness_home, profile);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create n8n config dir: {e}"))?;
    }
    let bytes =
        serde_json::to_vec_pretty(config).map_err(|e| format!("serialize n8n config: {e}"))?;
    fs::write(path, bytes).map_err(|e| format!("write n8n config: {e}"))
}

fn public_config(config: &N8nConfig) -> Value {
    json!({
        "base_url": config.base_url,
        "api_key_env": config.api_key_env,
        "container_name": config.container_name,
        "image": config.image,
        "port": config.port,
        "api_key_loaded": config.api_key_env.as_deref().and_then(|env| std::env::var(env).ok()).is_some(),
    })
}

fn config_path(harness_home: &Path, profile: &str) -> PathBuf {
    module_root(harness_home, profile).join("config.json")
}

fn workflows_dir(harness_home: &Path, profile: &str) -> PathBuf {
    module_root(harness_home, profile).join("workflows")
}

fn module_root(harness_home: &Path, profile: &str) -> PathBuf {
    harness_home
        .join("profiles")
        .join(profile)
        .join("modules/n8n")
}

fn ensure_encryption_key(harness_home: &Path, profile: &str) -> Result<String, String> {
    let path = module_root(harness_home, profile).join("encryption.key");
    if let Ok(key) = fs::read_to_string(&path) {
        let key = key.trim().to_string();
        if key.len() >= 32 {
            return Ok(key);
        }
    }
    let key = random_hex_key();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create n8n key dir: {e}"))?;
    }
    fs::write(&path, &key).map_err(|e| format!("write n8n encryption key: {e}"))?;
    Ok(key)
}

fn random_hex_key() -> String {
    let mut bytes = [0u8; 32];
    if fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .is_err()
    {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let fallback = format!("{now}-{}", std::process::id());
        bytes.copy_from_slice(&Sha256::digest(fallback.as_bytes()));
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn api_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/api/v1") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/api/v1")
    }
}

fn trim_base_url(base_url: &str) -> Result<String, String> {
    let trimmed = base_url.trim().trim_end_matches('/');
    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        return Err("base_url must start with http:// or https://".to_string());
    }
    Ok(trimmed.to_string())
}

fn validate_env_name(name: &str) -> Result<&str, String> {
    if name.is_empty()
        || !name
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
        || name.chars().next().is_some_and(|ch| ch.is_ascii_digit())
    {
        return Err("api_key_env must be an uppercase environment variable name".to_string());
    }
    Ok(name)
}

fn validate_file_stem(name: &str) -> Result<&str, String> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || !name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err("workflow name must be a safe file stem".to_string());
    }
    Ok(name)
}

fn validate_container_name(name: &str) -> Result<&str, String> {
    if name.is_empty()
        || !name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(
            "container_name must contain only ASCII letters, digits, '.', '_' or '-'".to_string(),
        );
    }
    Ok(name)
}

fn validate_image_ref(image: &str) -> Result<&str, String> {
    if image.is_empty()
        || image.starts_with('-')
        || image.contains(char::is_whitespace)
        || image.contains(';')
        || image.contains('&')
        || image.contains('|')
    {
        return Err("invalid docker image reference".to_string());
    }
    Ok(image)
}

fn validate_port(port: u64, key: &str) -> Result<u16, String> {
    if !(1024..=65535).contains(&port) {
        return Err(format!("{key} must be between 1024 and 65535"));
    }
    Ok(port as u16)
}

fn free_local_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .map(|addr| addr.port())
        .unwrap_or(5678)
}

fn default_container_name(profile: &str) -> String {
    let suffix: String = profile
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
        .take(32)
        .collect();
    format!(
        "harness-n8n-{}",
        if suffix.is_empty() {
            "default"
        } else {
            &suffix
        }
    )
}

fn url_segment(segment: &str) -> Result<&str, String> {
    if segment.is_empty()
        || segment.contains('/')
        || segment.contains('\\')
        || segment.contains("..")
    {
        return Err("id must be a single URL path segment".to_string());
    }
    Ok(segment)
}

fn str_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing or non-string arg: {key}"))
}

fn require_approved(args: &Value, action: &str) -> Result<(), String> {
    if args
        .get("approved")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        Ok(())
    } else {
        Err(format!(
            "{action} requires explicit approval; pass approved=true after user confirmation"
        ))
    }
}

fn container_status(container_name: &str) -> Value {
    match which::which("docker") {
        Ok(docker) => {
            let output = Command::new(docker)
                .args(["inspect", container_name])
                .output();
            match output {
                Ok(output) if output.status.success() => {
                    let parsed = serde_json::from_slice::<Value>(&output.stdout)
                        .unwrap_or_else(|_| json!([]));
                    json!({ "available": true, "container_name": container_name, "inspect": parsed })
                }
                Ok(output) => json!({
                    "available": true,
                    "container_name": container_name,
                    "error": stderr_or_stdout(&output),
                }),
                Err(e) => {
                    json!({ "available": true, "container_name": container_name, "error": e.to_string() })
                }
            }
        }
        Err(_) => json!({ "available": false, "error": "docker not found" }),
    }
}

fn container_is_running(container_name: &str) -> bool {
    let Ok(docker) = which::which("docker") else {
        return false;
    };
    Command::new(docker)
        .args(["inspect", "-f", "{{.State.Running}}", container_name])
        .output()
        .map(|output| {
            output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == "true"
        })
        .unwrap_or(false)
}

fn stderr_or_stdout(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    } else {
        stderr
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_workflow() -> Value {
        json!({
            "name": "Harness smoke",
            "nodes": [
                {
                    "parameters": {},
                    "id": "manual",
                    "name": "Manual Trigger",
                    "type": "n8n-nodes-base.manualTrigger",
                    "typeVersion": 1,
                    "position": [0, 0]
                }
            ],
            "connections": {}
        })
    }

    #[test]
    fn validates_minimal_workflow() {
        let result = validate_workflow_value(&sample_workflow());
        assert_eq!(result["valid"], true);
        assert!(result["errors"].as_array().unwrap().is_empty());
    }

    #[test]
    fn validation_warns_about_raw_secrets() {
        let mut workflow = sample_workflow();
        workflow["nodes"][0]["parameters"] = json!({ "apiKey": "secret" });
        let result = validate_workflow_value(&workflow);
        assert_eq!(result["valid"], true);
        assert!(result["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning.as_str().unwrap().contains("raw secret")));
    }

    #[test]
    fn save_and_read_workflow_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let args = json!({ "name": "smoke", "workflow": sample_workflow() });
        save_workflow(dir.path(), "default", &args).unwrap();
        let read = read_workflow(dir.path(), "default", &json!({ "name": "smoke" })).unwrap();
        assert_eq!(read["workflow"]["name"], "Harness smoke");
    }

    #[test]
    fn validates_safe_names() {
        assert!(validate_file_stem("ok-name_1.2").is_ok());
        assert!(validate_file_stem("../bad").is_err());
        assert!(validate_env_name("N8N_API_KEY").is_ok());
        assert!(validate_env_name("n8n_api_key").is_err());
        assert!(url_segment("abc123").is_ok());
        assert!(url_segment("../abc").is_err());
    }
}
