//! The `task_list` tool: the model keeps a list of the steps of longer work, shown live in
//! the session panel's "Progress" (Phase 11). Each call replaces the whole list.

use oxide_core::workspace::{MAX_TASKS, TASK_LIST, clean_task_title};
use oxide_core::{TaskItem, TaskList, TaskStatus};
use serde_json::{Value, json};

use crate::tools::{Builtin, ToolEntry, ToolKind};

/// The tool as models see it.
pub fn task_list_entry() -> ToolEntry {
    ToolEntry {
        api_name: TASK_LIST.into(),
        kind: ToolKind::Builtin(Builtin::TaskList),
        title: Some("Task list".into()),
        description: "Plan and track work that takes several steps. Send the whole list each \
                      time: every step with its status (pending, in_progress or done). Make \
                      the list before you start, mark a step in_progress when you begin it \
                      and done as soon as it's finished; keep one step in_progress at a time. \
                      The user sees the list live. Skip it for simple questions."
            .into(),
        schema: json!({"type": "object", "properties": {
            "tasks": {"type": "array", "maxItems": MAX_TASKS, "items": {
                "type": "object",
                "properties": {
                    "title": {"type": "string", "description": "What the step does, briefly."},
                    "status": {"type": "string", "enum": ["pending", "in_progress", "done"]}
                },
                "required": ["title", "status"]
            }}
        }, "required": ["tasks"]}),
        read_only: true,
        destructive: false,
    }
}

/// Reads the list a call sends. Accepts what models commonly send instead: a list of
/// strings, `content`/`name`/`step` for the title, `completed: true`, statuses like
/// `in progress` or `completed`.
pub fn parse_tasks(input: &Value) -> Result<Vec<TaskItem>, String> {
    let list = input
        .get("tasks")
        .or_else(|| input.get("todos"))
        .or_else(|| input.get("items"))
        .or_else(|| input.get("steps"));
    let list = match list {
        Some(Value::Array(a)) => a.clone(),
        // Some models send the array as a JSON string.
        Some(Value::String(s)) => match serde_json::from_str::<Value>(s) {
            Ok(Value::Array(a)) => a,
            _ => return Err("task_list needs \"tasks\": an array of {title, status}.".into()),
        },
        _ => return Err("task_list needs \"tasks\": an array of {title, status}.".into()),
    };
    if list.len() > MAX_TASKS {
        return Err(format!("A task list can have at most {MAX_TASKS} steps."));
    }
    let mut items = Vec::new();
    for (i, t) in list.iter().enumerate() {
        let (title, status) = match t {
            Value::String(s) => (s.clone(), TaskStatus::Pending),
            Value::Object(o) => {
                let title = ["title", "content", "name", "step", "task", "description"]
                    .iter()
                    .find_map(|k| o.get(*k).and_then(Value::as_str))
                    .unwrap_or("")
                    .to_owned();
                let status = match o.get("status").and_then(Value::as_str) {
                    Some(s) => TaskStatus::parse(s).ok_or_else(|| {
                        format!(
                            "Step {} has an unknown status “{s}”; use pending, in_progress or \
                             done.",
                            i + 1
                        )
                    })?,
                    None if o.get("completed").and_then(Value::as_bool) == Some(true)
                        || o.get("done").and_then(Value::as_bool) == Some(true) =>
                    {
                        TaskStatus::Done
                    }
                    None => TaskStatus::Pending,
                };
                (title, status)
            }
            _ => return Err(format!("Step {} isn't an object with a title.", i + 1)),
        };
        let title =
            clean_task_title(&title).ok_or_else(|| format!("Step {} has no title.", i + 1))?;
        items.push(TaskItem { title, status });
    }
    Ok(items)
}

/// What the model is told after a list is saved.
pub fn summary(list: &TaskList) -> String {
    let (done, total) = list.progress();
    if total == 0 {
        return "The task list is cleared.".into();
    }
    match list.current() {
        Some((i, t)) => format!(
            "Task list saved: {done} of {total} done. Now: step {} “{}”.",
            i + 1,
            t.title
        ),
        None => format!("Task list saved: every step is done ({total} of {total})."),
    }
}

/// The usage-log label: the step being worked on, or how far the list is.
pub fn label(items: &[TaskItem]) -> Option<String> {
    let list = TaskList {
        items: items.to_vec(),
        ..TaskList::default()
    };
    let (done, total) = list.progress();
    if total == 0 {
        return None;
    }
    Some(match list.current() {
        Some((_, t)) => t.title.clone(),
        None => format!("{done} of {total} done"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_parse_from_what_models_send() {
        let items = parse_tasks(&json!({"tasks": [
            {"title": "Look at the folder", "status": "done"},
            {"content": "Sort files", "status": "in progress"},
            {"title": "Write summary", "status": "pending"},
            {"title": "Check", "completed": true},
            "Tell the user"
        ]}))
        .expect("parses");
        let statuses: Vec<TaskStatus> = items.iter().map(|t| t.status).collect();
        assert_eq!(
            statuses,
            vec![
                TaskStatus::Done,
                TaskStatus::InProgress,
                TaskStatus::Pending,
                TaskStatus::Done,
                TaskStatus::Pending
            ]
        );
        assert_eq!(items[1].title, "Sort files");
        let as_string = parse_tasks(&json!({"todos": "[{\"title\":\"a\",\"status\":\"done\"}]"}))
            .expect("string");
        assert_eq!(as_string.len(), 1);
        assert!(parse_tasks(&json!({})).is_err());
        assert!(parse_tasks(&json!({"tasks": [{"title": "", "status": "done"}]})).is_err());
        assert!(parse_tasks(&json!({"tasks": [{"title": "a", "status": "later"}]})).is_err());
        assert!(parse_tasks(&json!({"tasks": vec!["x"; MAX_TASKS + 1]})).is_err());
        assert!(
            parse_tasks(&json!({"tasks": []}))
                .expect("empty")
                .is_empty()
        );
    }

    #[test]
    fn summaries_name_the_current_step() {
        let items = parse_tasks(&json!({"tasks": [
            {"title": "A", "status": "done"},
            {"title": "B", "status": "in_progress"}
        ]}))
        .expect("parses");
        let list = TaskList {
            items: items.clone(),
            ..TaskList::default()
        };
        assert_eq!(
            summary(&list),
            "Task list saved: 1 of 2 done. Now: step 2 “B”."
        );
        assert_eq!(label(&items).as_deref(), Some("B"));
        let done = TaskList {
            items: vec![TaskItem {
                title: "A".into(),
                status: TaskStatus::Done,
            }],
            ..TaskList::default()
        };
        assert_eq!(
            summary(&done),
            "Task list saved: every step is done (1 of 1)."
        );
        assert_eq!(summary(&TaskList::default()), "The task list is cleared.");
    }
}
