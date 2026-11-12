//! Turning the visible thread into a model request.

use oxide_core::{
    ContentBlock, Effort, GenerateOptions, Message, ModelCapabilities, ProviderKind, Role,
    ThinkingStyle, settings::MIN_THINKING_BUDGET,
};
use oxide_llm::{ChatMessage, ChatRequest, Thinking};

/// Output tokens asked for when the model doesn't say how many it allows.
const FALLBACK_MAX_OUTPUT: u32 = 8_192;
/// Upper bound on output tokens for a normal reply.
const REPLY_MAX_OUTPUT: u32 = 32_000;
/// Upper bound at `xhigh` / `max` effort (the effort docs advise a large limit there).
const HIGH_EFFORT_MAX_OUTPUT: u32 = 64_000;
/// Room left for the answer after a thinking budget.
const ANSWER_ROOM: u32 = 4_096;

/// The conversation as the model sees it.
///
/// - Earlier thinking is left out (providers ignore or reject it in past turns).
/// - A reply that used tools is split into its turns: the model's calls, then the results
///   (as a user turn), then what it wrote next. Calls without results (and results without
///   calls) are dropped, since both APIs reject them.
/// - Provider-run tools (Anthropic web search) of earlier replies are left out; their text
///   stays.
/// - Empty turns are skipped and consecutive turns of the same role merged, so roles
///   alternate starting with the user.
pub fn build_messages(thread: &[Message]) -> Vec<ChatMessage> {
    let mut out: Vec<ChatMessage> = Vec::new();
    for m in thread {
        match m.role {
            Role::User => push_turn(&mut out, Role::User, keep_blocks(&m.content, false)),
            Role::Assistant => {
                for turn in split_turns(&m.content, false) {
                    push_turn(&mut out, turn.role, turn.content);
                }
            }
        }
    }
    finish(out)
}

/// The turns of the reply being written, sent after [`build_messages`] while the tool loop
/// runs. Signed thinking and provider tool blocks are kept: they must go back unchanged.
pub fn reply_turns(content: &[ContentBlock]) -> Vec<ChatMessage> {
    let mut out = Vec::new();
    for turn in split_turns(content, true) {
        push_turn(&mut out, turn.role, turn.content);
    }
    out
}

/// Joins the history and the reply so far, and drops unmatched tool calls and results.
pub fn request_messages(history: &[ChatMessage], reply: &[ContentBlock]) -> Vec<ChatMessage> {
    let mut out: Vec<ChatMessage> = Vec::new();
    for m in history.iter().cloned().chain(reply_turns(reply)) {
        push_turn(&mut out, m.role, m.content);
    }
    finish(out)
}

fn keep_blocks(content: &[ContentBlock], current: bool) -> Vec<ContentBlock> {
    content
        .iter()
        .filter(|b| match b {
            ContentBlock::Text { text } => !text.trim().is_empty(),
            ContentBlock::Image { .. }
            | ContentBlock::Document { .. }
            | ContentBlock::ArtifactRef { .. }
            | ContentBlock::ToolUse { .. }
            | ContentBlock::ToolResult { .. } => true,
            ContentBlock::Thinking { signature, .. } => current && signature.is_some(),
            ContentBlock::ServerToolUse { .. } | ContentBlock::ServerToolResult { .. } => current,
        })
        .cloned()
        .collect()
}

/// An assistant message's blocks as alternating assistant / user (tool results) turns.
fn split_turns(content: &[ContentBlock], current: bool) -> Vec<ChatMessage> {
    let mut turns: Vec<ChatMessage> = Vec::new();
    for b in keep_blocks(content, current) {
        let role = if matches!(b, ContentBlock::ToolResult { .. }) {
            Role::User
        } else {
            Role::Assistant
        };
        match turns.last_mut() {
            Some(t) if t.role == role => t.content.push(b),
            _ => turns.push(ChatMessage {
                role,
                content: vec![b],
            }),
        }
    }
    turns
}

fn push_turn(out: &mut Vec<ChatMessage>, role: Role, content: Vec<ContentBlock>) {
    if content.is_empty() {
        return;
    }
    match out.last_mut() {
        Some(prev) if prev.role == role => {
            // Tool results go first in a user turn (Anthropic requires it).
            if role == Role::User {
                let (mut results, rest): (Vec<_>, Vec<_>) = prev
                    .content
                    .drain(..)
                    .chain(content)
                    .partition(|b| matches!(b, ContentBlock::ToolResult { .. }));
                results.extend(rest);
                prev.content = results;
            } else {
                prev.content.extend(content);
            }
        }
        _ => out.push(ChatMessage { role, content }),
    }
}

/// Drops tool calls without a result in the next turn and results without a call in the
/// turn before, then empty turns, and starts with a user turn.
fn finish(turns: Vec<ChatMessage>) -> Vec<ChatMessage> {
    use std::collections::HashSet;
    let ids = |m: &ChatMessage, results: bool| -> HashSet<String> {
        m.content
            .iter()
            .filter_map(|b| match (b, results) {
                (ContentBlock::ToolUse { id, .. }, false) => Some(id.clone()),
                (ContentBlock::ToolResult { tool_use_id, .. }, true) => Some(tool_use_id.clone()),
                _ => None,
            })
            .collect()
    };
    let n = turns.len();
    let calls: Vec<HashSet<String>> = turns.iter().map(|m| ids(m, false)).collect();
    let results: Vec<HashSet<String>> = turns.iter().map(|m| ids(m, true)).collect();
    // Server tool uses need their result in the same turn.
    let server_results: Vec<HashSet<String>> = turns
        .iter()
        .map(|m| {
            m.content
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::ServerToolResult { tool_use_id, .. } => Some(tool_use_id.clone()),
                    _ => None,
                })
                .collect()
        })
        .collect();
    let mut cleaned: Vec<ChatMessage> = Vec::new();
    for (i, mut m) in turns.into_iter().enumerate() {
        let next_results = if i + 1 < n {
            &results[i + 1]
        } else {
            &HashSet::new()
        };
        let prev_calls = if i > 0 {
            &calls[i - 1]
        } else {
            &HashSet::new()
        };
        let is_last = i + 1 == n;
        m.content.retain(|b| match b {
            // The last turn's calls are being answered right now (never sent that way).
            ContentBlock::ToolUse { id, .. } => !is_last && next_results.contains(id),
            ContentBlock::ToolResult { tool_use_id, .. } => prev_calls.contains(tool_use_id),
            ContentBlock::ServerToolUse { id, .. } => server_results[i].contains(id),
            _ => true,
        });
        push_turn(&mut cleaned, m.role, m.content);
    }
    while cleaned.first().is_some_and(|m| m.role != Role::User) {
        cleaned.remove(0);
    }
    cleaned
}

/// What the system prompt says about the user's situation.
#[derive(Debug, Clone, Default)]
pub struct PromptContext<'a> {
    pub user_name: &'a str,
    pub now_ms: i64,
    /// Minutes east of UTC, from the window's clock.
    pub utc_offset_minutes: Option<i32>,
    /// IANA name, e.g. `Europe/Copenhagen`.
    pub time_zone: Option<&'a str>,
    /// `windows`, `macos`, `linux`.
    pub os: &'a str,
    pub web_search: bool,
    pub web_fetch: bool,
    /// MCP servers whose tools are on.
    pub connectors: Vec<String>,
    /// The `artifact` tool is on.
    pub artifacts: bool,
    /// The chat's project, by name.
    pub project_name: Option<&'a str>,
    /// The chat's project instructions.
    pub project_instructions: Option<&'a str>,
    /// Knowledge files too large to include: their names (the model gets `project_search`).
    /// Knowledge that fits goes in `ChatRequest::system_context` instead.
    pub knowledge_search: Vec<String>,
    /// What Oxide remembers about the user, as a bullet list (memory on).
    pub memory: Option<&'a str>,
    /// The skills the model may load, as `- name: description` lines (`load_skill` is on).
    pub skills: Option<&'a str>,
    /// Folders the user shared with the chat (the file tools are on).
    pub folders: Vec<String>,
    /// `delete_file` deletes for good instead of moving things into `_to_delete`.
    pub delete_for_good: bool,
    /// The shell `run_command` uses, when the shell tool is on.
    pub shell: Option<&'a str>,
    /// The `task_list` tool is on.
    pub task_list: bool,
    /// The `run_python` tool is on.
    pub python: bool,
    /// The Docker sandbox's tools are on.
    pub sandbox: Option<SandboxPrompt>,
    /// The browser agent's tools are on.
    pub browser: bool,
}

/// What models are told about the chat's Docker sandbox.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SandboxPrompt {
    /// The image commands run in.
    pub image: String,
    /// The sandbox can reach the internet.
    pub network: bool,
    /// The file server's tools (`sandbox__…`) are offered.
    pub files: bool,
    /// `sandbox_copy_in` / `sandbox_copy_out` are offered (the chat has shared folders).
    pub copies: bool,
}

fn os_name(os: &str) -> &str {
    match os {
        "windows" => "Windows",
        "macos" => "macOS",
        "linux" => "Linux",
        other => other,
    }
}

/// The system prompt: who the assistant is, the date and time, the user, tools, project
/// instructions and memory.
pub fn system_prompt(ctx: &PromptContext<'_>) -> String {
    let offset = ctx.utc_offset_minutes.unwrap_or(0);
    let local = ctx.now_ms + i64::from(offset) * 60_000;
    let (y, m, d) = civil_date(local);
    let mins = local.div_euclid(60_000).rem_euclid(1_440);
    let (hh, mm) = (mins / 60, mins % 60);
    let weekday =
        WEEKDAYS[usize::try_from(local.div_euclid(86_400_000).rem_euclid(7)).unwrap_or(0)];
    let zone = match (ctx.utc_offset_minutes, ctx.time_zone) {
        (None, _) => "UTC".to_owned(),
        (Some(o), tz) => {
            let sign = if o < 0 { '-' } else { '+' };
            let o = o.abs();
            let utc = format!("UTC{sign}{:02}:{:02}", o / 60, o % 60);
            match tz.filter(|t| !t.trim().is_empty()) {
                Some(t) => format!("{t}, {utc}"),
                None => utc,
            }
        }
    };
    let mut s = format!(
        "You are Oxide, a helpful AI assistant in a desktop app. It is {weekday} \
         {y:04}-{m:02}-{d:02}, {hh:02}:{mm:02} ({zone}) for the user. \
         Format answers with Markdown where it helps; use fenced code blocks with a language for code. \
         Files the user attaches appear in their message as <file> blocks or documents; read them in \
         full. Finish what you're asked in one reply: long replies are fine and there is room for \
         them. When asked to translate, rewrite, convert or summarize a file or text, do all of it, \
         not a sample or the first part, unless the user asks for less."
    );
    let name = ctx.user_name.trim();
    if !name.is_empty() {
        s.push_str(&format!(" The user's name is {name}."));
    }
    if !ctx.os.is_empty() {
        s.push_str(&format!(" The user's computer runs {}.", os_name(ctx.os)));
    }

    let mut tools = Vec::new();
    if ctx.web_search {
        tools.push("search the web".to_owned());
    }
    if ctx.web_fetch {
        tools.push("read web pages by URL".to_owned());
    }
    if !ctx.connectors.is_empty() {
        tools.push(format!(
            "use tools from these connectors: {}",
            ctx.connectors.join(", ")
        ));
    }
    if ctx.artifacts {
        tools.push(
            "make artifacts (documents, code, web pages, diagrams and small apps shown beside \
             the chat)"
                .to_owned(),
        );
    }
    if !tools.is_empty() {
        s.push_str(&format!(
            "\n\nYou can {}. Use tools when they help answer better or are needed for the task; \
             don't use them for things you already know well. Some tool calls need the user's \
             approval; if one is declined, carry on without it or ask what they'd prefer. \
             When you use information from the web, mention where it came from.",
            join_list(&tools)
        ));
        if ctx.web_search {
            s.push_str(&format!(
                " Search for anything that may have changed since your training: news, prices, \
                 current office holders, software versions. Write queries in the user's language \
                 when it matters, and include the year ({y}) for recent events."
            ));
        }
        if ctx.artifacts {
            s.push_str(
                " Put substantial content the user will keep, edit or run (a document, a longer \
                 program, a page, a diagram, an interactive app) in an artifact; keep short \
                 answers, explanations and small snippets in the chat. To change an artifact, \
                 call the artifact tool again with its id instead of writing it out again.",
            );
        }
    }
    if !ctx.folders.is_empty() {
        let list: Vec<String> = ctx.folders.iter().map(|f| format!("- {f}")).collect();
        s.push_str(&format!(
            "\n\n<shared_folders>\n{}\n</shared_folders>\nThe user shared these folders on \
             their computer with this chat. You can list, read and search files in them \
             (list_dir, read_file, search_files) and change them (write_file, edit_file, \
             move_file, delete_file; the user approves changes). Nothing outside these folders \
             can be reached. Look before you change things, and don't delete or overwrite \
             anything the user didn't ask you to.{}",
            list.join("\n"),
            if ctx.delete_for_good {
                " delete_file deletes for good."
            } else {
                " delete_file moves things into a _to_delete folder the user can empty."
            }
        ));
        if let Some(shell) = ctx.shell {
            s.push_str(&format!(
                " You can also run commands there with run_command ({shell}); prefer the file \
                 tools for reading and changing files."
            ));
        }
    }
    if ctx.python {
        s.push_str(
            "\n\nYou can run Python with run_python (pandas, numpy, matplotlib and openpyxl are \
             installed). Use it for calculations, data analysis, charts and for making files such \
             as spreadsheets; print the results you need, and save charts with plt.savefig. Prefer \
             it over doing arithmetic or tabulating data in your head. The user approves each run \
             and sees your code, so keep scripts short and readable, and don't touch files you \
             weren't asked to.",
        );
    }
    if let Some(sb) = &ctx.sandbox {
        s.push_str(&format!(
            "\n\nThis chat has a sandbox: an isolated Linux container ({image}) with a workspace \
             at /workspace that keeps its files for the whole chat. Use it whenever you write \
             code that should be built, run or tested: create the files there{files}, run \
             commands with sandbox_exec, read the output, fix what fails and run again until it \
             works. {network} The sandbox can't see the user's computer.",
            image = sb.image,
            files = if sb.files {
                " (with the sandbox__ file tools, e.g. sandbox__write_file and sandbox__edit_file)"
            } else {
                " (with sandbox_exec, e.g. cat > file <<'EOF')"
            },
            network = if sb.network {
                "It has internet access, so you can install packages (pip, apt-get, npm)."
            } else {
                "It has no network: use what the image has (no pip or apt-get downloads), and \
                 don't try to reach the internet from it."
            },
        ));
        if sb.copies {
            s.push_str(
                " To give the user a result, copy it into one of their shared folders with \
                 sandbox_copy_out; bring their files in with sandbox_copy_in. Each copy asks \
                 the user first.",
            );
        } else {
            s.push_str(
                " No folder is shared with this chat, so results stay in the sandbox; the user \
                 can save them from the chat's Sandbox panel, or share a folder for \
                 sandbox_copy_out.",
            );
        }
    }
    if ctx.browser {
        s.push_str(
            "\n\nYou can use a web browser through the browser__ tools: browser__search \
             searches Google, browser__navigate opens a page and returns its text with links, \
             buttons and fields numbered (like [3] button \"Search\"), browser__read_page reads \
             the current page, browser__click and browser__type act on those numbers, \
             browser__screenshot shows the page, and browser__browser_task hands a whole \
             multi-step goal to an autonomous browsing agent. Whenever you don't know the exact \
             address of the page you need, find it with browser__search first and open a real \
             result; never guess URLs. When Google's results include an AI Overview that \
             answers the question, use it and say it comes from Google's AI Overview; open its \
             sources when accuracy matters. When a page fails (not found, access denied, unknown \
             site), search for the right one instead of trying made-up addresses. If a search is \
             refused with a check for robots, never try to solve or get around it: ask the user \
             to complete it, or use another way to find the page. Use the \
             browser for pages that need JavaScript, clicking or forms; a public page whose \
             address you know is quicker with web_fetch when you have it. Never type passwords, \
             payment details or other sensitive data, and don't buy, post, send or sign up for \
             anything unless the user asked for exactly that. Close the browser with \
             browser__close when you're done.",
        );
    }
    if ctx.task_list {
        s.push_str(
            "\n\nFor work with several steps (more than two tool calls, or anything the user \
             would want to follow), first make a plan with the task_list tool, then keep it \
             up to date as you go: mark each step in_progress when you start it and done when \
             it's finished. When everything is done, tell the user what you did.",
        );
    }
    if let Some(list) = ctx.skills.map(str::trim).filter(|l| !l.is_empty()) {
        s.push_str(&format!(
            "\n\n<skills>\n{list}\n</skills>\nThese are skills the user installed: instructions for \
             particular kinds of tasks. When a request matches a skill's description, call \
             load_skill with its name before you start and follow what it says (it may point \
             to more files to read). Don't load skills that don't apply."
        ));
    }
    if let Some(name) = ctx.project_name.map(str::trim).filter(|n| !n.is_empty()) {
        s.push_str(&format!(
            "\n\nThis chat belongs to the user's project “{name}”."
        ));
    }
    if !ctx.knowledge_search.is_empty() {
        let shown: Vec<&str> = ctx
            .knowledge_search
            .iter()
            .take(50)
            .map(String::as_str)
            .collect();
        let more = ctx.knowledge_search.len().saturating_sub(shown.len());
        s.push_str(&format!(
            " Its knowledge files are too large to include here: {}{}. Search them with the \
             project_search tool whenever a question may be answered by the user's files, \
             before answering from general knowledge, and say which file an answer came from.",
            shown.join(", "),
            if more > 0 {
                format!(" and {more} more")
            } else {
                String::new()
            }
        ));
    }
    if let Some(mem) = ctx.memory.map(str::trim).filter(|p| !p.is_empty()) {
        s.push_str(&format!(
            "\n\n<memory>\n{mem}\n</memory>\nThis is what Oxide remembers about the user from \
             earlier chats. Use it when relevant, without mentioning it otherwise; the user's \
             current messages take precedence."
        ));
    }
    // Last, so they're the freshest thing in the prompt: small local models without thinking
    // otherwise tend to drop them (seen with Qwen on Strata in the Phase 7 check).
    if let Some(p) = ctx
        .project_instructions
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        s.push_str(&format!(
            "\n\n<project_instructions>\n{p}\n</project_instructions>\nThe user set these \
             instructions for every chat in this project. Follow them in every reply, including \
             the language, tone and format they ask for. They take priority over the general \
             guidance above. Only an explicit request from the user in this chat overrides \
             them; the language a message is written in is not such a request."
        ));
    }
    s
}

const WEEKDAYS: [&str; 7] = [
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
];

fn join_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [a] => a.clone(),
        [a, b] => format!("{a} and {b}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
    }
}

/// Year, month and day (UTC) of a Unix time in milliseconds.
pub(crate) fn civil_date(ms: i64) -> (i64, u32, u32) {
    // Howard Hinnant's days-to-civil algorithm.
    let z = ms.div_euclid(86_400_000) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (
        y,
        u32::try_from(m).unwrap_or(1),
        u32::try_from(d).unwrap_or(1),
    )
}

/// Fills in thinking, effort, media handling, output limit and caching for a model.
///
/// `options` are the composer's choices; `budget` the configured thinking budget.
pub fn plan_request(
    request: &mut ChatRequest,
    kind: ProviderKind,
    caps: &ModelCapabilities,
    max_output: Option<u32>,
    options: GenerateOptions,
    budget: u32,
) {
    let thinking_on = options.thinking;
    let effort = caps.effective_effort(options.effort);
    // Strata doesn't report vision support: let the server decide.
    request.images = caps.images || kind == ProviderKind::Strata;
    request.native_pdfs = kind == ProviderKind::Anthropic && caps.pdfs;
    if kind != ProviderKind::Anthropic {
        // OpenAI-compatible servers pick their own output limit.
        request.max_tokens = None;
        request.cache = false;
        request.effort = if kind == ProviderKind::OpenAi {
            effort
        } else {
            None
        };
        request.thinking = match (caps.thinking, thinking_on) {
            (ThinkingStyle::Switch, false) => Thinking::Off,
            _ => Thinking::Default,
        };
        return;
    }
    request.cache = true;
    request.effort = effort;
    let high_effort = matches!(effort, Some(Effort::Xhigh | Effort::Max));
    let model_max = max_output
        .unwrap_or(FALLBACK_MAX_OUTPUT)
        .max(MIN_THINKING_BUDGET * 2);
    let normal = model_max.min(if high_effort {
        HIGH_EFFORT_MAX_OUTPUT
    } else {
        REPLY_MAX_OUTPUT
    });
    let (thinking, max) = match (caps.thinking, thinking_on) {
        (ThinkingStyle::Adaptive, true) => (Thinking::Adaptive, normal),
        // Only models that accept it are told not to think; the others use their default.
        // Thinking can't be disabled at xhigh / max effort (400 on several models).
        (ThinkingStyle::Adaptive, false) if caps.thinking_off && !high_effort => {
            (Thinking::Off, normal)
        }
        (ThinkingStyle::Adaptive, false) => (Thinking::Default, normal),
        (ThinkingStyle::Budget, true) => {
            let max = model_max.min(normal.max(budget.saturating_add(ANSWER_ROOM)));
            let budget = budget.min(max.saturating_sub(ANSWER_ROOM.min(max / 2)));
            if budget < MIN_THINKING_BUDGET {
                (Thinking::Default, normal)
            } else {
                (Thinking::Budget(budget), max)
            }
        }
        _ => (Thinking::Default, normal),
    };
    request.thinking = thinking;
    request.max_tokens = Some(max);
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxide_core::{MediaSource, Message};
    use oxide_llm::ChatMessage;

    fn msg(role: Role, content: Vec<ContentBlock>) -> Message {
        Message {
            id: "m".into(),
            conversation_id: "c".into(),
            parent_id: None,
            role,
            content,
            model: None,
            created_at: 0,
            stop_reason: None,
            usage: None,
            feedback: None,
        }
    }

    #[test]
    fn sandbox_paragraph_follows_what_is_on() {
        let base = PromptContext {
            sandbox: Some(SandboxPrompt {
                image: "python:3.12-bookworm".into(),
                network: false,
                files: true,
                copies: true,
            }),
            ..Default::default()
        };
        let p = system_prompt(&base);
        assert!(p.contains("python:3.12-bookworm"), "{p}");
        assert!(p.contains("no network"), "{p}");
        assert!(p.contains("sandbox__write_file"), "{p}");
        assert!(p.contains("sandbox_copy_out"), "{p}");
        let p = system_prompt(&PromptContext {
            sandbox: Some(SandboxPrompt {
                network: true,
                copies: false,
                ..SandboxPrompt::default()
            }),
            ..Default::default()
        });
        assert!(p.contains("install packages"), "{p}");
        assert!(p.contains("No folder is shared"), "{p}");
        assert!(!system_prompt(&PromptContext::default()).contains("sandbox"));
    }

    #[test]
    fn browser_paragraph_only_when_on() {
        let p = system_prompt(&PromptContext {
            browser: true,
            ..Default::default()
        });
        assert!(
            p.contains("browser__navigate") && p.contains("browser__browser_task"),
            "{p}"
        );
        assert!(p.contains("Never type passwords"), "{p}");
        assert!(p.contains("find it with browser__search first"), "{p}");
        assert!(p.contains("never try to solve or get around it"), "{p}");
        assert!(p.contains("Google's AI Overview"), "{p}");
        assert!(!system_prompt(&PromptContext::default()).contains("browser__"));
    }

    #[test]
    fn history_drops_thinking_and_tools_and_merges_turns() {
        let thread = vec![
            msg(Role::Assistant, vec![ContentBlock::text("orphan greeting")]),
            msg(Role::User, vec![ContentBlock::text("first")]),
            msg(
                Role::User,
                vec![ContentBlock::Image {
                    media_type: "image/png".into(),
                    source: MediaSource::Base64 { data: "x".into() },
                }],
            ),
            msg(
                Role::Assistant,
                vec![
                    ContentBlock::Thinking {
                        text: "hmm".into(),
                        signature: Some("s".into()),
                        duration_ms: None,
                    },
                    ContentBlock::text("answer"),
                    ContentBlock::ToolUse {
                        id: "t".into(),
                        name: "x".into(),
                        input_json: "{}".into(),
                        server: None,
                        tool: None,
                    },
                ],
            ),
            msg(Role::Assistant, vec![ContentBlock::text("  ")]),
            msg(Role::User, vec![ContentBlock::text("second")]),
        ];
        let out = build_messages(&thread);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].role, Role::User);
        assert_eq!(out[0].content.len(), 2);
        assert_eq!(out[1].content, vec![ContentBlock::text("answer")]);
        assert_eq!(out[2].content, vec![ContentBlock::text("second")]);
    }

    fn use_(id: &str) -> ContentBlock {
        ContentBlock::ToolUse {
            id: id.into(),
            name: "t".into(),
            input_json: "{}".into(),
            server: None,
            tool: None,
        }
    }

    fn result(id: &str) -> ContentBlock {
        ContentBlock::ToolResult {
            tool_use_id: id.into(),
            content: vec![ContentBlock::text("out")],
            is_error: false,
        }
    }

    #[test]
    fn replies_with_tools_split_into_turns() {
        let thread = vec![
            msg(Role::User, vec![ContentBlock::text("q")]),
            msg(
                Role::Assistant,
                vec![
                    ContentBlock::text("looking"),
                    use_("a"),
                    use_("orphan"),
                    result("a"),
                    result("ghost"),
                    ContentBlock::ServerToolUse {
                        id: "s".into(),
                        name: "web_search".into(),
                        input_json: "{}".into(),
                    },
                    ContentBlock::text("found"),
                ],
            ),
            msg(Role::User, vec![ContentBlock::text("next")]),
        ];
        let out = build_messages(&thread);
        let roles: Vec<Role> = out.iter().map(|m| m.role).collect();
        assert_eq!(
            roles,
            vec![
                Role::User,
                Role::Assistant,
                Role::User,
                Role::Assistant,
                Role::User
            ]
        );
        assert_eq!(
            out[1].content,
            vec![ContentBlock::text("looking"), use_("a")]
        );
        assert_eq!(out[2].content, vec![result("a")]);
        assert_eq!(out[3].content, vec![ContentBlock::text("found")]);
    }

    #[test]
    fn a_stopped_reply_ending_in_results_merges_with_the_next_question() {
        let thread = vec![
            msg(Role::User, vec![ContentBlock::text("q")]),
            msg(Role::Assistant, vec![use_("a"), result("a")]),
            msg(Role::User, vec![ContentBlock::text("go on")]),
        ];
        let out = build_messages(&thread);
        assert_eq!(out.len(), 3);
        assert_eq!(
            out[2].content,
            vec![result("a"), ContentBlock::text("go on")]
        );
    }

    #[test]
    fn the_current_reply_keeps_signed_thinking_and_server_blocks() {
        let history = vec![ChatMessage::user("q")];
        let reply = vec![
            ContentBlock::Thinking {
                text: "plan".into(),
                signature: Some("sig".into()),
                duration_ms: None,
            },
            ContentBlock::Thinking {
                text: "local model thoughts".into(),
                signature: None,
                duration_ms: None,
            },
            use_("a"),
            result("a"),
            ContentBlock::ServerToolUse {
                id: "s".into(),
                name: "web_search".into(),
                input_json: "{}".into(),
            },
            ContentBlock::ServerToolResult {
                tool_use_id: "s".into(),
                raw_json: "{}".into(),
            },
        ];
        let out = request_messages(&history, &reply);
        assert_eq!(out.len(), 4);
        assert!(matches!(
            &out[1].content[0],
            ContentBlock::Thinking {
                signature: Some(_),
                ..
            }
        ));
        assert_eq!(out[1].content.len(), 2, "unsigned thinking is left out");
        assert_eq!(out[3].role, Role::Assistant);
        assert_eq!(out[3].content.len(), 2);
    }

    #[test]
    fn system_prompt_mentions_time_tools_and_project() {
        let p = system_prompt(&PromptContext {
            user_name: "Mikkel",
            now_ms: 1_793_992_217_000, // 2026-11-06T19:10:17Z
            utc_offset_minutes: Some(60),
            time_zone: Some("Europe/Copenhagen"),
            os: "windows",
            web_search: true,
            web_fetch: true,
            connectors: vec!["fs".into(), "time".into()],
            artifacts: false,
            project_name: None,
            project_instructions: Some("Answer in Danish."),
            knowledge_search: Vec::new(),
            memory: None,
            skills: None,
            ..PromptContext::default()
        });
        assert!(
            p.contains("Friday 2026-11-06, 20:10 (Europe/Copenhagen, UTC+01:00)"),
            "{p}"
        );
        assert!(p.contains("runs Windows"));
        assert!(p.contains("search the web, read web pages by URL, and use tools from these connectors: fs, time"), "{p}");
        assert!(p.contains("include the year (2026)"));
        assert!(p.contains("<project_instructions>\nAnswer in Danish.\n</project_instructions>"));
        assert!(p.ends_with("the language a message is written in is not such a request."));
        assert!(!p.contains("<memory>"));
        let neg = system_prompt(&PromptContext {
            utc_offset_minutes: Some(-330),
            ..PromptContext::default()
        });
        assert!(neg.contains("(UTC-05:30)"), "{neg}");
        assert!(!neg.contains("You can"));
    }

    #[test]
    fn system_prompt_carries_project_search_and_memory() {
        let p = system_prompt(&PromptContext {
            project_name: Some("Oxide"),
            knowledge_search: vec!["PLAN.md".into(), "PROGRESS.md".into()],
            memory: Some("- Prefers Danish"),
            ..PromptContext::default()
        });
        assert!(p.contains("project “Oxide”"), "{p}");
        assert!(
            p.contains("too large to include here: PLAN.md, PROGRESS.md."),
            "{p}"
        );
        assert!(p.contains("project_search"));
        assert!(p.contains("<memory>\n- Prefers Danish\n</memory>"));
        // Project instructions come after memory, at the very end.
        let with_both = system_prompt(&PromptContext {
            project_instructions: Some("Answer in Danish."),
            memory: Some("- Likes Rust"),
            ..PromptContext::default()
        });
        assert!(with_both.find("<memory>") < with_both.find("<project_instructions>"));
        let many = system_prompt(&PromptContext {
            knowledge_search: (0..60).map(|i| format!("f{i}")).collect(),
            ..PromptContext::default()
        });
        assert!(many.contains("f49 and 10 more."), "{many}");
        assert!(!system_prompt(&PromptContext::default()).contains("project_search"));
    }

    #[test]
    fn system_prompt_explains_artifacts_when_the_tool_is_on() {
        let p = system_prompt(&PromptContext {
            artifacts: true,
            ..PromptContext::default()
        });
        assert!(p.contains("You can make artifacts"), "{p}");
        assert!(
            p.contains("call the artifact tool again with its id"),
            "{p}"
        );
        assert!(!system_prompt(&PromptContext::default()).contains("artifact"));
    }

    #[test]
    fn system_prompt_lists_skills() {
        let p = system_prompt(&PromptContext {
            skills: Some("- haiku: Use for haiku requests"),
            project_instructions: Some("Be brief."),
            ..PromptContext::default()
        });
        assert!(
            p.contains("<skills>\n- haiku: Use for haiku requests\n</skills>"),
            "{p}"
        );
        assert!(p.contains("call load_skill with its name"));
        assert!(p.find("<skills>") < p.find("<project_instructions>"));
        assert!(!system_prompt(&PromptContext::default()).contains("load_skill"));
    }

    #[test]
    fn dates_are_utc_calendar_dates() {
        assert_eq!(civil_date(0), (1970, 1, 1));
        // 2026-11-05T12:00:00Z
        assert_eq!(civil_date(1_793_880_000_000), (2026, 11, 5));
        // 2024-02-29
        assert_eq!(civil_date(1_709_164_800_000), (2024, 2, 29));
        let ctx = |name| PromptContext {
            user_name: name,
            ..PromptContext::default()
        };
        let p = system_prompt(&ctx(" Ada "));
        assert!(p.contains("Thursday 1970-01-01, 00:00 (UTC)"), "{p}");
        assert!(p.ends_with("The user's name is Ada."), "{p}");
        assert!(!system_prompt(&ctx("")).contains("name is"));
        assert!(system_prompt(&ctx("")).contains("do all of it"));
    }

    fn on() -> GenerateOptions {
        GenerateOptions {
            thinking: true,
            effort: None,
        }
    }

    fn off() -> GenerateOptions {
        GenerateOptions::default()
    }

    #[test]
    fn anthropic_thinking_and_limits() {
        let mut req = ChatRequest::new("m", vec![]);
        let budget = ModelCapabilities {
            thinking: ThinkingStyle::Budget,
            ..ModelCapabilities::default()
        };
        plan_request(
            &mut req,
            ProviderKind::Anthropic,
            &budget,
            Some(64_000),
            on(),
            8_000,
        );
        assert_eq!(req.thinking, Thinking::Budget(8_000));
        assert_eq!(req.max_tokens, Some(32_000));
        assert!(req.cache);

        // A big budget raises the limit up to the model's maximum.
        plan_request(
            &mut req,
            ProviderKind::Anthropic,
            &budget,
            Some(64_000),
            on(),
            60_000,
        );
        assert_eq!(req.max_tokens, Some(64_000));
        assert_eq!(req.thinking, Thinking::Budget(59_904));

        // Small models: the budget shrinks to leave room for the answer.
        plan_request(
            &mut req,
            ProviderKind::Anthropic,
            &budget,
            Some(8_192),
            on(),
            8_000,
        );
        assert_eq!(req.max_tokens, Some(8_192));
        assert_eq!(req.thinking, Thinking::Budget(4_096));

        plan_request(
            &mut req,
            ProviderKind::Anthropic,
            &budget,
            None,
            off(),
            8_000,
        );
        assert_eq!(req.thinking, Thinking::Default);

        let always_thinks = ModelCapabilities {
            thinking: ThinkingStyle::Adaptive,
            ..ModelCapabilities::default()
        };
        plan_request(
            &mut req,
            ProviderKind::Anthropic,
            &always_thinks,
            Some(128_000),
            off(),
            8_000,
        );
        assert_eq!(
            req.thinking,
            Thinking::Default,
            "no `disabled` for always-on thinking"
        );

        let adaptive = ModelCapabilities {
            thinking: ThinkingStyle::Adaptive,
            thinking_off: true,
            ..ModelCapabilities::default()
        };
        plan_request(
            &mut req,
            ProviderKind::Anthropic,
            &adaptive,
            Some(128_000),
            off(),
            8_000,
        );
        assert_eq!(req.thinking, Thinking::Off);
        plan_request(
            &mut req,
            ProviderKind::Anthropic,
            &adaptive,
            Some(128_000),
            on(),
            8_000,
        );
        assert_eq!(req.thinking, Thinking::Adaptive);
        assert_eq!(req.max_tokens, Some(32_000));
    }

    #[test]
    fn openai_compatible_requests_use_server_defaults() {
        let mut req = ChatRequest::new("m", vec![]);
        req.max_tokens = Some(5);
        plan_request(
            &mut req,
            ProviderKind::LmStudio,
            &ModelCapabilities::default(),
            Some(9),
            on(),
            8_000,
        );
        assert_eq!(req.max_tokens, None);
        assert_eq!(req.thinking, Thinking::Default);
        assert!(!req.cache);
        assert!(!req.images && !req.native_pdfs);
    }

    #[test]
    fn effort_media_and_local_thinking_switch() {
        let mut req = ChatRequest::new("m", vec![]);
        let opus = ModelCapabilities {
            thinking: ThinkingStyle::Adaptive,
            thinking_off: true,
            images: true,
            pdfs: true,
            effort: Effort::ALL.to_vec(),
            default_effort: Some(Effort::Medium),
            ..ModelCapabilities::default()
        };
        let max = GenerateOptions {
            thinking: false,
            effort: Some(Effort::Max),
        };
        plan_request(
            &mut req,
            ProviderKind::Anthropic,
            &opus,
            Some(128_000),
            max,
            8_000,
        );
        assert_eq!(req.effort, Some(Effort::Max));
        // No `disabled` at max effort, and a larger output limit.
        assert_eq!(req.thinking, Thinking::Default);
        assert_eq!(req.max_tokens, Some(64_000));
        assert!(req.images && req.native_pdfs);

        let low = GenerateOptions {
            thinking: false,
            effort: Some(Effort::Low),
        };
        plan_request(
            &mut req,
            ProviderKind::Anthropic,
            &opus,
            Some(128_000),
            low,
            8_000,
        );
        assert_eq!(req.effort, Some(Effort::Low));
        assert_eq!(req.thinking, Thinking::Off);
        assert_eq!(req.max_tokens, Some(32_000));

        // Models without effort control get none.
        plan_request(
            &mut req,
            ProviderKind::Anthropic,
            &ModelCapabilities::default(),
            None,
            low,
            8_000,
        );
        assert_eq!(req.effort, None);

        // OpenAI reasoning models: nearest supported level.
        let o4 = ModelCapabilities {
            effort: vec![Effort::Low, Effort::Medium, Effort::High],
            ..ModelCapabilities::default()
        };
        plan_request(&mut req, ProviderKind::OpenAi, &o4, None, max, 8_000);
        assert_eq!(req.effort, Some(Effort::High));
        assert!(!req.native_pdfs);

        // Strata: thinking off through the switch, images left to the server.
        let local = ModelCapabilities {
            thinking: ThinkingStyle::Switch,
            thinking_off: true,
            ..ModelCapabilities::default()
        };
        plan_request(&mut req, ProviderKind::Strata, &local, None, max, 8_000);
        assert_eq!(req.thinking, Thinking::Off);
        assert_eq!(req.effort, None);
        assert!(req.images);
        plan_request(&mut req, ProviderKind::Strata, &local, None, on(), 8_000);
        assert_eq!(req.thinking, Thinking::Default);
    }
}
