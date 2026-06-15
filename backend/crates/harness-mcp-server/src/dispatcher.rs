//! Maps incoming JSON-RPC requests to handlers.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use harness_session::SessionMeta;
use serde_json::{json, Value};
use tracing::warn;

use crate::gateway::Gateway;
use crate::protocol::{
    error_response, error_response_with, result_response, Request, RpcError, PROTOCOL_VERSION,
    SERVER_NAME, SERVER_VERSION,
};
use crate::tools::{
    self, attachments as attachment_tools, capabilities as capability_tools, db as db_tools,
    docs as docs_tools, evidence as evidence_tools, knowledge as knowledge_tools, n8n as n8n_tools,
    planning, repo, session as session_tools, skills, spec, ssh as ssh_tools, tasks,
    toolsets::ToolRegistry, wrap_error, wrap_text,
};
use harness_core::TaskStore;
use harness_policy::{capability_default, is_sensitive_tool, Decision, PolicyEngine};
use module_db::Manager as DbManager;
use module_ssh::Manager as SshManager;

const DEFAULT_TRUNCATION_LINES: usize = 2_000;
const DEFAULT_TRUNCATION_BYTES: usize = 50 * 1024;
const POLICY_CHECK_TIMEOUT: Duration = Duration::from_secs(8);

pub struct Dispatcher {
    store: TaskStore,
    db: DbManager,
    ssh: SshManager,
    harness_home: PathBuf,
    profile: String,
    thread_id: String,
    agent_id: String,
    role: Option<String>,
    task_id: Option<String>,
    scopes: Vec<String>,
    policy: Option<PolicyEngine>,
    policy_load_error: Option<String>,
    /// Stable session id owning this MCP instance. Used to attribute
    /// `session.spawn_child` calls to the right parent session in the tree.
    /// `None` for legacy callers that pre-date the `--session-id` flag.
    session_id: Option<String>,
    /// Base URL of the harness-server (e.g. `http://127.0.0.1:8787`). When
    /// `Some`, task creation/proposal delegates to REST so the in-process
    /// broadcast bus emits `task.created` and the SSE stream pushes the new
    /// task into the right panel without the user having to refresh.
    server_url: Option<String>,
    api_token: Option<String>,
    cwd: PathBuf,
    gateway: Gateway,
    seeded_mcp_servers: HashSet<String>,
    requested_capabilities: Mutex<HashSet<String>>,
    tool_registry: ToolRegistry,
    active_tool_groups: Mutex<HashSet<String>>,
    descriptor_cache: Mutex<Option<(Vec<String>, Vec<crate::protocol::ToolDescriptor>)>>,
    notifications: Mutex<Vec<Value>>,
}

impl Dispatcher {
    #[allow(dead_code)]
    pub fn new(harness_home: PathBuf, thread_id: String, agent_id: String) -> Result<Self, String> {
        Self::new_with_server(
            harness_home,
            thread_id,
            agent_id,
            None,
            "default".into(),
            None,
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            None,
            None,
            None,
            Vec::new(),
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_server(
        harness_home: PathBuf,
        thread_id: String,
        agent_id: String,
        session_id: Option<String>,
        profile: String,
        server_url: Option<String>,
        cwd: PathBuf,
        api_token: Option<String>,
        role: Option<String>,
        task_id: Option<String>,
        scopes: Vec<String>,
        upstream_config: Option<PathBuf>,
    ) -> Result<Self, String> {
        let store = TaskStore::with_profile(&harness_home, &profile).map_err(|e| e.to_string())?;
        let db = DbManager::new(&harness_home, &profile).map_err(|e| e.to_string())?;
        let ssh = SshManager::new(&harness_home, &profile).map_err(|e| e.to_string())?;
        let policy_path = harness_home
            .join("profiles")
            .join(&profile)
            .join("policy.toml");
        let (policy, policy_load_error) = match PolicyEngine::load(policy_path) {
            Ok(policy) => (Some(policy), None),
            Err(e) => {
                let msg = e.to_string();
                warn!(error = %msg, "failed to load local MCP policy");
                (None, Some(msg))
            }
        };
        let gateway = match upstream_config.as_deref() {
            Some(path) => Gateway::from_config_path(path)?,
            None => Gateway::default(),
        };

        let loaded_capabilities =
            seed_loaded_capabilities(&harness_home, &profile, session_id.as_deref());
        let tool_registry = ToolRegistry::new(tools::list_descriptors());
        let active_tool_groups = loaded_capabilities
            .tool_groups
            .into_iter()
            .filter_map(|group| tool_registry.canonical_group(&group).map(str::to_string))
            .filter(|group| group != "core")
            .collect();
        let seeded_mcp_servers = loaded_capabilities.mcp_servers.into_iter().collect();

        Ok(Self {
            store,
            db,
            ssh,
            harness_home,
            profile,
            thread_id,
            agent_id,
            role,
            task_id,
            scopes,
            policy,
            policy_load_error,
            session_id,
            server_url,
            api_token,
            cwd,
            gateway,
            seeded_mcp_servers,
            requested_capabilities: Mutex::new(HashSet::new()),
            tool_registry,
            active_tool_groups: Mutex::new(active_tool_groups),
            descriptor_cache: Mutex::new(None),
            notifications: Mutex::new(Vec::new()),
        })
    }

    /// Handle a request. Returns `None` for notifications (no id).
    pub fn handle(&self, req: Request) -> Option<Value> {
        let id = req.id.clone();

        // Notifications have no id and never produce a response.
        let is_notification = id.is_none();
        let id = id.unwrap_or(Value::Null);

        match req.method.as_str() {
            "initialize" => Some(result_response(
                id,
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": SERVER_NAME, "version": SERVER_VERSION }
                }),
            )),
            "notifications/initialized" | "initialized" => None,
            "notifications/cancelled" => None,
            "ping" => Some(result_response(id, json!({}))),
            "tools/list" => Some(result_response(
                id,
                json!({ "tools": self.list_tool_descriptors() }),
            )),
            "tools/call" => {
                if is_notification {
                    return None;
                }
                Some(self.handle_tool_call(id, req.params))
            }
            other => {
                if is_notification {
                    warn!(method = %other, "unhandled notification");
                    return None;
                }
                Some(error_response_with(
                    id,
                    RpcError::MethodNotFound,
                    &format!("method not found: {other}"),
                ))
            }
        }
    }

    pub fn drain_notifications(&self) -> Vec<Value> {
        self.notifications
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .drain(..)
            .collect()
    }

    fn handle_tool_call(&self, id: Value, params: Value) -> Value {
        let name = match params.get("name").and_then(|v| v.as_str()) {
            Some(n) => n.to_string(),
            None => {
                return error_response_with(id, RpcError::InvalidParams, "missing tool name");
            }
        };
        let args = params.get("arguments").cloned().unwrap_or(json!({}));

        let auto_loaded = self.auto_load_group_for_tool(&name);

        if let Some(msg) = self.check_tool_policy(&name, &args) {
            return result_response(id, wrap_error(&msg));
        }

        if self.gateway.prefixed_tool(&name).is_some() {
            return match self.gateway.call(&name, args) {
                Ok(result) => result_response(
                    id,
                    self.truncate_tool_result(
                        &name,
                        self.apply_auto_load_note(result, auto_loaded),
                    ),
                ),
                Err(msg) => result_response(id, self.truncate_tool_result(&name, wrap_error(&msg))),
            };
        }

        let outcome: Result<Value, String> = match name.as_str() {
            "capability_list" => Ok(capability_tools::list(self.capability_runtime())),
            "capability_describe" => capability_tools::describe(self.capability_runtime(), &args),
            "capability_request" => {
                match capability_tools::request(self.capability_runtime(), &args) {
                    Ok(id) => {
                        self.requested_capabilities
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .insert(id.clone());
                        let _ = self.load_tool_groups(std::slice::from_ref(&id));
                        Ok(json!({
                            "id": id,
                            "status": "loaded",
                            "message": "Capability tools will be included in subsequent tools/list responses for this MCP session."
                        }))
                    }
                    Err(e) => Err(e),
                }
            }
            "tools_search" => self.tools_search(&args),
            "tools_load" => self.tools_load(&args),
            "tools_unload" => self.tools_unload(&args),
            "planning_pack" => planning::pack(&args),
            "test_selector" => planning::test_selector(&args),
            "contract_guard" => planning::contract_guard(&args),
            "evidence_pack" => evidence_tools::pack(
                &self.store,
                &self.harness_home,
                &self.profile,
                &self.cwd,
                &self.thread_id,
                self.session_id.as_deref(),
                self.task_id.as_deref(),
                &args,
            ),
            "session_context_pack" => session_tools::context_pack(
                &self.store,
                &self.harness_home,
                &self.profile,
                self.session_id.as_deref(),
                &self.thread_id,
                &args,
            ),
            "context_status" => session_tools::context_status(
                self.session_id.as_deref(),
                self.server_url.as_deref(),
                self.api_token.as_deref(),
                &args,
            ),
            "context_search" => session_tools::context_search(
                self.session_id.as_deref(),
                self.server_url.as_deref(),
                self.api_token.as_deref(),
                &args,
            ),
            "context_checkpoint_request" => session_tools::context_checkpoint_request(
                self.session_id.as_deref(),
                self.server_url.as_deref(),
                self.api_token.as_deref(),
                &args,
            ),
            "timeline_query" => session_tools::timeline_query(
                &self.thread_id,
                self.server_url.as_deref(),
                self.api_token.as_deref(),
                &args,
            ),
            "transcript_query" => session_tools::transcript_query(
                self.session_id.as_deref(),
                self.server_url.as_deref(),
                self.api_token.as_deref(),
                &args,
            ),
            "transcript_search" => session_tools::transcript_search(
                self.session_id.as_deref(),
                self.server_url.as_deref(),
                self.api_token.as_deref(),
                &args,
            ),
            "transcript_tool_results" => session_tools::transcript_tool_results(
                self.session_id.as_deref(),
                self.server_url.as_deref(),
                self.api_token.as_deref(),
                &args,
            ),
            "attach_list" => attachment_tools::list(&self.harness_home, self.session_id.as_deref()),
            "attach_read" => {
                attachment_tools::read(&self.harness_home, self.session_id.as_deref(), &args)
            }
            "n8n_configure" => n8n_tools::configure(&self.harness_home, &self.profile, &args),
            "n8n_status" => n8n_tools::status(&self.harness_home, &self.profile, &args),
            "n8n_local_start" => n8n_tools::local_start(&self.harness_home, &self.profile, &args),
            "n8n_local_stop" => n8n_tools::local_stop(&self.harness_home, &self.profile, &args),
            "n8n_save_workflow" => {
                n8n_tools::save_workflow(&self.harness_home, &self.profile, &args)
            }
            "n8n_list_saved_workflows" => {
                n8n_tools::list_saved_workflows(&self.harness_home, &self.profile)
            }
            "n8n_read_workflow" => {
                n8n_tools::read_workflow(&self.harness_home, &self.profile, &args)
            }
            "n8n_validate_workflow" => n8n_tools::validate_workflow(&args),
            "n8n_import_workflow" => {
                n8n_tools::import_workflow(&self.harness_home, &self.profile, &args)
            }
            "n8n_list_remote_workflows" => {
                n8n_tools::list_remote_workflows(&self.harness_home, &self.profile, &args)
            }
            "n8n_activate_workflow" => {
                n8n_tools::activate_workflow(&self.harness_home, &self.profile, &args)
            }
            "n8n_deactivate_workflow" => {
                n8n_tools::deactivate_workflow(&self.harness_home, &self.profile, &args)
            }
            "n8n_webhook_request" => {
                n8n_tools::webhook_request(&self.harness_home, &self.profile, &args)
            }
            "task_create" => tasks::create(
                &self.store,
                &self.thread_id,
                &self.agent_id,
                self.server_url.as_deref(),
                self.api_token.as_deref(),
                &args,
            ),
            "task_propose" => tasks::propose(
                &self.store,
                &self.thread_id,
                &self.agent_id,
                self.server_url.as_deref(),
                self.api_token.as_deref(),
                &args,
            ),
            "task_list" => tasks::list(&self.store, &self.thread_id, &args),
            "task_list_summary" => tasks::list_summary(&self.store, &self.thread_id, &args),
            "task_next_best" => tasks::next_best(&self.store, &self.thread_id, &args),
            "task_get" => tasks::get(&self.store, &self.thread_id, &args),
            "task_claim" => tasks::claim(&self.store, &self.thread_id, &args),
            "task_renew" => tasks::renew(&self.store, &self.thread_id, &args),
            "task_update" => tasks::update(&self.store, &self.thread_id, &self.agent_id, &args),
            "task_release" => tasks::release(&self.store, &self.thread_id, &args),
            "task_submit" => tasks::submit(&self.store, &self.thread_id, &self.agent_id, &args),
            "spec_read" => spec::read(&self.harness_home, &self.thread_id, &args),
            "spec_write" => spec::write(
                &self.harness_home,
                self.server_url.as_deref(),
                self.api_token.as_deref(),
                &args,
            ),
            "spec_set_section" => spec::set_section(
                &self.harness_home,
                self.server_url.as_deref(),
                self.api_token.as_deref(),
                &args,
            ),
            "knowledge_pdf_ingest" => {
                knowledge_tools::pdf_ingest(&self.harness_home, &self.profile, &args)
            }
            "knowledge_office_ingest" => {
                knowledge_tools::office_ingest(&self.harness_home, &self.profile, &args)
            }
            "knowledge_data_ingest" => {
                knowledge_tools::data_ingest(&self.harness_home, &self.profile, &args)
            }
            "knowledge_search" => knowledge_tools::search(&self.harness_home, &self.profile, &args),
            "skills_search" => skills::search(&self.harness_home, &self.profile, &args),
            "skill_propose" => skills::propose(&self.harness_home, &self.profile, &args),
            "skill_promote" => skills::promote(&self.harness_home, &self.profile, &args),
            "skill_archive" => skills::archive(&self.harness_home, &self.profile, &args),
            "skill_record_usage" => skills::record_usage(&self.harness_home, &self.profile, &args),
            "evolve_observe" => skills::observe(&self.harness_home, &self.profile, &args),
            "evolve_run" => skills::evolve_run(&self.harness_home, &self.profile, &args),
            "curator_run" => skills::curator_run(&self.harness_home, &self.profile, &args),
            "repo_analyze" => repo::analyze(&self.cwd, &args),
            "repo_scan" => repo::scan(&self.cwd, &args),
            "repo_find" => repo::find(&self.cwd, &args),
            "repo_read_file" => repo::read_file(&self.cwd, &args),
            "repo_write_file" => match self.repo_write_scope() {
                Ok((write_paths, forbidden_paths)) => {
                    repo::write_file(&self.cwd, &args, &write_paths, &forbidden_paths)
                }
                Err(e) => Err(e),
            },
            "repo_git_status" => repo::git_status(&self.cwd, &args),
            "repo_git_log" => repo::git_log(&self.cwd, &args),
            "repo_git_diff" => repo::git_diff(&self.cwd, &args),
            "repo_git_create_branch" => repo::git_branch_create(&self.cwd, &args),
            "repo_git_commit" => match self.repo_write_scope() {
                Ok((write_paths, forbidden_paths)) => {
                    repo::git_commit(&self.cwd, &args, &write_paths, &forbidden_paths)
                }
                Err(e) => Err(e.replace("repo_write_file", "repo_git_commit")),
            },
            "repo_git_push" => repo::git_push(&self.cwd, &args),
            "repo_github_pr_create" => repo::git_pr_create(&self.cwd, &args),
            "repo_codebase_memory_status" => repo::codebase_memory_status(&self.cwd, &args),
            "repo_manifest" => repo::manifest(&self.cwd, &args),
            "repo_symbol_search" => repo::symbol_search(&self.cwd, &args),
            "repo_related_files" => repo::related_files(&self.cwd, &args),
            "repo_code_graph_status" => repo::code_graph_status(&self.cwd, &args),
            "docs_build" => docs_tools::build(&self.cwd, &args),
            "db_query" => db_tools::query(&self.db, &args),
            "db_context_refresh" => db_tools::context_refresh(&self.db, &args),
            "db_context" => db_tools::context(&self.db, &args),
            "db_select" => db_tools::select(&self.db, &args),
            "db_validate_query" => db_tools::validate_query(&self.db, &args),
            "db_schema" => db_tools::schema(&self.db, &args),
            "db_table_info" => db_tools::table_info(&self.db, &args),
            "db_search_tables" => db_tools::search_tables(&self.db, &args),
            "db_sample" => db_tools::sample(&self.db, &args),
            "db_count" => db_tools::count(&self.db, &args),
            "db_distinct_values" => db_tools::distinct_values(&self.db, &args),
            "db_find_rows" => db_tools::find_rows(&self.db, &args),
            "db_aggregate" => db_tools::aggregate(&self.db, &args),
            "db_extract_enriched" => db_tools::extract_enriched(&self.db, &args),
            "db_relation_performance" => db_tools::relation_performance(&self.db, &args),
            "db_row_insert" => db_tools::row_insert(&self.db, &args),
            "db_row_delete" => db_tools::row_delete(&self.db, &args),
            "db_row_duplicate" => db_tools::row_duplicate(&self.db, &args),
            "db_export_table" => db_tools::export_table(&self.db, &self.harness_home, &args),
            "db_export_query" => db_tools::export_query(&self.db, &self.harness_home, &args),
            "db_generate_view_sql" => db_tools::generate_view_sql(&self.db, &args),
            "db_drop_table" => db_tools::drop_table(&self.db, &args),
            "db_drop_schema" => db_tools::drop_schema(&self.db, &args),
            "db_explain" => db_tools::explain(&self.db, &args),
            "db_performance_audit" => db_tools::performance_audit(&self.db, &args),
            "db_backup" => db_tools::backup(&self.db, &self.harness_home, &args),
            "db_memory_read" => db_tools::memory_read(&self.harness_home, &self.profile, &args),
            "db_memory_write" => db_tools::memory_write(&self.harness_home, &self.profile, &args),
            "ssh_hosts" => ssh_tools::hosts(&self.ssh),
            "ssh_test" => ssh_tools::test_host(&self.ssh, &args),
            "ssh_exec" => ssh_tools::exec(&self.ssh, &args),
            "ssh_context_refresh" => ssh_tools::context_refresh(&self.ssh, &args),
            "ssh_context" => ssh_tools::context(&self.ssh, &args),
            "sftp_list" => ssh_tools::sftp_list(&self.ssh, &args),
            "sftp_get" => ssh_tools::sftp_get(&self.ssh, &args),
            "sftp_put" => ssh_tools::sftp_put(&self.ssh, &args),
            "sftp_mkdir" => ssh_tools::sftp_mkdir(&self.ssh, &args),
            "sftp_rmdir" => ssh_tools::sftp_rmdir(&self.ssh, &args),
            "sftp_unlink" => ssh_tools::sftp_unlink(&self.ssh, &args),
            "sftp_rename" => ssh_tools::sftp_rename(&self.ssh, &args),
            "session_spawn_child" => session_tools::spawn_child(
                self.session_id.as_deref(),
                self.server_url.as_deref(),
                self.api_token.as_deref(),
                &args,
            ),
            "session_list_children" => session_tools::list_children(
                self.session_id.as_deref(),
                self.server_url.as_deref(),
                self.api_token.as_deref(),
            ),
            "session_read_child_summary" => session_tools::read_child_summary(
                self.session_id.as_deref(),
                self.server_url.as_deref(),
                self.api_token.as_deref(),
                &args,
            ),
            "session_send_input" => session_tools::send_input(
                self.session_id.as_deref(),
                self.server_url.as_deref(),
                self.api_token.as_deref(),
                &args,
            ),
            "session_mailbox_send" => session_tools::mailbox_send(
                self.session_id.as_deref(),
                self.server_url.as_deref(),
                self.api_token.as_deref(),
                &args,
            ),
            "session_mailbox_list" => session_tools::mailbox_list(
                self.session_id.as_deref(),
                self.server_url.as_deref(),
                self.api_token.as_deref(),
            ),
            "session_mailbox_ack" => session_tools::mailbox_ack(
                self.session_id.as_deref(),
                self.server_url.as_deref(),
                self.api_token.as_deref(),
                &args,
            ),
            "session_cancel_child" => session_tools::cancel_child(
                self.session_id.as_deref(),
                self.server_url.as_deref(),
                self.api_token.as_deref(),
                &args,
            ),
            other => {
                return error_response_with(
                    id,
                    RpcError::MethodNotFound,
                    &format!("unknown tool: {other}"),
                );
            }
        };

        match outcome {
            Ok(payload) => result_response(
                id,
                self.truncate_tool_result(
                    &name,
                    self.apply_auto_load_note(wrap_text(&payload), auto_loaded),
                ),
            ),
            Err(msg) => {
                // Per MCP spec, tool errors should be returned as a normal
                // result with isError=true, NOT as a JSON-RPC error (those are
                // reserved for protocol-level failures). This keeps the agent
                // loop alive and surfaces a structured message to the model.
                result_response(id, self.truncate_tool_result(&name, wrap_error(&msg)))
            }
        }
    }

    fn list_tool_descriptors(&self) -> Vec<crate::protocol::ToolDescriptor> {
        let active = self.active_tool_group_snapshot();
        let cache_key = active_cache_key(&active);
        if let Some((key, descriptors)) = self
            .descriptor_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .filter(|(key, _)| key == &cache_key)
            .cloned()
        {
            let _ = key;
            return descriptors;
        }

        let mut descriptors = match self.tool_registry.visible_descriptors(&active) {
            Ok(descriptors) => descriptors,
            Err(msg) => {
                warn!(error = %msg, "failed to resolve active tool groups");
                Vec::new()
            }
        };
        let mut gateway_names = Vec::new();
        if self.should_list_crawl4ai_gateway(&active) {
            gateway_names.push("crawl4ai");
        }
        if self.should_list_codebase_memory_gateway(&active) {
            gateway_names.push("codebase_memory");
        }
        if !gateway_names.is_empty() {
            descriptors.extend(self.gateway.list_descriptors_for(&gateway_names));
        }
        *self
            .descriptor_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some((cache_key, descriptors.clone()));
        descriptors
    }

    fn capability_runtime(&self) -> capability_tools::CapabilityRuntime {
        capability_tools::CapabilityRuntime {
            docs_web_loaded: self.gateway.has_upstream("crawl4ai"),
            requested: self.requested_capability_snapshot().into_iter().collect(),
        }
    }

    fn requested_capability_snapshot(&self) -> HashSet<String> {
        self.requested_capabilities
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn active_tool_group_snapshot(&self) -> HashSet<String> {
        self.active_tool_groups
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn tools_search(&self, args: &Value) -> Result<Value, String> {
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "tools_search requires query".to_string())?;
        Ok(self
            .tool_registry
            .search(&self.active_tool_group_snapshot(), query))
    }

    fn tools_load(&self, args: &Value) -> Result<Value, String> {
        let groups = parse_groups_arg(args, "tools_load")?;
        let loaded = self.load_tool_groups(&groups)?;
        let active = active_cache_key(&self.active_tool_group_snapshot());
        Ok(json!({
            "loaded": loaded,
            "active_groups": active,
            "message": "Tool groups loaded. Clients should refresh tools/list."
        }))
    }

    fn tools_unload(&self, args: &Value) -> Result<Value, String> {
        let groups = parse_groups_arg(args, "tools_unload")?;
        let groups = self.canonicalize_tool_groups(&groups)?;
        let mut unloaded = Vec::new();
        let mut changed = false;
        {
            let mut active = self
                .active_tool_groups
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            for group in groups {
                if group == "core" {
                    continue;
                }
                if active.remove(&group) {
                    unloaded.push(group);
                    changed = true;
                }
            }
        }
        if changed {
            self.invalidate_tool_descriptor_cache();
            self.queue_tools_list_changed();
        }
        let active = active_cache_key(&self.active_tool_group_snapshot());
        Ok(json!({
            "unloaded": unloaded,
            "active_groups": active,
            "message": "Tool groups unloaded. Clients should refresh tools/list."
        }))
    }

    fn auto_load_group_for_tool(&self, tool_name: &str) -> Option<String> {
        let group = self.tool_registry.group_for_tool(tool_name)?;
        if group == "core" || self.active_tool_group_snapshot().contains(group) {
            return None;
        }
        match self.load_tool_groups(&[group.to_string()]) {
            Ok(loaded) if loaded.iter().any(|item| item == group) => Some(group.to_string()),
            Ok(_) => None,
            Err(msg) => {
                warn!(tool = %tool_name, group = %group, error = %msg, "tool auto-load failed");
                None
            }
        }
    }

    fn load_tool_groups(&self, groups: &[String]) -> Result<Vec<String>, String> {
        let groups = self.canonicalize_tool_groups(groups)?;
        let mut loaded = Vec::new();
        let mut changed = false;
        {
            let mut active = self
                .active_tool_groups
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            for group in groups {
                if group == "core" {
                    continue;
                }
                if active.insert(group.clone()) {
                    loaded.push(group);
                    changed = true;
                }
            }
        }
        if changed {
            self.invalidate_tool_descriptor_cache();
            self.queue_tools_list_changed();
        }
        Ok(loaded)
    }

    fn canonicalize_tool_groups(&self, groups: &[String]) -> Result<Vec<String>, String> {
        groups
            .iter()
            .map(|group| {
                self.tool_registry
                    .canonical_group(group)
                    .map(str::to_string)
                    .ok_or_else(|| format!("unknown tool group: {group}"))
            })
            .collect()
    }

    fn should_list_crawl4ai_gateway(&self, active: &HashSet<String>) -> bool {
        self.gateway.has_upstream("crawl4ai")
            && (self.seeded_mcp_servers.contains("crawl4ai")
                || active.contains("knowledge")
                || active.contains("docs"))
    }

    fn should_list_codebase_memory_gateway(&self, active: &HashSet<String>) -> bool {
        self.gateway.has_upstream("codebase_memory")
            && (self.seeded_mcp_servers.contains("codebase_memory")
                || active.contains("code_graph"))
    }

    fn invalidate_tool_descriptor_cache(&self) {
        *self
            .descriptor_cache
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
    }

    fn queue_tools_list_changed(&self) {
        self.notifications
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(json!({
                "jsonrpc": "2.0",
                "method": "notifications/tools/list_changed",
                "params": {}
            }));
    }

    fn apply_auto_load_note(&self, result: Value, auto_loaded: Option<String>) -> Value {
        let Some(group) = auto_loaded else {
            return result;
        };
        if result.get("isError").and_then(|v| v.as_bool()) == Some(true) {
            return result;
        }
        let note = json!({
            "type": "text",
            "text": format!(
                "note: auto-loaded tool group `{group}` before executing this tool. Refresh tools/list to see the expanded schema set."
            )
        });
        match result {
            Value::Object(mut map) => {
                if let Some(content) = map
                    .get_mut("content")
                    .and_then(|value| value.as_array_mut())
                {
                    content.push(note);
                    Value::Object(map)
                } else {
                    let result = Value::Object(map);
                    json!({ "content": [note, { "type": "text", "text": result.to_string() }] })
                }
            }
            other => json!({ "content": [note, { "type": "text", "text": other.to_string() }] }),
        }
    }

    fn truncate_tool_result(&self, tool_name: &str, mut result: Value) -> Value {
        let policy = truncation_policy(tool_name);
        let allow_byte_truncation = allows_byte_truncation(tool_name);
        let Some(content) = result.get_mut("content").and_then(Value::as_array_mut) else {
            return result;
        };
        for item in content {
            if item.get("type").and_then(Value::as_str) != Some("text") {
                continue;
            }
            let Some(text) = item.get("text").and_then(Value::as_str).map(str::to_string) else {
                continue;
            };
            let truncated = truncate_text_result(&text, policy, allow_byte_truncation);
            if truncated.changed {
                *item.get_mut("text").expect("checked text field") = Value::String(truncated.text);
            }
        }
        result
    }

    fn check_tool_policy(&self, tool_name: &str, tool_args: &Value) -> Option<String> {
        self.check_tool_policy_with_timeout(tool_name, tool_args, POLICY_CHECK_TIMEOUT)
    }

    fn check_tool_policy_with_timeout(
        &self,
        tool_name: &str,
        tool_args: &Value,
        timeout: Duration,
    ) -> Option<String> {
        let Some(server_url) = self.server_url.as_deref() else {
            return self.check_local_tool_policy(tool_name, tool_args);
        };
        let payload = json!({
            "tool": tool_name,
            "args": tool_args,
            "thread_id": self.thread_id,
            "agent_id": self.agent_id,
            "role": self.role.as_deref(),
        });
        let url = format!("{}/api/approvals/check", server_url.trim_end_matches('/'));
        let mut req = ureq::post(&url).timeout(timeout);
        if let Some(token) = self.api_token.as_deref() {
            req = req.set("Authorization", &format!("Bearer {token}"));
        }
        match req.send_json(payload) {
            Ok(resp) => match resp.into_json::<Value>() {
                Ok(value) => match value.get("decision").and_then(|v| v.as_str()) {
                    Some("allow") => None,
                    Some("deny") => Some(policy_denied_message(tool_name)),
                    Some(other) => Some(format!(
                        "approval check returned unknown decision `{other}` for {tool_name}; failing closed"
                    )),
                    None => {
                        Some(format!(
                            "approval check response missing decision for {tool_name}; failing closed"
                        ))
                    }
                },
                Err(e) => {
                    warn!(error = %e, "approval check response parse failed");
                    Some(format!(
                        "approval check response parse failed for {tool_name}; failing closed"
                    ))
                }
            },
            Err(e) => {
                warn!(error = %e, "approval check failed");
                Some(format!(
                    "approval check failed for {tool_name} within {}; failing closed",
                    format_duration(timeout)
                ))
            }
        }
    }

    fn check_local_tool_policy(&self, tool_name: &str, tool_args: &Value) -> Option<String> {
        if self.offline_role_is_untrusted() && is_sensitive_tool(tool_name) {
            return Some(policy_denied_message(tool_name));
        }

        let Some(policy) = self.policy.as_ref() else {
            return if is_sensitive_tool(tool_name) {
                Some(format!(
                    "tool call denied by policy: {tool_name}; local policy failed to load: {}",
                    self.policy_load_error
                        .as_deref()
                        .unwrap_or("unknown policy load error")
                ))
            } else {
                None
            };
        };

        if let Some(decision) = policy.evaluate_rule(tool_name, tool_args, self.role.as_deref()) {
            return match decision {
                Decision::Allow => None,
                Decision::Deny => Some(policy_denied_message(tool_name)),
                Decision::Ask => Some(format!(
                    "tool call requires approval by policy: {tool_name}; no approval server is configured"
                )),
            };
        }

        match capability_default(tool_name, self.role.as_deref()) {
            Some(Decision::Deny) => Some(policy_denied_message(tool_name)),
            _ => None,
        }
    }

    fn offline_role_is_untrusted(&self) -> bool {
        !matches!(
            self.role.as_deref(),
            Some("planner" | "orchestrator" | "worker" | "generator" | "evaluator")
        )
    }

    fn repo_write_scope(&self) -> Result<(Vec<String>, Vec<String>), String> {
        let task_id = self.task_id.as_deref().ok_or_else(|| {
            "repo_write_file denied: MCP session is not scoped to a task".to_string()
        })?;
        if !self
            .scopes
            .iter()
            .any(|scope| scope == &format!("task:{task_id}"))
        {
            return Err(format!(
                "repo_write_file denied: MCP session scope does not include task:{task_id}"
            ));
        }
        let task = self
            .store
            .get(&self.thread_id, task_id)
            .map_err(|e| format!("repo_write_file denied: cannot load task scope: {e}"))?;
        if task.write_paths.is_empty() {
            return Err(format!(
                "repo_write_file denied: task {task_id} has no write_paths"
            ));
        }
        Ok((task.write_paths, task.forbidden_paths))
    }
}

fn policy_denied_message(tool_name: &str) -> String {
    match tool_name {
        "task_create" => "denied_by_role: task_create; usa task_propose".to_string(),
        _ => format!("denied_by_role: {tool_name}"),
    }
}

fn format_duration(duration: Duration) -> String {
    if duration.as_millis() < 1_000 {
        format!("{}ms", duration.as_millis())
    } else {
        format!("{}s", duration.as_secs())
    }
}

fn active_cache_key(active: &HashSet<String>) -> Vec<String> {
    let mut groups: Vec<_> = active.iter().cloned().collect();
    groups.sort();
    groups
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TruncationMode {
    Head,
    Tail,
}

#[derive(Debug, Clone, Copy)]
struct TruncationPolicy {
    mode: TruncationMode,
    max_lines: usize,
    max_bytes: usize,
}

#[derive(Debug, PartialEq, Eq)]
struct TruncatedText {
    text: String,
    changed: bool,
}

fn truncation_policy(tool_name: &str) -> TruncationPolicy {
    let mode = match tool_name {
        "ssh_exec" => TruncationMode::Tail,
        name if name.starts_with("repo_git_") => TruncationMode::Tail,
        _ => TruncationMode::Head,
    };
    TruncationPolicy {
        mode,
        max_lines: DEFAULT_TRUNCATION_LINES,
        max_bytes: DEFAULT_TRUNCATION_BYTES,
    }
}

fn allows_byte_truncation(tool_name: &str) -> bool {
    tool_name == "repo_read_file"
        || tool_name == "ssh_exec"
        || tool_name.starts_with("repo_git_")
        || tool_name.contains("__")
}

fn truncate_text_result(
    text: &str,
    policy: TruncationPolicy,
    allow_byte_truncation: bool,
) -> TruncatedText {
    let line_count = text.lines().count();
    let exceeds_byte_limit = allow_byte_truncation && text.len() > policy.max_bytes;
    if line_count <= policy.max_lines && !exceeds_byte_limit {
        return TruncatedText {
            text: text.to_string(),
            changed: false,
        };
    }

    let range = match policy.mode {
        TruncationMode::Head => {
            let line_end = byte_after_first_lines(text, policy.max_lines);
            let byte_end = if allow_byte_truncation {
                previous_char_boundary(text, policy.max_bytes.min(text.len()))
            } else {
                text.len()
            };
            0..line_end.min(byte_end)
        }
        TruncationMode::Tail => {
            let line_start = byte_before_last_lines(text, policy.max_lines);
            let byte_start = if allow_byte_truncation {
                next_char_boundary(text, text.len().saturating_sub(policy.max_bytes))
            } else {
                0
            };
            line_start.max(byte_start)..text.len()
        }
    };

    let kept = &text[range.clone()];
    let omitted_bytes = text.len().saturating_sub(kept.len());
    let kept_lines = kept.lines().count();
    let omitted_lines = line_count.saturating_sub(kept_lines);
    let notice = format!("[truncated: {omitted_lines} lines / {omitted_bytes} bytes omitted]");
    let text = match policy.mode {
        TruncationMode::Head => format!("{kept}\n{notice}"),
        TruncationMode::Tail => format!("{notice}\n{kept}"),
    };
    TruncatedText {
        text,
        changed: true,
    }
}

fn byte_after_first_lines(text: &str, max_lines: usize) -> usize {
    if max_lines == 0 {
        return 0;
    }
    let mut lines_seen = 0usize;
    for (idx, ch) in text.char_indices() {
        if ch == '\n' {
            lines_seen += 1;
            if lines_seen == max_lines {
                return idx;
            }
        }
    }
    text.len()
}

fn byte_before_last_lines(text: &str, max_lines: usize) -> usize {
    if max_lines == 0 {
        return text.len();
    }
    let mut newlines_seen = 0usize;
    for (idx, ch) in text.char_indices().rev() {
        if ch == '\n' {
            newlines_seen += 1;
            if newlines_seen == max_lines {
                return idx + ch.len_utf8();
            }
        }
    }
    0
}

fn previous_char_boundary(text: &str, mut idx: usize) -> usize {
    while idx > 0 && !text.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn next_char_boundary(text: &str, mut idx: usize) -> usize {
    while idx < text.len() && !text.is_char_boundary(idx) {
        idx += 1;
    }
    idx
}

fn parse_groups_arg(args: &Value, tool: &str) -> Result<Vec<String>, String> {
    let groups = args
        .get("groups")
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("{tool} requires groups: string[]"))?;
    groups
        .iter()
        .map(|group| {
            group
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("{tool} groups must be strings"))
        })
        .collect()
}

#[derive(Default)]
struct SeededLoadedCapabilities {
    mcp_servers: Vec<String>,
    tool_groups: Vec<String>,
}

fn seed_loaded_capabilities(
    harness_home: &std::path::Path,
    profile: &str,
    session_id: Option<&str>,
) -> SeededLoadedCapabilities {
    let Some(session_id) = session_id else {
        return SeededLoadedCapabilities::default();
    };
    let meta_path = harness_home
        .join("profiles")
        .join(profile)
        .join("sessions")
        .join(session_id)
        .join("meta.json");
    let Ok(bytes) = std::fs::read(&meta_path) else {
        return SeededLoadedCapabilities::default();
    };
    match serde_json::from_slice::<SessionMeta>(&bytes) {
        Ok(meta) => SeededLoadedCapabilities {
            mcp_servers: meta.loaded_capabilities.mcp_servers,
            tool_groups: meta.loaded_capabilities.tool_groups,
        },
        Err(e) => {
            warn!(path = %meta_path.display(), error = %e, "failed to read MCP session loaded capabilities");
            SeededLoadedCapabilities::default()
        }
    }
}

// Keep error_response symbol used so importers don't get warnings.
#[allow(dead_code)]
fn _unused() -> Value {
    error_response(Value::Null, RpcError::InternalError)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::parse_request;

    fn tmp_home() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "harness-mcp-disp-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn mk(thread: &str, agent: &str) -> (Dispatcher, PathBuf) {
        let home = tmp_home();
        let d = Dispatcher::new(home.clone(), thread.to_string(), agent.to_string()).unwrap();
        (d, home)
    }

    fn mk_with_cwd(thread: &str, agent: &str, cwd: PathBuf) -> (Dispatcher, PathBuf) {
        let home = tmp_home();
        let d = Dispatcher::new_with_server(
            home.clone(),
            thread.to_string(),
            agent.to_string(),
            None,
            "default".into(),
            None,
            cwd,
            None,
            None,
            None,
            Vec::new(),
            None,
        )
        .unwrap();
        (d, home)
    }

    fn mk_with_role(thread: &str, agent: &str, role: Option<&str>) -> (Dispatcher, PathBuf) {
        let home = tmp_home();
        let d = Dispatcher::new_with_server(
            home.clone(),
            thread.to_string(),
            agent.to_string(),
            None,
            "default".into(),
            None,
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            None,
            role.map(String::from),
            None,
            Vec::new(),
            None,
        )
        .unwrap();
        (d, home)
    }

    fn mk_with_role_and_policy(
        thread: &str,
        agent: &str,
        role: Option<&str>,
        policy: &str,
    ) -> (Dispatcher, PathBuf) {
        let home = tmp_home();
        let policy_dir = home.join("profiles/default");
        std::fs::create_dir_all(&policy_dir).unwrap();
        std::fs::write(policy_dir.join("policy.toml"), policy).unwrap();
        let d = Dispatcher::new_with_server(
            home.clone(),
            thread.to_string(),
            agent.to_string(),
            None,
            "default".into(),
            None,
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            None,
            role.map(String::from),
            None,
            Vec::new(),
            None,
        )
        .unwrap();
        (d, home)
    }

    fn mk_scoped_writer(
        cwd: PathBuf,
        write_paths: Vec<&str>,
        forbidden_paths: Vec<&str>,
    ) -> Dispatcher {
        use harness_core::TaskDraft;

        let home = tmp_home();
        let d = Dispatcher::new_with_server(
            home,
            "t-write".to_string(),
            "agent:writer".to_string(),
            None,
            "default".into(),
            None,
            cwd,
            None,
            Some("generator".into()),
            Some("T-0001".into()),
            vec!["task:T-0001".into()],
            None,
        )
        .unwrap();
        d.store
            .create(
                "t-write",
                TaskDraft {
                    title: "write scoped file".into(),
                    write_paths: write_paths.into_iter().map(String::from).collect(),
                    forbidden_paths: forbidden_paths.into_iter().map(String::from).collect(),
                    created_by: "planner".into(),
                    ..TaskDraft::default()
                },
            )
            .unwrap();
        d
    }

    #[test]
    fn initialize_then_list_tools() {
        let (d, _) = mk("t1", "agent:1");

        let init_line = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
        let req = parse_request(init_line).unwrap();
        let resp = d.handle(req).unwrap();
        assert_eq!(resp["result"]["protocolVersion"], PROTOCOL_VERSION);

        let initialized = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
        let req = parse_request(initialized).unwrap();
        assert!(d.handle(req).is_none(), "notifications produce no response");

        let list_line = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;
        let req = parse_request(list_line).unwrap();
        let resp = d.handle(req).unwrap();
        let tools = resp["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(
            names.len() <= 23,
            "base tools/list too large: {}",
            names.len()
        );
        assert_eq!(names[0], "tools_search");
        assert_eq!(names[1], "tools_load");
        assert_eq!(names[2], "tools_unload");
        for expected in [
            "tools_search",
            "tools_load",
            "tools_unload",
            "attach_list",
            "attach_read",
            "task_create",
            "task_propose",
            "task_list",
            "task_get",
            "task_claim",
            "task_renew",
            "task_update",
            "task_release",
            "task_submit",
            "spec_read",
            "session_spawn_child",
            "session_cancel_child",
            "session_mailbox_list",
        ] {
            assert!(names.contains(&expected), "missing tool: {expected}");
        }
        for hidden in [
            "capability_list",
            "capability_describe",
            "capability_request",
            "planning_pack",
            "test_selector",
            "contract_guard",
            "skills_search",
            "spec_write",
            "knowledge_pdf_ingest",
            "knowledge_office_ingest",
            "knowledge_data_ingest",
            "knowledge_search",
            "docs_build",
            "db_query",
            "db_performance_audit",
            "ssh_exec",
        ] {
            assert!(!names.contains(&hidden), "tool should be hidden: {hidden}");
        }
    }

    #[test]
    fn capability_describe_returns_category_selection_cues() {
        let (d, _) = mk("t1", "agent:1");
        let line = r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"capability_describe","arguments":{"id":"docs_web"}}}"#;
        let resp = d.handle(parse_request(line).unwrap()).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        let value: serde_json::Value = serde_json::from_str(text).unwrap();

        assert_eq!(value["id"], "docs_web");
        assert_eq!(value["status"], "available_on_request");
        assert!(value["mentions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|mention| mention == "docs"));
        assert!(value["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool == "crawl4ai__*"));
    }

    #[test]
    fn capability_request_expands_tools_list_for_category() {
        let (d, _) = mk("t1", "agent:1");
        let request = r#"{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"capability_request","arguments":{"id":"db","reason":"Need schema inspection"}}}"#;
        let resp = d.handle(parse_request(request).unwrap()).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        let value: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(value["id"], "db");
        assert_eq!(value["status"], "loaded");

        let list_line = r#"{"jsonrpc":"2.0","id":6,"method":"tools/list"}"#;
        let resp = d.handle(parse_request(list_line).unwrap()).unwrap();
        let tools = resp["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(names.contains(&"db_query"));
        assert!(names.contains(&"db_schema"));
        assert!(!names.contains(&"ssh_exec"));
    }

    #[test]
    fn capability_request_expands_document_extract_tools() {
        let (d, _) = mk("t1", "agent:1");
        let request = r#"{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"capability_request","arguments":{"id":"document_extract","reason":"Need to ingest a DOCX"}}}"#;
        let resp = d.handle(parse_request(request).unwrap()).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        let value: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(value["id"], "document_extract");

        let list_line = r#"{"jsonrpc":"2.0","id":9,"method":"tools/list"}"#;
        let resp = d.handle(parse_request(list_line).unwrap()).unwrap();
        let tools = resp["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(names.contains(&"knowledge_pdf_ingest"));
        assert!(names.contains(&"knowledge_office_ingest"));
        assert!(names.contains(&"knowledge_data_ingest"));
        assert!(names.contains(&"knowledge_search"));
    }

    #[test]
    fn tools_load_and_unload_emit_list_changed_and_change_list() {
        let (d, _) = mk("t1", "agent:1");
        let load = r#"{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"tools_load","arguments":{"groups":["db"]}}}"#;
        let resp = d.handle(parse_request(load).unwrap()).unwrap();
        assert_ne!(resp["result"]["isError"], true);
        let notifications = d.drain_notifications();
        assert_eq!(notifications.len(), 1);
        assert_eq!(
            notifications[0]["method"],
            "notifications/tools/list_changed"
        );

        let list_line = r#"{"jsonrpc":"2.0","id":11,"method":"tools/list"}"#;
        let resp = d.handle(parse_request(list_line).unwrap()).unwrap();
        let tools = resp["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(names.contains(&"db_query"));
        assert!(names.contains(&"db_export_table"));

        let unload = r#"{"jsonrpc":"2.0","id":12,"method":"tools/call","params":{"name":"tools_unload","arguments":{"groups":["db"]}}}"#;
        let resp = d.handle(parse_request(unload).unwrap()).unwrap();
        assert_ne!(resp["result"]["isError"], true);
        assert_eq!(d.drain_notifications().len(), 1);

        let resp = d.handle(parse_request(list_line).unwrap()).unwrap();
        let tools = resp["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(!names.contains(&"db_query"));
    }

    #[test]
    fn tools_search_finds_db_export_by_natural_language() {
        let (d, _) = mk("t1", "agent:1");
        let search = r#"{"jsonrpc":"2.0","id":13,"method":"tools/call","params":{"name":"tools_search","arguments":{"query":"export csv de una tabla"}}}"#;
        let resp = d.handle(parse_request(search).unwrap()).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        let value: serde_json::Value = serde_json::from_str(text).unwrap();
        assert!(value["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| { tool["name"] == "db_export_table" && tool["group"] == "db" }));
    }

    #[test]
    fn tools_search_finds_planning_group_without_loading_it() {
        let (d, _) = mk("t1", "agent:1");
        let search = r#"{"jsonrpc":"2.0","id":23,"method":"tools/call","params":{"name":"tools_search","arguments":{"query":"planning guardrails"}}}"#;
        let resp = d.handle(parse_request(search).unwrap()).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        let value: serde_json::Value = serde_json::from_str(text).unwrap();
        let groups = value["groups"].as_array().unwrap();
        let tools = value["tools"].as_array().unwrap();
        assert!(groups.iter().any(|group| group["group"] == "planning"));
        let planning_pack = tools
            .iter()
            .find(|tool| tool["name"] == "planning_pack")
            .expect("planning_pack should be discoverable");
        assert_eq!(planning_pack["group"], "planning");
        assert_eq!(planning_pack["loaded"], false);
    }

    #[test]
    fn planning_pack_auto_loads_and_returns_checks() {
        let (d, _) = mk("t1", "agent:1");
        let call = r#"{"jsonrpc":"2.0","id":24,"method":"tools/call","params":{"name":"planning_pack","arguments":{"objective":"Fix frontend/backend API contract bug","files":["backend/crates/harness-server/src/routes/sessions.rs","frontend/src/lib/api/client.ts"]}}}"#;
        let resp = d.handle(parse_request(call).unwrap()).unwrap();
        assert_ne!(resp["result"]["isError"], true);
        let content = resp["result"]["content"].as_array().unwrap();
        assert!(content.iter().any(|item| item["text"]
            .as_str()
            .is_some_and(|text| text.contains("auto-loaded tool group `planning`"))));
        let payload_text = content
            .iter()
            .filter_map(|item| item["text"].as_str())
            .find(|text| text.contains("recommended_tool_groups"))
            .unwrap();
        let value: serde_json::Value = serde_json::from_str(payload_text).unwrap();
        assert!(value["recommended_tool_groups"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item == "repo"));
        assert!(value["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["command"] == "just gen-types"));
    }

    #[test]
    fn truncation_limits_by_lines_with_head_mode_metadata() {
        let text = (0..8)
            .map(|idx| format!("line-{idx}"))
            .collect::<Vec<_>>()
            .join("\n");
        let result = truncate_text_result(
            &text,
            TruncationPolicy {
                mode: TruncationMode::Head,
                max_lines: 3,
                max_bytes: 1_000,
            },
            false,
        );

        assert!(result.changed);
        assert!(result.text.starts_with("line-0\nline-1\nline-2"));
        assert!(!result.text.contains("line-7"));
        assert!(result
            .text
            .ends_with("[truncated: 5 lines / 35 bytes omitted]"));
    }

    #[test]
    fn truncation_limits_by_bytes_utf8_safe() {
        let text = "áéíóú".repeat(20);
        let result = truncate_text_result(
            &text,
            TruncationPolicy {
                mode: TruncationMode::Head,
                max_lines: 100,
                max_bytes: 21,
            },
            true,
        );

        assert!(result.changed);
        assert!(std::str::from_utf8(result.text.as_bytes()).is_ok());
        assert!(result.text.contains("[truncated: 0 lines /"));
    }

    #[test]
    fn truncation_tail_mode_keeps_execution_end() {
        let text = (0..8)
            .map(|idx| format!("line-{idx}"))
            .collect::<Vec<_>>()
            .join("\n");
        let result = truncate_text_result(
            &text,
            TruncationPolicy {
                mode: TruncationMode::Tail,
                max_lines: 3,
                max_bytes: 1_000,
            },
            false,
        );

        assert!(result.changed);
        assert!(!result.text.contains("line-0"));
        assert!(result.text.starts_with("[truncated: 5 lines /"));
        assert!(result.text.ends_with("line-5\nline-6\nline-7"));
    }

    #[test]
    fn dispatcher_truncates_tool_result_text_after_execution() {
        let cwd = tempfile::tempdir().unwrap();
        std::fs::write(
            cwd.path().join("big.txt"),
            "x".repeat(DEFAULT_TRUNCATION_BYTES + 4096),
        )
        .unwrap();
        let (d, _) = mk_with_cwd("t1", "agent:1", cwd.path().to_path_buf());
        let call = r#"{"jsonrpc":"2.0","id":31,"method":"tools/call","params":{"name":"repo_read_file","arguments":{"path":"big.txt"}}}"#;

        let resp = d.handle(parse_request(call).unwrap()).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();

        assert!(text.contains("[truncated:"));
        assert!(text.len() < DEFAULT_TRUNCATION_BYTES + 512);
    }

    #[test]
    fn dispatcher_does_not_byte_truncate_structured_json_results() {
        let (d, _) = mk("t1", "agent:1");
        let payload = json!({
            "tables": (0..400)
                .map(|idx| json!({
                    "name": format!("table_{idx}"),
                    "columns": ["alpha", "beta", "gamma", "delta"],
                    "description": "x".repeat(1024)
                }))
                .collect::<Vec<_>>()
        });
        let result = d.truncate_tool_result("db_schema", wrap_text(&payload));
        let text = result["content"][0]["text"].as_str().unwrap();

        assert!(text.len() > DEFAULT_TRUNCATION_BYTES);
        serde_json::from_str::<Value>(text).unwrap();
    }

    #[test]
    fn dispatcher_byte_truncates_free_text_results() {
        let (d, _) = mk("t1", "agent:1");
        let result = d.truncate_tool_result(
            "ssh_exec",
            wrap_text(&Value::String("x".repeat(DEFAULT_TRUNCATION_BYTES + 4096))),
        );
        let text = result["content"][0]["text"].as_str().unwrap();

        assert!(text.starts_with("[truncated:"));
        assert!(text.len() < DEFAULT_TRUNCATION_BYTES + 512);
    }

    #[test]
    fn active_groups_are_seeded_from_session_meta() {
        let home = tmp_home();
        let meta_dir = home.join("profiles/default/sessions/sid-1");
        std::fs::create_dir_all(&meta_dir).unwrap();
        std::fs::write(
            meta_dir.join("meta.json"),
            serde_json::to_vec(&json!({
                "id": "sid-1",
                "kind": "codex",
                "thread_id": "t1",
                "cwd": ".",
                "pid": 0,
                "status": "exited",
                "started_at": 0,
                "loaded_capabilities": {
                    "mcp_servers": [],
                    "skills": [],
                    "tool_groups": ["ssh"]
                },
                "root_session_id": "sid-1",
                "has_transcript": false
            }))
            .unwrap(),
        )
        .unwrap();
        let d = Dispatcher::new_with_server(
            home,
            "t1".to_string(),
            "agent:1".to_string(),
            Some("sid-1".to_string()),
            "default".into(),
            None,
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            None,
            None,
            None,
            Vec::new(),
            None,
        )
        .unwrap();
        let list_line = r#"{"jsonrpc":"2.0","id":15,"method":"tools/list"}"#;
        let resp = d.handle(parse_request(list_line).unwrap()).unwrap();
        let tools = resp["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(names.contains(&"ssh_hosts"));
        assert!(names.contains(&"ssh_exec"));
        assert!(!names.contains(&"db_query"));
    }

    #[test]
    fn load_and_unload_validate_all_groups_before_mutating() {
        let (d, _) = mk("t1", "agent:1");

        let bad_load = r#"{"jsonrpc":"2.0","id":16,"method":"tools/call","params":{"name":"tools_load","arguments":{"groups":["db","missing"]}}}"#;
        let resp = d.handle(parse_request(bad_load).unwrap()).unwrap();
        assert_eq!(resp["result"]["isError"], true);
        assert!(d.drain_notifications().is_empty());

        let list_line = r#"{"jsonrpc":"2.0","id":17,"method":"tools/list"}"#;
        let resp = d.handle(parse_request(list_line).unwrap()).unwrap();
        let tools = resp["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(!names.contains(&"db_query"));

        let load = r#"{"jsonrpc":"2.0","id":18,"method":"tools/call","params":{"name":"tools_load","arguments":{"groups":["db","ssh"]}}}"#;
        let resp = d.handle(parse_request(load).unwrap()).unwrap();
        assert_ne!(resp["result"]["isError"], true);
        assert_eq!(d.drain_notifications().len(), 1);

        let bad_unload = r#"{"jsonrpc":"2.0","id":19,"method":"tools/call","params":{"name":"tools_unload","arguments":{"groups":["db","missing"]}}}"#;
        let resp = d.handle(parse_request(bad_unload).unwrap()).unwrap();
        assert_eq!(resp["result"]["isError"], true);
        assert!(d.drain_notifications().is_empty());

        let resp = d.handle(parse_request(list_line).unwrap()).unwrap();
        let tools = resp["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(names.contains(&"db_query"));
        assert!(names.contains(&"ssh_hosts"));
    }

    #[test]
    fn auto_load_fallback_wraps_result_as_content_item() {
        let (d, _) = mk("t1", "agent:1");
        let result = d.apply_auto_load_note(json!({"ok": true}), Some("db".to_string()));

        assert!(result.get("result").is_none());
        let content = result["content"].as_array().unwrap();
        assert_eq!(content.len(), 2);
        assert!(content[0]["text"]
            .as_str()
            .unwrap()
            .contains("auto-loaded tool group `db`"));
        assert_eq!(content[1]["type"], "text");
        assert!(content[1]["text"].as_str().unwrap().contains("\"ok\":true"));
    }

    #[test]
    fn unloaded_group_tool_auto_loads_before_execution() {
        let (d, _) = mk("t1", "agent:1");
        let call = r#"{"jsonrpc":"2.0","id":14,"method":"tools/call","params":{"name":"ssh_hosts","arguments":{}}}"#;
        let resp = d.handle(parse_request(call).unwrap()).unwrap();
        assert_ne!(resp["result"]["isError"], true);
        let content = resp["result"]["content"].as_array().unwrap();
        let note = content
            .iter()
            .filter_map(|item| item["text"].as_str())
            .find(|text| text.contains("auto-loaded tool group `ssh`"))
            .unwrap();
        assert!(note.contains("Refresh tools/list"));
        assert_eq!(d.drain_notifications().len(), 1);

        let list_line = r#"{"jsonrpc":"2.0","id":15,"method":"tools/list"}"#;
        let resp = d.handle(parse_request(list_line).unwrap()).unwrap();
        let tools = resp["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(names.contains(&"ssh_hosts"));
    }

    #[test]
    fn capability_request_docs_web_requires_smart_loaded_upstream() {
        let (d, _) = mk("t1", "agent:1");
        let request = r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"capability_request","arguments":{"id":"docs_web","reason":"Need external docs"}}}"#;
        let resp = d.handle(parse_request(request).unwrap()).unwrap();

        assert_eq!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("docs_web is not hot-loaded"));
    }

    #[test]
    fn crawl4ai_gateway_lists_when_seeded_mcp_server_is_active() {
        let home = tmp_home();
        let meta_dir = home.join("profiles/default/sessions/sid-crawl");
        std::fs::create_dir_all(&meta_dir).unwrap();
        std::fs::write(
            meta_dir.join("meta.json"),
            serde_json::to_vec(&json!({
                "id": "sid-crawl",
                "kind": "codex",
                "thread_id": "t1",
                "cwd": ".",
                "pid": 0,
                "status": "exited",
                "started_at": 0,
                "loaded_capabilities": {
                    "mcp_servers": ["crawl4ai"],
                    "skills": [],
                    "tool_groups": []
                },
                "root_session_id": "sid-crawl",
                "has_transcript": false
            }))
            .unwrap(),
        )
        .unwrap();

        let upstream = home.join("mock-crawl4ai.sh");
        std::fs::write(
            &upstream,
            r#"#!/bin/sh
init='{"jsonrpc":"2.0","id":1,"result":{}}'
tools='{"jsonrpc":"2.0","id":2,"result":{"tools":[{"name":"crawl","description":"Crawl docs","inputSchema":{"type":"object"}}]}}'
printf 'Content-Length: %s\r\n\r\n%s' "${#init}" "$init"
printf 'Content-Length: %s\r\n\r\n%s' "${#tools}" "$tools"
sleep 5
"#,
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&upstream).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&upstream, perms).unwrap();
        }

        let upstream_config = home.join("upstreams.json");
        std::fs::write(
            &upstream_config,
            serde_json::to_vec(&json!([{
                "name": "crawl4ai",
                "command": upstream,
                "args": []
            }]))
            .unwrap(),
        )
        .unwrap();

        let d = Dispatcher::new_with_server(
            home,
            "t1".to_string(),
            "agent:1".to_string(),
            Some("sid-crawl".to_string()),
            "default".into(),
            None,
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            None,
            None,
            None,
            Vec::new(),
            Some(upstream_config),
        )
        .unwrap();

        let list_line = r#"{"jsonrpc":"2.0","id":15,"method":"tools/list"}"#;
        let resp = d.handle(parse_request(list_line).unwrap()).unwrap();
        let tools = resp["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(
            names.contains(&"crawl4ai__crawl"),
            "expected crawl4ai__crawl in tools/list, got: {names:?}"
        );
    }

    #[test]
    fn task_list_default_thread_empty() {
        let (d, _) = mk("t1", "agent:1");
        let line = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"task_list","arguments":{}}}"#;
        let req = parse_request(line).unwrap();
        let resp = d.handle(req).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert_eq!(text, "[]");
    }

    #[test]
    fn task_create_with_brief_persists_worker_contract() {
        let (d, _home) = mk_with_role("t-brief", "agent:planner", Some("planner"));
        let create_line = r#"{
            "jsonrpc":"2.0",
            "id":31,
            "method":"tools/call",
            "params":{
                "name":"task_create",
                "arguments":{
                    "title":"Wire task brief",
                    "brief":{
                        "objetivo":"Permitir handoff claro al worker.",
                        "contexto":"MCP task_create debe conservar memoria entre sesiones.",
                        "tarea":["Crear task con brief","Recuperar task con task_get"],
                        "reglas":["No romper","Cambios mínimos","Seguir estilo existente","Agregar test"],
                        "resultado_esperado":"El worker puede leer el contrato completo."
                    },
                    "labels":["backend","brief"],
                    "spec_refs":[{"section":"requirements","version":1}]
                }
            }
        }"#;
        let resp = d.handle(parse_request(create_line).unwrap()).unwrap();
        assert_ne!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        let created: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(created["title"], "Wire task brief");
        assert_eq!(
            created["brief"]["objective"],
            "Permitir handoff claro al worker."
        );
        assert_eq!(
            created["brief"]["context"],
            "MCP task_create debe conservar memoria entre sesiones."
        );
        assert_eq!(created["brief"]["tasks"][0], "Crear task con brief");
        assert_eq!(created["spec_refs"][0]["section"], "requirements");
        assert_eq!(created["spec_refs"][0]["version"], 1);
        assert_eq!(created["brief"]["tasks"][1], "Recuperar task con task_get");
        assert_eq!(created["brief"]["rules"][0], "No romper");
        assert_eq!(
            created["brief"]["expected_result"],
            "El worker puede leer el contrato completo."
        );
        assert_eq!(created["acceptance"]["checks"].as_array().unwrap().len(), 0);

        let task_id = created["id"].as_str().unwrap();
        let get_line = format!(
            r#"{{"jsonrpc":"2.0","id":32,"method":"tools/call","params":{{"name":"task_get","arguments":{{"task_id":"{task_id}"}}}}}}"#
        );
        let resp = d.handle(parse_request(&get_line).unwrap()).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        let fetched: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(fetched["id"], task_id);
        assert_eq!(fetched["brief"], created["brief"]);
    }

    #[test]
    fn task_create_rejects_incomplete_structured_brief() {
        let (d, _home) = mk_with_role("t-brief-invalid", "agent:planner", Some("planner"));
        let create_line = r#"{
            "jsonrpc":"2.0",
            "id":33,
            "method":"tools/call",
            "params":{
                "name":"task_create",
                "arguments":{
                    "title":"Vague task",
                    "brief":{
                        "objetivo":"Arreglar cosas"
                    }
                }
            }
        }"#;
        let resp = d.handle(parse_request(create_line).unwrap()).unwrap();
        assert_eq!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("brief incomplete"));
        assert!(text.contains("contexto"));
        assert!(text.contains("tarea"));
        assert!(text.contains("reglas"));
        assert!(text.contains("resultado_esperado"));
        assert!(text.contains("Retry task_create with brief using this exact shape"));
        assert!(text.contains("\"objetivo\""));
        assert!(text.contains("\"contexto\""));
    }

    #[test]
    fn task_create_accepts_legacy_string_brief() {
        let (d, _home) = mk_with_role("t-brief-string", "agent:planner", Some("planner"));
        let create_line = r#"{
            "jsonrpc":"2.0",
            "id":34,
            "method":"tools/call",
            "params":{
                "name":"task_create",
                "arguments":{
                    "title":"Legacy brief",
                    "brief":"Plain text brief from an older caller."
                }
            }
        }"#;
        let resp = d.handle(parse_request(create_line).unwrap()).unwrap();
        assert_ne!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        let created: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(
            created["brief"]["objective"],
            "Plain text brief from an older caller."
        );
        assert_eq!(created["acceptance"]["checks"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn task_propose_creates_proposed_task() {
        let (d, _home) = mk_with_role("t-propose", "agent:worker", Some("worker"));
        let line = r#"{
            "jsonrpc":"2.0",
            "id":35,
            "method":"tools/call",
            "params":{
                "name":"task_propose",
                "arguments":{
                    "title":"Suggested follow-up",
                    "brief":"Worker found more work."
                }
            }
        }"#;
        let resp = d.handle(parse_request(line).unwrap()).unwrap();
        assert_ne!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        let created: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(created["status"], "proposed");
    }

    #[test]
    fn task_create_rejects_worker_role_with_hint() {
        let (d, _home) = mk_with_role("t-worker-create", "agent:worker", Some("worker"));
        let line = r#"{
            "jsonrpc":"2.0",
            "id":36,
            "method":"tools/call",
            "params":{
                "name":"task_create",
                "arguments":{"title":"Should be proposed"}
            }
        }"#;
        let resp = d.handle(parse_request(line).unwrap()).unwrap();
        assert_eq!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert_eq!(text, "error: denied_by_role: task_create; usa task_propose");
    }

    #[test]
    fn offline_capability_matrix_rejects_generator_spec_write() {
        let (d, _home) = mk_with_role("t-generator-spec", "agent:generator", Some("generator"));
        let line = r##"{
            "jsonrpc":"2.0",
            "id":39,
            "method":"tools/call",
            "params":{
                "name":"spec_write",
                "arguments":{"thread_id":"t-generator-spec","content":"# Should not write"}
            }
        }"##;
        let resp = d.handle(parse_request(line).unwrap()).unwrap();
        assert_eq!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert_eq!(text, "error: denied_by_role: spec_write");
    }

    #[test]
    fn invariant_worker_cannot_set_spec_section() {
        let (d, _home) = mk_with_role("t-worker-spec", "agent:worker", Some("worker"));
        let line = r##"{
            "jsonrpc":"2.0",
            "id":139,
            "method":"tools/call",
            "params":{
                "name":"spec_set_section",
                "arguments":{
                    "thread_id":"t-worker-spec",
                    "section":"requirements",
                    "content":"Should not write",
                    "spec_version_required":0
                }
            }
        }"##;
        let resp = d.handle(parse_request(line).unwrap()).unwrap();
        assert_eq!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert_eq!(text, "error: denied_by_role: spec_set_section");
    }

    #[test]
    fn invariant_planner_cannot_claim_task() {
        let (d, _home) = mk_with_role("t-planner-claim", "agent:planner", Some("planner"));
        let line = r#"{
            "jsonrpc":"2.0",
            "id":140,
            "method":"tools/call",
            "params":{
                "name":"task_claim",
                "arguments":{"task_id":"T-0001","agent_id":"agent:planner"}
            }
        }"#;
        let resp = d.handle(parse_request(line).unwrap()).unwrap();
        assert_eq!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert_eq!(text, "error: denied_by_role: task_claim");
    }

    #[test]
    fn offline_capability_matrix_rejects_evaluator_sensitive_tool() {
        let (d, _home) = mk_with_role("t-evaluator-db", "agent:evaluator", Some("evaluator"));
        let line = r#"{
            "jsonrpc":"2.0",
            "id":40,
            "method":"tools/call",
            "params":{
                "name":"db_query",
                "arguments":{"sql":"select 1"}
            }
        }"#;
        let resp = d.handle(parse_request(line).unwrap()).unwrap();
        assert_eq!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert_eq!(text, "error: denied_by_role: db_query");
    }

    #[test]
    fn offline_capability_matrix_rejects_unknown_roles_for_sensitive_tools() {
        for role in ["super-planner-worker", "planner-worker", "not-orchestrator"] {
            let (d, _home) = mk_with_role("t-stuffed", "agent:worker", Some(role));
            let line = r#"{
                "jsonrpc":"2.0",
                "id":38,
                "method":"tools/call",
                "params":{
                    "name":"task_create",
                    "arguments":{"title":"Sneaky create"}
                }
            }"#;
            let resp = d.handle(parse_request(line).unwrap()).unwrap();
            assert_eq!(resp["result"]["isError"], true);
            let text = resp["result"]["content"][0]["text"].as_str().unwrap();
            assert_eq!(text, "error: denied_by_role: task_create; usa task_propose");
        }
    }

    #[test]
    fn task_create_allows_planner_role() {
        let (d, home) = mk_with_role("t-planner-create", "agent:planner", Some("planner"));
        let line = r#"{
                "jsonrpc":"2.0",
                "id":37,
                "method":"tools/call",
                "params":{
                    "name":"task_create",
                    "arguments":{"title":"Allowed create"}
                }
            }"#;
        let resp = d.handle(parse_request(line).unwrap()).unwrap();
        assert_ne!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        let created: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(created["status"], "queued");
        let artifact_dir = created["artifact_dir"].as_str().expect("artifact_dir");
        assert!(artifact_dir.ends_with("threads/t-planner-create/artifacts/T-0001"));
        assert!(std::path::Path::new(artifact_dir).is_dir());
        assert!(std::path::Path::new(artifact_dir).starts_with(&home));
    }

    #[test]
    fn offline_capability_matrix_rejects_missing_role_for_sensitive_tools() {
        let (d, _home) = mk_with_role("t-none-create", "agent:legacy", None);
        let line = r#"{
            "jsonrpc":"2.0",
            "id":41,
            "method":"tools/call",
            "params":{
                "name":"task_create",
                "arguments":{"title":"Missing role create"}
            }
        }"#;
        let resp = d.handle(parse_request(line).unwrap()).unwrap();
        assert_eq!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert_eq!(text, "error: denied_by_role: task_create; usa task_propose");
    }

    #[test]
    fn offline_capability_matrix_allows_missing_role_for_read_only_tools() {
        let (d, _home) = mk_with_role("t-none-list", "agent:legacy", None);
        let line = r#"{
            "jsonrpc":"2.0",
            "id":42,
            "method":"tools/call",
            "params":{
                "name":"task_list",
                "arguments":{}
            }
        }"#;
        let resp = d.handle(parse_request(line).unwrap()).unwrap();
        assert_ne!(resp["result"]["isError"], true);
    }

    #[test]
    fn offline_policy_file_can_deny_read_only_tool() {
        let (d, _home) = mk_with_role_and_policy(
            "t-policy-deny",
            "agent:planner",
            Some("planner"),
            r#"
[[rules]]
tool = "task_list"
decision = "deny"
"#,
        );

        let msg = d.check_tool_policy("task_list", &json!({})).unwrap();
        assert_eq!(msg, "denied_by_role: task_list");
    }

    #[test]
    fn offline_policy_file_can_allow_sensitive_tool_for_trusted_role() {
        let (d, _home) = mk_with_role_and_policy(
            "t-policy-allow",
            "agent:generator",
            Some("generator"),
            r#"
[[rules]]
tool = "task_create"
decision = "allow"
"#,
        );

        assert!(d.check_tool_policy("task_create", &json!({})).is_none());
    }

    #[test]
    fn offline_corrupt_policy_fails_closed_for_sensitive_tools() {
        let (d, _home) = mk_with_role_and_policy(
            "t-policy-corrupt",
            "agent:planner",
            Some("planner"),
            "this is not toml =",
        );

        let msg = d.check_tool_policy("task_create", &json!({})).unwrap();
        assert!(msg.contains("local policy failed to load"));
        assert!(d.check_tool_policy("task_list", &json!({})).is_none());
    }

    #[test]
    fn approval_check_failure_message_includes_timeout_budget() {
        let home = tmp_home();
        let d = Dispatcher::new_with_server(
            home,
            "t-policy-timeout".to_string(),
            "agent:planner".to_string(),
            None,
            "default".into(),
            Some("http://127.0.0.1:9".into()),
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            None,
            Some("planner".into()),
            None,
            Vec::new(),
            None,
        )
        .unwrap();

        let msg = d
            .check_tool_policy_with_timeout("task_list", &json!({}), Duration::from_millis(50))
            .unwrap();

        assert_eq!(format_duration(POLICY_CHECK_TIMEOUT), "8s");
        assert!(msg.contains("approval check failed for task_list within 50ms"));
        assert!(msg.contains("failing closed"));
    }

    #[test]
    fn approval_check_times_out_against_hanging_endpoint_when_tcp_available() {
        let listener = match std::net::TcpListener::bind("127.0.0.1:0") {
            Ok(listener) => listener,
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => return,
            Err(e) => panic!("failed to bind test listener: {e}"),
        };
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (_stream, _) = listener.accept().unwrap();
            std::thread::sleep(Duration::from_millis(900));
        });
        let home = tmp_home();
        let d = Dispatcher::new_with_server(
            home,
            "t-policy-hanging".to_string(),
            "agent:planner".to_string(),
            None,
            "default".into(),
            Some(format!("http://{addr}")),
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            None,
            Some("planner".into()),
            None,
            Vec::new(),
            None,
        )
        .unwrap();

        let msg = d
            .check_tool_policy_with_timeout("task_list", &json!({}), Duration::from_millis(150))
            .unwrap();

        assert!(msg.contains("approval check failed for task_list within 150ms"));
        assert!(msg.contains("failing closed"));
        handle.join().unwrap();
    }

    #[test]
    fn unknown_tool_returns_structured_error() {
        let (d, _) = mk("t1", "agent:1");
        let line = r#"{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"bogus","arguments":{}}}"#;
        let resp = d.handle(parse_request(line).unwrap()).unwrap();
        // We use JSON-RPC error for unknown tools (protocol-level).
        assert_eq!(resp["error"]["code"], -32601);
    }

    #[test]
    fn missing_args_returns_is_error_payload() {
        let (d, _) = mk("t1", "agent:1");
        let line = r#"{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"task_get","arguments":{}}}"#;
        let resp = d.handle(parse_request(line).unwrap()).unwrap();
        assert_eq!(resp["result"]["isError"], true);
    }

    #[test]
    fn spec_read_missing_returns_empty_string() {
        let (d, _) = mk("t1", "agent:1");
        let line = r#"{"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"spec_read","arguments":{"thread_id":"t1"}}}"#;
        let resp = d.handle(parse_request(line).unwrap()).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"content\":\"\""));
    }

    #[test]
    fn spec_write_then_spec_read_returns_written_content() {
        let (d, _) = mk_with_role("t1", "agent:planner", Some("planner"));
        let write_line = r##"{"jsonrpc":"2.0","id":12,"method":"tools/call","params":{"name":"spec_write","arguments":{"thread_id":"t1","content":"# Spec\nBody"}}}"##;
        let resp = d.handle(parse_request(write_line).unwrap()).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"ok\":true"));
        assert!(text.contains("\"created\":true"));

        let read_line = r#"{"jsonrpc":"2.0","id":13,"method":"tools/call","params":{"name":"spec_read","arguments":{"thread_id":"t1"}}}"#;
        let resp = d.handle(parse_request(read_line).unwrap()).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("# Spec\\nBody"));
    }

    #[test]
    fn spec_set_section_versions_and_rejects_stale_writes() {
        let (d, _) = mk_with_role("t1", "agent:planner", Some("planner"));
        let set_line = r##"{"jsonrpc":"2.0","id":14,"method":"tools/call","params":{"name":"spec_set_section","arguments":{"thread_id":"t1","section":"requirements","content":"Must pass","spec_version_required":0}}}"##;
        let resp = d.handle(parse_request(set_line).unwrap()).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("\"version\":1"));

        let stale_line = r##"{"jsonrpc":"2.0","id":15,"method":"tools/call","params":{"name":"spec_set_section","arguments":{"thread_id":"t1","section":"requirements","content":"Stale","spec_version_required":0}}}"##;
        let resp = d.handle(parse_request(stale_line).unwrap()).unwrap();
        assert_eq!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("spec_version_mismatch"));

        let read_line = r#"{"jsonrpc":"2.0","id":16,"method":"tools/call","params":{"name":"spec_read","arguments":{"thread_id":"t1"}}}"#;
        let resp = d.handle(parse_request(read_line).unwrap()).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("harness:section requirements"));
        assert!(text.contains("\"version\":1"));
    }

    #[test]
    fn repo_analyze_reports_stack_and_codebase_memory_status() {
        let cwd = tmp_home();
        let (d, _home) = mk_with_cwd("t-repo", "agent:planner", cwd.clone());
        std::fs::write(cwd.join("Cargo.toml"), "[package]\nname = \"demo\"\n").unwrap();
        std::fs::write(
            cwd.join("package.json"),
            r#"{"scripts":{"test":"vitest"},"devDependencies":{"vite":"latest"}}"#,
        )
        .unwrap();
        std::fs::write(cwd.join("pnpm-lock.yaml"), "").unwrap();
        let line = r#"{"jsonrpc":"2.0","id":41,"method":"tools/call","params":{"name":"repo_analyze","arguments":{}}}"#;
        let resp = d.handle(parse_request(line).unwrap()).unwrap();
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        let value: serde_json::Value = serde_json::from_str(text).unwrap();
        let stack = value["stack"].as_array().unwrap();
        assert!(stack.iter().any(|v| v == "rust"));
        assert!(stack.iter().any(|v| v == "node"));
        assert_eq!(value["package_manager"], "pnpm");
        assert!(value["codebase_memory"]["recommended"].as_bool().unwrap());
    }

    #[test]
    fn repo_read_file_rejects_parent_escape() {
        let (d, _) = mk("t-repo-safe", "agent:planner");
        let line = r#"{"jsonrpc":"2.0","id":42,"method":"tools/call","params":{"name":"repo_read_file","arguments":{"path":"../secret.txt"}}}"#;
        let resp = d.handle(parse_request(line).unwrap()).unwrap();
        assert_eq!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("must not escape"));
    }

    #[test]
    fn repo_find_searches_names_and_content_with_limits() {
        let cwd = tmp_home();
        let (d, _home) = mk_with_cwd("t-repo-find", "agent:planner", cwd.clone());
        std::fs::create_dir_all(cwd.join("src")).unwrap();
        std::fs::write(cwd.join("src/session.rs"), "pub fn login_session() {}\n").unwrap();
        std::fs::write(cwd.join("src/account.rs"), "pub fn account() {}\n").unwrap();

        let line = r#"{"jsonrpc":"2.0","id":49,"method":"tools/call","params":{"name":"repo_find","arguments":{"path":"src","content_contains":"login","extensions":["rs"],"limit":5}}}"#;
        let resp = d.handle(parse_request(line).unwrap()).unwrap();
        assert_ne!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        let value: serde_json::Value = serde_json::from_str(text).unwrap();
        let matches = value["matches"].as_array().unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0]["path"], "session.rs");
        assert_eq!(matches[0]["content_matched"], true);
    }

    #[test]
    fn repo_write_file_allows_task_scoped_path() {
        let cwd = tmp_home();
        let d = mk_scoped_writer(cwd.clone(), vec!["src"], vec![]);
        let line = r#"{"jsonrpc":"2.0","id":46,"method":"tools/call","params":{"name":"repo_write_file","arguments":{"path":"src/lib.rs","content":"pub fn ok() {}\n"}}}"#;
        let resp = d.handle(parse_request(line).unwrap()).unwrap();
        assert_ne!(resp["result"]["isError"], true);
        assert_eq!(
            std::fs::read_to_string(cwd.join("src/lib.rs")).unwrap(),
            "pub fn ok() {}\n"
        );
    }

    #[test]
    fn repo_write_file_denies_outside_task_scope() {
        let cwd = tmp_home();
        let d = mk_scoped_writer(cwd, vec!["src"], vec![]);
        let line = r#"{"jsonrpc":"2.0","id":47,"method":"tools/call","params":{"name":"repo_write_file","arguments":{"path":"README.md","content":"nope"}}}"#;
        let resp = d.handle(parse_request(line).unwrap()).unwrap();
        assert_eq!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("outside task write_paths"));
    }

    #[test]
    fn repo_write_file_denies_forbidden_subpath() {
        let cwd = tmp_home();
        let d = mk_scoped_writer(cwd, vec!["src"], vec!["src/secrets"]);
        let line = r#"{"jsonrpc":"2.0","id":48,"method":"tools/call","params":{"name":"repo_write_file","arguments":{"path":"src/secrets/token.txt","content":"nope"}}}"#;
        let resp = d.handle(parse_request(line).unwrap()).unwrap();
        assert_eq!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("forbidden by task scope"));
    }

    #[test]
    fn spec_read_rejects_invalid_thread_id() {
        let (d, _) = mk("t-spec-safe", "agent:planner");
        let line = r#"{"jsonrpc":"2.0","id":43,"method":"tools/call","params":{"name":"spec_read","arguments":{"thread_id":"../escape"}}}"#;
        let resp = d.handle(parse_request(line).unwrap()).unwrap();
        assert_eq!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("thread_id"));
    }

    #[test]
    fn task_tools_reject_invalid_path_ids() {
        let (d, _) = mk("t-task-safe", "agent:planner");
        let bad_thread = r#"{"jsonrpc":"2.0","id":44,"method":"tools/call","params":{"name":"task_list","arguments":{"thread_id":"../escape"}}}"#;
        let resp = d.handle(parse_request(bad_thread).unwrap()).unwrap();
        assert_eq!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("thread_id"));

        let bad_task = r#"{"jsonrpc":"2.0","id":45,"method":"tools/call","params":{"name":"task_get","arguments":{"task_id":"../T-0001"}}}"#;
        let resp = d.handle(parse_request(bad_task).unwrap()).unwrap();
        assert_eq!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("task_id"));
    }

    #[test]
    fn repo_read_file_truncates_on_utf8_boundary() {
        let cwd = tmp_home();
        let (d, _home) = mk_with_cwd("t-repo-utf8", "agent:planner", cwd.clone());
        std::fs::write(cwd.join("note.txt"), "aéz").unwrap();

        let line = r#"{"jsonrpc":"2.0","id":43,"method":"tools/call","params":{"name":"repo_read_file","arguments":{"path":"note.txt","max_bytes":2}}}"#;
        let resp = d.handle(parse_request(line).unwrap()).unwrap();
        assert_ne!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().unwrap();
        let value: serde_json::Value = serde_json::from_str(text).unwrap();
        assert_eq!(value["content"], "a");
        assert_eq!(value["truncated"], true);
    }
}
