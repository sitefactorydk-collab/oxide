//! Protocol versions, per-request metadata, HTTP header mirroring, and conversion of server
//! answers into the `oxide-core` types the UI shows.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use oxide_core::{
    McpCapabilities, McpContent, McpPrompt, McpPromptArgument, McpPromptMessage, McpPromptResult,
    McpResource, McpResourceRead, McpResourceTemplate, McpTool, McpToolAnnotations, McpToolResult,
};
use serde_json::{Map, Value, json};

/// Stateless versions Oxide speaks, newest first.
pub const MODERN_VERSIONS: &[&str] = &["2026-07-28"];
/// Handshake-based versions Oxide speaks, newest first.
pub const LEGACY_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

pub const META_PROTOCOL_VERSION: &str = "io.modelcontextprotocol/protocolVersion";
pub const META_CLIENT_INFO: &str = "io.modelcontextprotocol/clientInfo";
pub const META_CLIENT_CAPABILITIES: &str = "io.modelcontextprotocol/clientCapabilities";
pub const META_LOG_LEVEL: &str = "io.modelcontextprotocol/logLevel";
pub const META_SERVER_INFO: &str = "io.modelcontextprotocol/serverInfo";

/// Methods whose `params.name` / `params.uri` goes into the `Mcp-Name` header.
pub fn name_param(method: &str, params: &Value) -> Option<String> {
    let key = match method {
        "tools/call" | "prompts/get" => "name",
        "resources/read" => "uri",
        _ => return None,
    };
    params.get(key).and_then(Value::as_str).map(str::to_owned)
}

pub fn client_info() -> Value {
    json!({"name": oxide_core::APP_NAME, "title": "Oxide", "version": oxide_core::version()})
}

/// What Oxide offers servers: roots (an empty list until folder grants exist), sampling
/// (with user approval) and elicitation (forms and URLs).
pub fn client_capabilities() -> Value {
    json!({
        "roots": {},
        "sampling": {},
        "elicitation": {"form": {}, "url": {}}
    })
}

/// Adds the per-request `_meta` of a modern request (and a progress token, if any) to
/// `params`, keeping any `_meta` already there.
pub fn with_modern_meta(params: Value, version: &str, progress_token: Option<&str>) -> Value {
    let mut obj = match params {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    let mut meta = match obj.remove("_meta") {
        Some(Value::Object(m)) => m,
        _ => Map::new(),
    };
    meta.insert(META_PROTOCOL_VERSION.into(), json!(version));
    meta.insert(META_CLIENT_INFO.into(), client_info());
    meta.insert(META_CLIENT_CAPABILITIES.into(), client_capabilities());
    meta.insert(META_LOG_LEVEL.into(), json!("info"));
    if let Some(t) = progress_token {
        meta.insert("progressToken".into(), json!(t));
    }
    obj.insert("_meta".into(), Value::Object(meta));
    Value::Object(obj)
}

/// Adds only a progress token (legacy requests carry no other metadata).
pub fn with_progress_token(params: Value, progress_token: Option<&str>) -> Value {
    let Some(t) = progress_token else {
        return params;
    };
    let mut obj = match params {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    let meta = obj
        .entry("_meta")
        .or_insert_with(|| Value::Object(Map::new()));
    if let Value::Object(m) = meta {
        m.insert("progressToken".into(), json!(t));
    }
    Value::Object(obj)
}

/// Encodes a header value: plain if it is safe visible ASCII without surrounding spaces,
/// otherwise as `=?base64?…?=` (Streamable HTTP "Value Encoding").
pub fn encode_header_value(value: &str) -> String {
    let safe = !value.is_empty()
        && value
            .bytes()
            .all(|b| (0x20..=0x7e).contains(&b) || b == b'\t')
        && !value.starts_with([' ', '\t'])
        && !value.ends_with([' ', '\t'])
        && !(value.starts_with("=?base64?") && value.ends_with("?="));
    if safe || value.is_empty() {
        value.to_owned()
    } else {
        format!("=?base64?{}?=", B64.encode(value.as_bytes()))
    }
}

/// `x-mcp-header` annotations of a tool's input schema: (header name part, property path).
/// `Err` explains why the tool definition is invalid (it is then hidden on HTTP).
pub fn header_params(schema: &Value) -> Result<Vec<(String, Vec<String>)>, String> {
    let mut out = Vec::new();
    collect_header_params(schema, &mut Vec::new(), &mut out)?;
    // Anything annotated outside a pure `properties` chain makes the tool invalid.
    if count_annotations(schema) != out.len() {
        return Err("x-mcp-header used outside a plain property".into());
    }
    let mut seen = std::collections::HashSet::new();
    for (name, _) in &out {
        if !seen.insert(name.to_ascii_lowercase()) {
            return Err(format!("duplicate x-mcp-header \"{name}\""));
        }
    }
    Ok(out)
}

fn collect_header_params(
    schema: &Value,
    path: &mut Vec<String>,
    out: &mut Vec<(String, Vec<String>)>,
) -> Result<(), String> {
    let Some(props) = schema.get("properties").and_then(Value::as_object) else {
        return Ok(());
    };
    for (key, prop) in props {
        path.push(key.clone());
        if let Some(h) = prop.get("x-mcp-header") {
            let name = h.as_str().ok_or("x-mcp-header must be a string")?;
            let token_ok = !name.is_empty()
                && name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b));
            if !token_ok {
                return Err(format!("invalid x-mcp-header \"{name}\""));
            }
            let ty = prop.get("type").and_then(Value::as_str).unwrap_or("");
            if !matches!(ty, "string" | "integer" | "boolean") {
                return Err(format!(
                    "x-mcp-header \"{name}\" on a non-primitive property"
                ));
            }
            out.push((name.to_owned(), path.clone()));
        }
        collect_header_params(prop, path, out)?;
        path.pop();
    }
    Ok(())
}

fn count_annotations(v: &Value) -> usize {
    match v {
        Value::Object(m) => {
            usize::from(m.contains_key("x-mcp-header"))
                + m.values().map(count_annotations).sum::<usize>()
        }
        Value::Array(a) => a.iter().map(count_annotations).sum(),
        _ => 0,
    }
}

/// The `Mcp-Param-*` headers for a call with `args`.
pub fn param_headers(schema: &Value, args: &Value) -> Vec<(String, String)> {
    let Ok(params) = header_params(schema) else {
        return Vec::new();
    };
    params
        .into_iter()
        .filter_map(|(name, path)| {
            let mut v = args;
            for key in &path {
                v = v.get(key)?;
            }
            let s = match v {
                Value::String(s) => s.clone(),
                Value::Bool(b) => b.to_string(),
                Value::Number(n) if n.is_i64() || n.is_u64() => n.to_string(),
                _ => return None,
            };
            Some((format!("Mcp-Param-{name}"), encode_header_value(&s)))
        })
        .collect()
}

/// Summary of a server's `capabilities` object.
pub fn capabilities_summary(caps: &Value) -> McpCapabilities {
    let list_changed = ["tools", "resources", "prompts"].iter().any(|k| {
        caps.get(k)
            .and_then(|c| c.get("listChanged"))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    });
    McpCapabilities {
        tools: caps.get("tools").is_some(),
        resources: caps.get("resources").is_some(),
        prompts: caps.get("prompts").is_some(),
        logging: caps.get("logging").is_some(),
        list_changed,
    }
}

fn opt_str(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_owned)
}

pub fn parse_tool(server: &str, v: &Value) -> Option<McpTool> {
    let name = opt_str(v, "name")?;
    let schema = v
        .get("inputSchema")
        .cloned()
        .unwrap_or_else(|| json!({"type": "object"}));
    let a = v.get("annotations").cloned().unwrap_or(Value::Null);
    let b = |k: &str| a.get(k).and_then(Value::as_bool);
    Some(McpTool {
        server: server.to_owned(),
        name,
        title: opt_str(v, "title").or_else(|| opt_str(&a, "title")),
        description: opt_str(v, "description"),
        input_schema: schema.to_string(),
        annotations: McpToolAnnotations {
            title: opt_str(&a, "title"),
            read_only_hint: b("readOnlyHint"),
            destructive_hint: b("destructiveHint"),
            idempotent_hint: b("idempotentHint"),
            open_world_hint: b("openWorldHint"),
        },
    })
}

pub fn parse_resource(server: &str, v: &Value) -> Option<McpResource> {
    let uri = opt_str(v, "uri")?;
    Some(McpResource {
        server: server.to_owned(),
        name: opt_str(v, "name").unwrap_or_else(|| uri.clone()),
        uri,
        title: opt_str(v, "title"),
        description: opt_str(v, "description"),
        mime_type: opt_str(v, "mimeType"),
        size: v.get("size").and_then(Value::as_u64),
    })
}

pub fn parse_template(server: &str, v: &Value) -> Option<McpResourceTemplate> {
    let uri_template = opt_str(v, "uriTemplate")?;
    Some(McpResourceTemplate {
        server: server.to_owned(),
        name: opt_str(v, "name").unwrap_or_else(|| uri_template.clone()),
        uri_template,
        title: opt_str(v, "title"),
        description: opt_str(v, "description"),
        mime_type: opt_str(v, "mimeType"),
    })
}

pub fn parse_prompt(server: &str, v: &Value) -> Option<McpPrompt> {
    let name = opt_str(v, "name")?;
    let arguments = v
        .get("arguments")
        .and_then(Value::as_array)
        .map(|args| {
            args.iter()
                .filter_map(|a| {
                    Some(McpPromptArgument {
                        name: opt_str(a, "name")?,
                        title: opt_str(a, "title"),
                        description: opt_str(a, "description"),
                        required: a.get("required").and_then(Value::as_bool).unwrap_or(false),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Some(McpPrompt {
        server: server.to_owned(),
        name,
        title: opt_str(v, "title"),
        description: opt_str(v, "description"),
        arguments,
    })
}

/// One content block. Unknown types become a text note so nothing is silently lost.
pub fn parse_content(v: &Value) -> McpContent {
    let ty = v.get("type").and_then(Value::as_str).unwrap_or("");
    match ty {
        "text" => McpContent::Text {
            text: opt_str(v, "text").unwrap_or_default(),
        },
        "image" => McpContent::Image {
            data: opt_str(v, "data").unwrap_or_default(),
            mime_type: opt_str(v, "mimeType").unwrap_or_else(|| "image/png".into()),
        },
        "audio" => McpContent::Audio {
            data: opt_str(v, "data").unwrap_or_default(),
            mime_type: opt_str(v, "mimeType").unwrap_or_else(|| "audio/wav".into()),
        },
        "resource_link" => McpContent::ResourceLink {
            uri: opt_str(v, "uri").unwrap_or_default(),
            name: opt_str(v, "name"),
            mime_type: opt_str(v, "mimeType"),
            description: opt_str(v, "description"),
        },
        "resource" => resource_contents(v.get("resource").unwrap_or(&Value::Null)),
        other => McpContent::Text {
            text: format!("[unsupported content type \"{other}\"]"),
        },
    }
}

/// A `TextResourceContents` / `BlobResourceContents` object.
pub fn resource_contents(r: &Value) -> McpContent {
    McpContent::Resource {
        uri: opt_str(r, "uri").unwrap_or_default(),
        mime_type: opt_str(r, "mimeType"),
        text: opt_str(r, "text"),
        blob: opt_str(r, "blob"),
    }
}

pub fn parse_tool_result(v: &Value, duration_ms: u64) -> McpToolResult {
    let content = v
        .get("content")
        .and_then(Value::as_array)
        .map(|a| a.iter().map(parse_content).collect())
        .unwrap_or_default();
    McpToolResult {
        content,
        structured: v
            .get("structuredContent")
            .filter(|s| !s.is_null())
            .map(Value::to_string),
        is_error: v.get("isError").and_then(Value::as_bool).unwrap_or(false),
        duration_ms,
    }
}

pub fn parse_resource_read(v: &Value) -> McpResourceRead {
    McpResourceRead {
        contents: v
            .get("contents")
            .and_then(Value::as_array)
            .map(|a| a.iter().map(resource_contents).collect())
            .unwrap_or_default(),
    }
}

pub fn parse_prompt_result(v: &Value) -> McpPromptResult {
    McpPromptResult {
        description: opt_str(v, "description"),
        messages: v
            .get("messages")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|m| McpPromptMessage {
                        role: opt_str(m, "role").unwrap_or_else(|| "user".into()),
                        content: parse_content(m.get("content").unwrap_or(&Value::Null)),
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// Plain text of content blocks (text and text resources), for attaching as context.
pub fn content_text(content: &[McpContent]) -> String {
    let mut parts = Vec::new();
    for c in content {
        match c {
            McpContent::Text { text } => parts.push(text.clone()),
            McpContent::Resource {
                text: Some(text), ..
            } => parts.push(text.clone()),
            McpContent::ResourceLink { uri, name, .. } => {
                parts.push(format!("[{}]({uri})", name.as_deref().unwrap_or(uri)));
            }
            _ => {}
        }
    }
    parts.join("\n\n")
}

/// A prompt's messages as one text file to attach.
pub fn prompt_as_text(r: &McpPromptResult) -> String {
    let all_user = r.messages.iter().all(|m| m.role == "user");
    let mut parts = Vec::new();
    for m in &r.messages {
        let text = content_text(std::slice::from_ref(&m.content));
        if text.is_empty() {
            continue;
        }
        if all_user {
            parts.push(text);
        } else {
            parts.push(format!(
                "{}:\n{text}",
                if m.role == "assistant" {
                    "Assistant"
                } else {
                    "User"
                }
            ));
        }
    }
    parts.join("\n\n")
}

fn extension_for(mime: Option<&str>) -> &'static str {
    match mime.unwrap_or("") {
        "text/markdown" => "md",
        "application/json" => "json",
        "text/html" => "html",
        "text/csv" => "csv",
        "application/pdf" => "pdf",
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        m if m.starts_with("image/") => "png",
        _ => "txt",
    }
}

/// A file name for a resource: the last part of its URI, with an extension from its type.
pub fn resource_file_name(uri: &str, mime: Option<&str>, fallback: &str) -> String {
    let tail = uri
        .trim_end_matches('/')
        .rsplit(['/', ':', '\\'])
        .next()
        .unwrap_or("");
    let tail = tail.split(['?', '#']).next().unwrap_or("");
    let clean: String = tail
        .chars()
        .filter(|c| !matches!(c, '<' | '>' | '"' | '|' | '*' | '?'))
        .take(80)
        .collect();
    let base = if clean.is_empty() {
        fallback.to_owned()
    } else {
        clean
    };
    if base.contains('.') {
        base
    } else {
        format!("{base}.{}", extension_for(mime))
    }
}

/// The files to attach for a read resource: `(file name, bytes)` per content item.
pub fn resource_files(read: &McpResourceRead, fallback: &str) -> Vec<(String, Vec<u8>)> {
    read.contents
        .iter()
        .filter_map(|c| match c {
            McpContent::Resource {
                uri,
                mime_type,
                text: Some(text),
                ..
            } => Some((
                resource_file_name(uri, mime_type.as_deref().or(Some("text/plain")), fallback),
                text.clone().into_bytes(),
            )),
            McpContent::Resource {
                uri,
                mime_type,
                blob: Some(blob),
                ..
            } => B64.decode(blob.trim()).ok().map(|bytes| {
                (
                    resource_file_name(uri, mime_type.as_deref(), fallback),
                    bytes,
                )
            }),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompts_and_resources_become_files() {
        let p = parse_prompt_result(&json!({"messages": [
            {"role": "user", "content": {"type": "text", "text": "One"}},
            {"role": "user", "content": {"type": "text", "text": "Two"}}
        ]}));
        assert_eq!(prompt_as_text(&p), "One\n\nTwo");
        let p = parse_prompt_result(&json!({"messages": [
            {"role": "user", "content": {"type": "text", "text": "Q"}},
            {"role": "assistant", "content": {"type": "text", "text": "A"}}
        ]}));
        assert_eq!(prompt_as_text(&p), "User:\nQ\n\nAssistant:\nA");

        assert_eq!(
            resource_file_name("file:///a/b/notes.md", None, "r"),
            "notes.md"
        );
        assert_eq!(
            resource_file_name("mock://readme", Some("text/plain"), "r"),
            "readme.txt"
        );
        assert_eq!(
            resource_file_name("db://rows/", Some("application/json"), "r"),
            "rows.json"
        );
        assert_eq!(resource_file_name("x://", None, "fallback"), "fallback.txt");
        let read = parse_resource_read(&json!({"contents": [
            {"uri": "mock://readme", "text": "hi"},
            {"uri": "img://logo", "mimeType": "image/png", "blob": "AAEC"},
            {"uri": "bad://", "blob": "!!!"}
        ]}));
        let files = resource_files(&read, "r");
        assert_eq!(files.len(), 2);
        assert_eq!(files[0], ("readme.txt".to_string(), b"hi".to_vec()));
        assert_eq!(files[1], ("logo.png".to_string(), vec![0, 1, 2]));
    }

    #[test]
    fn modern_meta_is_added_and_kept() {
        let p = with_modern_meta(
            json!({"name": "x", "_meta": {"traceparent": "t"}}),
            "2026-07-28",
            Some("p1"),
        );
        let meta = &p["_meta"];
        assert_eq!(meta[META_PROTOCOL_VERSION], json!("2026-07-28"));
        assert_eq!(meta[META_CLIENT_INFO]["name"], json!("Oxide"));
        assert!(meta[META_CLIENT_CAPABILITIES].is_object());
        assert_eq!(meta["traceparent"], json!("t"));
        assert_eq!(meta["progressToken"], json!("p1"));
        assert_eq!(p["name"], json!("x"));
        assert!(with_modern_meta(Value::Null, "v", None)["_meta"].is_object());
    }

    #[test]
    fn header_values_are_encoded_when_unsafe() {
        assert_eq!(encode_header_value("us-west1"), "us-west1");
        assert_eq!(
            encode_header_value("Hello, 世界"),
            "=?base64?SGVsbG8sIOS4lueVjA==?="
        );
        assert_eq!(encode_header_value(" padded "), "=?base64?IHBhZGRlZCA=?=");
        assert_eq!(
            encode_header_value("line1\nline2"),
            "=?base64?bGluZTEKbGluZTI=?="
        );
        assert_eq!(
            encode_header_value("=?base64?literal?="),
            "=?base64?PT9iYXNlNjQ/bGl0ZXJhbD89?="
        );
    }

    #[test]
    fn x_mcp_header_params_are_mirrored() {
        let schema = json!({
            "type": "object",
            "properties": {
                "region": {"type": "string", "x-mcp-header": "Region"},
                "opts": {"type": "object", "properties": {
                    "dry": {"type": "boolean", "x-mcp-header": "Dry"}
                }},
                "query": {"type": "string"}
            }
        });
        let h = param_headers(
            &schema,
            &json!({"region": "us-west1", "opts": {"dry": true}, "query": "q"}),
        );
        assert!(h.contains(&("Mcp-Param-Region".into(), "us-west1".into())));
        assert!(h.contains(&("Mcp-Param-Dry".into(), "true".into())));
        assert_eq!(param_headers(&schema, &json!({})), vec![]);
    }

    #[test]
    fn invalid_x_mcp_headers_are_rejected() {
        let bad = |schema: Value| header_params(&schema).is_err();
        assert!(bad(
            json!({"properties": {"a": {"type": "number", "x-mcp-header": "A"}}})
        ));
        assert!(bad(json!({"properties": {
            "a": {"type": "string", "x-mcp-header": "A"},
            "b": {"type": "string", "x-mcp-header": "a"}
        }})));
        assert!(bad(json!({"properties": {"a": {"type": "array", "items": {
            "type": "string", "x-mcp-header": "A"
        }}}})));
        assert!(bad(
            json!({"properties": {"a": {"type": "string", "x-mcp-header": "bad name"}}})
        ));
        assert!(!bad(json!({"properties": {"a": {"type": "string"}}})));
    }

    #[test]
    fn lists_and_results_are_parsed() {
        let t = parse_tool(
            "s",
            &json!({"name": "echo", "description": "d", "inputSchema": {"type": "object"},
                    "annotations": {"readOnlyHint": true}}),
        )
        .expect("tool");
        assert_eq!(t.name, "echo");
        assert_eq!(t.annotations.read_only_hint, Some(true));
        assert_eq!(t.input_schema, r#"{"type":"object"}"#);

        let r = parse_tool_result(
            &json!({"content": [{"type": "text", "text": "hi"},
                                {"type": "image", "data": "AA==", "mimeType": "image/png"},
                                {"type": "resource", "resource": {"uri": "u", "text": "t"}},
                                {"type": "weird"}],
                    "isError": true, "structuredContent": {"a": 1}}),
            5,
        );
        assert!(r.is_error);
        assert_eq!(r.content.len(), 4);
        assert_eq!(r.structured.as_deref(), Some(r#"{"a":1}"#));
        assert!(content_text(&r.content).starts_with("hi\n\nt\n\n[unsupported"));

        let p = parse_prompt(
            "s",
            &json!({"name": "p", "arguments": [{"name": "a", "required": true}, {"name": "b"}]}),
        )
        .expect("prompt");
        assert!(p.arguments[0].required && !p.arguments[1].required);

        let caps = capabilities_summary(&json!({"tools": {"listChanged": true}, "logging": {}}));
        assert!(caps.tools && caps.logging && caps.list_changed && !caps.prompts);
    }
}
