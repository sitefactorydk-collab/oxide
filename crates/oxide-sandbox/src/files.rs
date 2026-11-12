//! The sandbox's file tools: the reference filesystem MCP server, run from its Docker image
//! with the chat's workspace mounted at `/workspace` (and nothing else).
//!
//! It is one server per chat, started on the first file call and stopped with the sandbox.
//! It isn't an entry of the MCP manager: those servers are shared by every chat, while each
//! chat's file server sees only that chat's workspace.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use futures_util::future::BoxFuture;
use oxide_core::sandbox::{SANDBOX_SERVER, WORKSPACE_DIR, sandbox_name};
use oxide_core::{McpTool, McpToolResult};
use oxide_mcp::handler::{ClientEvent, ClientHandler};
use oxide_mcp::jsonrpc::RpcError;
use oxide_mcp::log::ServerLog;
use oxide_mcp::process::ProcessSpec;
use oxide_mcp::transport::{StdioTransport, Transport};
use oxide_mcp::{ConnectOptions, McpClient, McpError};
use serde_json::{Value, json};

use crate::SandboxError;
use crate::docker::Docker;
use crate::manager::{Sandboxes, plain};

/// How long the server may take to start and answer.
const CONNECT: Duration = Duration::from_secs(60);

/// `read_file` is a deprecated alias of `read_text_file` in newer versions of the server;
/// it is hidden only when `read_text_file` is there (older images have just `read_file`).
const ALIAS: &str = "read_file";
const ALIAS_OF: &str = "read_text_file";

/// Tells the server its root is `/workspace`, so roots can't widen what it reaches.
struct WorkspaceRoot;

impl ClientHandler for WorkspaceRoot {
    fn sample<'a>(
        &'a self,
        _server: &'a str,
        _params: Value,
    ) -> BoxFuture<'a, Result<Value, RpcError>> {
        Box::pin(async { Err(RpcError::new(-1, "sampling isn't available")) })
    }

    fn elicit<'a>(
        &'a self,
        _server: &'a str,
        _params: Value,
    ) -> BoxFuture<'a, Result<Value, RpcError>> {
        Box::pin(async { Ok(json!({"action": "decline"})) })
    }

    fn roots<'a>(&'a self, _server: &'a str) -> BoxFuture<'a, Vec<Value>> {
        Box::pin(async {
            vec![json!({"uri": format!("file://{WORKSPACE_DIR}"), "name": "workspace"})]
        })
    }

    fn event(&self, _server: &str, _event: ClientEvent) {}
}

impl Sandboxes {
    /// The file server's tools, once known (they are the same for every chat).
    pub fn file_tools(&self) -> Option<Vec<McpTool>> {
        crate::manager::lock_tools(&self.inner.file_tools).clone()
    }

    fn remember_tools(&self, tools: Vec<McpTool>) -> Vec<McpTool> {
        let has_new = tools.iter().any(|t| t.name == ALIAS_OF);
        let tools: Vec<McpTool> = tools
            .into_iter()
            .filter(|t| !(has_new && t.name == ALIAS))
            .collect();
        *crate::manager::lock_tools(&self.inner.file_tools) = Some(tools.clone());
        tools
    }

    /// Learns the file server's tools from a throwaway server with an empty workspace, when
    /// its image is downloaded (this never downloads it). Cached for the rest of the run.
    pub async fn discover_file_tools(&self) -> Result<Vec<McpTool>, SandboxError> {
        if let Some(t) = self.file_tools() {
            return Ok(t);
        }
        let d = self.ready().await?;
        let image = self.settings().files_image;
        if crate::manager::image_size(&d, &image).await.is_none() {
            return Err(SandboxError::Invalid(format!(
                "The file server's image ({image}) isn't downloaded yet."
            )));
        }
        let args = vec![
            "run".to_owned(),
            "-i".into(),
            "--rm".into(),
            "--label".into(),
            "oxide.sandbox=1".into(),
            "--network".into(),
            "none".into(),
            "--tmpfs".into(),
            WORKSPACE_DIR.into(),
            image,
            WORKSPACE_DIR.into(),
        ];
        let client = self.spawn_server(&d, args).await?;
        let tools = client.list_tools().await;
        client.close().await;
        let tools = tools.map_err(|e| SandboxError::Files(e.to_string()))?;
        Ok(self.remember_tools(tools))
    }

    /// Calls one of the file server's tools in the chat's sandbox (starting both when
    /// needed).
    pub async fn call_file_tool(
        &self,
        id: &str,
        tool: &str,
        arguments: Value,
    ) -> Result<McpToolResult, SandboxError> {
        for attempt in 0..2 {
            let client = self.files(id).await?;
            match client.call_tool(tool, arguments.clone(), None).await {
                Ok(r) => {
                    self.touch(id);
                    return Ok(r);
                }
                // The server went away (the sandbox was stopped as idle meanwhile): once more.
                Err(McpError::Closed(_)) if attempt == 0 => {
                    self.set_files_client(id, None);
                }
                Err(e) => return Err(SandboxError::Files(e.to_string())),
            }
        }
        Err(SandboxError::Files("the file server keeps stopping".into()))
    }

    /// The chat's file server, started when it isn't running.
    async fn files(&self, id: &str) -> Result<Arc<McpClient>, SandboxError> {
        if let Some(c) = self.files_client(id) {
            return Ok(c);
        }
        let guard = self.files_lock(id);
        let _one = guard.lock().await;
        if let Some(c) = self.files_client(id) {
            return Ok(c);
        }
        // The workspace (volume or folder) must exist, and the container is what knows it.
        self.ensure(id).await?;
        let mount = self.workspace_mount(id).await?;
        let d = self.ensure_files_image().await?;
        let name = sandbox_name(id, true);
        let _ = d
            .run(["rm", "-f", name.as_str()], Duration::from_secs(30))
            .await;
        let image = self.settings().files_image;
        let mut args: Vec<String> = vec![
            "run".into(),
            "-i".into(),
            "--rm".into(),
            "--name".into(),
            name,
            "--label".into(),
            "oxide.sandbox=1".into(),
            "--label".into(),
            format!("oxide.chat={id}"),
            "--network".into(),
            "none".into(),
            "--cap-drop".into(),
            "ALL".into(),
        ];
        for cap in ["CHOWN", "DAC_OVERRIDE", "FOWNER"] {
            args.push("--cap-add".into());
            args.push(cap.into());
        }
        args.extend([
            // Root inside the container (with the few capabilities above), whatever user the
            // image names, so it can always write the workspace.
            "--user".into(),
            "0:0".into(),
            "--security-opt".into(),
            "no-new-privileges".into(),
            "--memory".into(),
            "512m".into(),
            "--pids-limit".into(),
            "256".into(),
            "--mount".into(),
            mount,
            image,
            WORKSPACE_DIR.into(),
        ]);
        let client = self.spawn_server(&d, args).await?;
        if self.file_tools().is_none()
            && let Ok(tools) = client.list_tools().await
        {
            self.remember_tools(tools);
        }
        self.set_files_client(id, Some(client.clone()));
        self.changed(Some(id));
        Ok(client)
    }

    async fn spawn_server(
        &self,
        d: &Docker,
        args: Vec<String>,
    ) -> Result<Arc<McpClient>, SandboxError> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let log = ServerLog::new(
            self.inner
                .logs
                .as_ref()
                .map(|l| l.join("sandbox-files.log")),
            None,
        );
        let spec = ProcessSpec {
            command: plain(&d.program),
            args,
            env: BTreeMap::new(),
            cwd: None,
        };
        let transport = StdioTransport::spawn(&spec, tx, log)
            .map_err(|e| SandboxError::Files(e.to_string()))?;
        let connect = McpClient::connect(
            SANDBOX_SERVER,
            Transport::Stdio(transport),
            rx,
            Arc::new(WorkspaceRoot),
            ConnectOptions::default(),
        );
        match tokio::time::timeout(CONNECT, connect).await {
            Ok(Ok(c)) => Ok(c),
            Ok(Err(e)) => Err(SandboxError::Files(e.to_string())),
            Err(_) => Err(SandboxError::Files(
                "it didn't answer within a minute".into(),
            )),
        }
    }
}
