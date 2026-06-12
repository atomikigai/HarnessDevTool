//! `ssh.*` MCP tools. Thin wrappers around `module_ssh::Manager`.

use module_ssh::Manager;
use serde_json::Value;

fn str_arg<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("missing or non-string arg: {key}"))
}

pub fn hosts(manager: &Manager) -> Result<Value, String> {
    serde_json::to_value(manager.list_hosts().map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())
}

pub fn test_host(manager: &Manager, args: &Value) -> Result<Value, String> {
    let id = str_arg(args, "host")?;
    block_on_json(manager.test_host(id))
}

pub fn exec(manager: &Manager, args: &Value) -> Result<Value, String> {
    let host = str_arg(args, "host")?;
    let cmd = str_arg(args, "cmd")?;
    block_on_json(manager.exec(host, cmd))
}

pub fn context_refresh(manager: &Manager, args: &Value) -> Result<Value, String> {
    let host = str_arg(args, "host_id")?;
    block_on_markdown(manager.context_refresh(host))
}

pub fn context(manager: &Manager, args: &Value) -> Result<Value, String> {
    let host = str_arg(args, "host_id")?;
    let max_age_hours = args.get("max_age_hours").and_then(|v| v.as_u64());
    block_on_markdown(manager.context(host, max_age_hours))
}

pub fn sftp_list(manager: &Manager, args: &Value) -> Result<Value, String> {
    let host = str_arg(args, "host")?;
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
    block_on_json(manager.sftp_list(host, path))
}

pub fn sftp_get(manager: &Manager, args: &Value) -> Result<Value, String> {
    let host = str_arg(args, "host")?;
    let remote_path = str_arg(args, "remote_path")?;
    let local_path = str_arg(args, "local_path")?;
    block_on_json(manager.sftp_get(host, remote_path, std::path::Path::new(local_path)))
}

pub fn sftp_put(manager: &Manager, args: &Value) -> Result<Value, String> {
    let host = str_arg(args, "host")?;
    let local_path = str_arg(args, "local_path")?;
    let remote_path = str_arg(args, "remote_path")?;
    block_on_json(manager.sftp_put(host, std::path::Path::new(local_path), remote_path))
}

pub fn sftp_mkdir(manager: &Manager, args: &Value) -> Result<Value, String> {
    let host = str_arg(args, "host")?;
    let path = str_arg(args, "path")?;
    block_on_json(manager.sftp_mkdir(host, path))
}

pub fn sftp_rmdir(manager: &Manager, args: &Value) -> Result<Value, String> {
    let host = str_arg(args, "host")?;
    let path = str_arg(args, "path")?;
    block_on_json(manager.sftp_rmdir(host, path))
}

pub fn sftp_unlink(manager: &Manager, args: &Value) -> Result<Value, String> {
    let host = str_arg(args, "host")?;
    let path = str_arg(args, "path")?;
    block_on_json(manager.sftp_unlink(host, path))
}

pub fn sftp_rename(manager: &Manager, args: &Value) -> Result<Value, String> {
    let host = str_arg(args, "host")?;
    let from_path = str_arg(args, "from_path")?;
    let to_path = str_arg(args, "to_path")?;
    block_on_json(manager.sftp_rename(host, from_path, to_path))
}

fn block_on_json<T, F>(future: F) -> Result<Value, String>
where
    T: serde::Serialize,
    F: std::future::Future<Output = Result<T, module_ssh::SshError>>,
{
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let result = rt.block_on(future).map_err(|e| e.to_string())?;
    serde_json::to_value(result).map_err(|e| e.to_string())
}

fn block_on_markdown<F>(future: F) -> Result<Value, String>
where
    F: std::future::Future<Output = Result<String, module_ssh::SshError>>,
{
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    Ok(Value::String(
        rt.block_on(future).map_err(|e| e.to_string())?,
    ))
}
