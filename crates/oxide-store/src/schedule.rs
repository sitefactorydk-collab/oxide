//! Scheduled tasks and their runs (Phase 15).
//!
//! The store only keeps them: [`TaskInput`]s arrive already checked
//! ([`TaskInput::clean`]), and the scheduler in `oxide-agent` decides when tasks run.

use oxide_core::{
    ApprovalMode, ChatTools, MAX_RUN_OUTPUT_CHARS, MissedRunPolicy, RunStatus, RunTrigger,
    ScheduledTask, TaskInput, TaskRun, TaskSchedule, Timestamp,
};

use crate::{Result, Store, StoreError, new_id, now_ms};

type TaskRow = (
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    String,
    String,
    i64,
    String,
    String,
    i64,
    Option<i64>,
    i64,
    i64,
);

const TASK_COLUMNS: &str = "id, name, prompt, schedule, model, project_id, folders, tools, \
                            thinking, approval, missed, enabled, next_run_at, created_at, \
                            updated_at";

type RunRow = (
    String,
    String,
    Option<String>,
    String,
    Option<i64>,
    i64,
    Option<i64>,
    String,
    Option<String>,
    Option<String>,
);

const RUN_COLUMNS: &str = "id, task_id, conversation_id, trigger, scheduled_for, started_at, \
                           finished_at, status, output, error";

fn corrupt(e: serde_json::Error) -> StoreError {
    StoreError::Corrupt(e.to_string())
}

fn task_from_row(r: TaskRow) -> Result<ScheduledTask> {
    let (
        id,
        name,
        prompt,
        schedule,
        model,
        project_id,
        folders,
        tools,
        thinking,
        approval,
        missed,
        enabled,
        next_run_at,
        created_at,
        updated_at,
    ) = r;
    let schedule: TaskSchedule = serde_json::from_str(&schedule).map_err(corrupt)?;
    Ok(ScheduledTask {
        id,
        name,
        prompt,
        schedule,
        model,
        project_id,
        folders: serde_json::from_str(&folders).unwrap_or_default(),
        tools: serde_json::from_str::<ChatTools>(&tools).unwrap_or_default(),
        thinking: thinking != 0,
        approval: ApprovalMode::parse(&approval),
        missed: MissedRunPolicy::parse(&missed),
        enabled: enabled != 0,
        next_run_at,
        last_run: None,
        created_at,
        updated_at,
    })
}

fn run_from_row(r: RunRow) -> TaskRun {
    let (
        id,
        task_id,
        conversation_id,
        trigger,
        scheduled_for,
        started_at,
        finished_at,
        status,
        output,
        error,
    ) = r;
    TaskRun {
        id,
        task_id,
        conversation_id,
        trigger: RunTrigger::parse(&trigger),
        scheduled_for,
        started_at,
        finished_at,
        status: RunStatus::parse(&status),
        output,
        error,
    }
}

/// Text cut to `max` characters, with an ellipsis.
fn cap(text: Option<String>, max: usize) -> Option<String> {
    text.map(|t| {
        if t.chars().count() > max {
            let cut: String = t.chars().take(max.saturating_sub(1)).collect();
            format!("{}…", cut.trim_end())
        } else {
            t
        }
    })
}

impl Store {
    /// Every task, by name, each with its latest run.
    pub async fn list_tasks(&self) -> Result<Vec<ScheduledTask>> {
        let rows: Vec<TaskRow> = sqlx::query_as(&format!(
            "SELECT {TASK_COLUMNS} FROM scheduled_tasks ORDER BY name COLLATE NOCASE, created_at"
        ))
        .fetch_all(&self.pool)
        .await?;
        let mut out = Vec::with_capacity(rows.len());
        for r in rows {
            let mut t = task_from_row(r)?;
            t.last_run = self.task_runs(&t.id, 1).await?.into_iter().next();
            out.push(t);
        }
        Ok(out)
    }

    pub async fn get_task(&self, id: &str) -> Result<ScheduledTask> {
        let row: Option<TaskRow> = sqlx::query_as(&format!(
            "SELECT {TASK_COLUMNS} FROM scheduled_tasks WHERE id = ?"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        let mut t = task_from_row(row.ok_or(StoreError::NotFound { what: "task" })?)?;
        t.last_run = self.task_runs(&t.id, 1).await?.into_iter().next();
        Ok(t)
    }

    pub async fn count_tasks(&self) -> Result<usize> {
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM scheduled_tasks")
            .fetch_one(&self.pool)
            .await?;
        Ok(usize::try_from(n).unwrap_or(0))
    }

    /// Saves a new task (checked by the caller) with its first run time.
    pub async fn create_task(
        &self,
        input: &TaskInput,
        next_run_at: Option<Timestamp>,
    ) -> Result<ScheduledTask> {
        let id = new_id();
        let now = now_ms();
        sqlx::query(&format!(
            "INSERT INTO scheduled_tasks ({TASK_COLUMNS}) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        ))
        .bind(&id)
        .bind(&input.name)
        .bind(&input.prompt)
        .bind(serde_json::to_string(&input.schedule).map_err(corrupt)?)
        .bind(&input.model)
        .bind(&input.project_id)
        .bind(serde_json::to_string(&input.folders).map_err(corrupt)?)
        .bind(serde_json::to_string(&input.tools).map_err(corrupt)?)
        .bind(i64::from(input.thinking))
        .bind(input.approval.as_str())
        .bind(input.missed.as_str())
        .bind(i64::from(input.enabled))
        .bind(next_run_at)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        self.get_task(&id).await
    }

    /// Replaces a task's settings and its next run time.
    pub async fn update_task(
        &self,
        id: &str,
        input: &TaskInput,
        next_run_at: Option<Timestamp>,
    ) -> Result<ScheduledTask> {
        let done = sqlx::query(
            "UPDATE scheduled_tasks SET name = ?, prompt = ?, schedule = ?, model = ?, \
             project_id = ?, folders = ?, tools = ?, thinking = ?, approval = ?, missed = ?, \
             enabled = ?, next_run_at = ?, updated_at = ? WHERE id = ?",
        )
        .bind(&input.name)
        .bind(&input.prompt)
        .bind(serde_json::to_string(&input.schedule).map_err(corrupt)?)
        .bind(&input.model)
        .bind(&input.project_id)
        .bind(serde_json::to_string(&input.folders).map_err(corrupt)?)
        .bind(serde_json::to_string(&input.tools).map_err(corrupt)?)
        .bind(i64::from(input.thinking))
        .bind(input.approval.as_str())
        .bind(input.missed.as_str())
        .bind(i64::from(input.enabled))
        .bind(next_run_at)
        .bind(now_ms())
        .bind(id)
        .execute(&self.pool)
        .await?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound { what: "task" });
        }
        self.get_task(id).await
    }

    /// Switches a task on or off and sets when it runs next.
    pub async fn set_task_schedule_state(
        &self,
        id: &str,
        enabled: bool,
        next_run_at: Option<Timestamp>,
    ) -> Result<()> {
        let done =
            sqlx::query("UPDATE scheduled_tasks SET enabled = ?, next_run_at = ? WHERE id = ?")
                .bind(i64::from(enabled))
                .bind(next_run_at)
                .bind(id)
                .execute(&self.pool)
                .await?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound { what: "task" });
        }
        Ok(())
    }

    /// Deletes a task and its history (the runs' chats stay).
    pub async fn delete_task(&self, id: &str) -> Result<()> {
        let done = sqlx::query("DELETE FROM scheduled_tasks WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound { what: "task" });
        }
        Ok(())
    }

    /// Enabled tasks whose next run is at or before `now`, earliest first.
    pub async fn due_tasks(&self, now: Timestamp) -> Result<Vec<ScheduledTask>> {
        let rows: Vec<TaskRow> = sqlx::query_as(&format!(
            "SELECT {TASK_COLUMNS} FROM scheduled_tasks \
             WHERE enabled = 1 AND next_run_at IS NOT NULL AND next_run_at <= ? \
             ORDER BY next_run_at"
        ))
        .bind(now)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(task_from_row).collect()
    }

    /// The earliest next run of any enabled task.
    pub async fn earliest_next_run(&self) -> Result<Option<Timestamp>> {
        let (t,): (Option<i64>,) =
            sqlx::query_as("SELECT MIN(next_run_at) FROM scheduled_tasks WHERE enabled = 1")
                .fetch_one(&self.pool)
                .await?;
        Ok(t)
    }

    /// Records a run (its output is shortened).
    pub async fn add_task_run(&self, run: &TaskRun) -> Result<TaskRun> {
        let output = cap(run.output.clone(), MAX_RUN_OUTPUT_CHARS);
        let error = cap(run.error.clone(), 2_000);
        sqlx::query(&format!(
            "INSERT INTO task_runs ({RUN_COLUMNS}) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
        ))
        .bind(&run.id)
        .bind(&run.task_id)
        .bind(&run.conversation_id)
        .bind(run.trigger.as_str())
        .bind(run.scheduled_for)
        .bind(run.started_at)
        .bind(run.finished_at)
        .bind(run.status.as_str())
        .bind(&output)
        .bind(&error)
        .execute(&self.pool)
        .await?;
        Ok(TaskRun {
            output,
            error,
            ..run.clone()
        })
    }

    /// Saves a run's progress: status, end, output and error. (Its chat is set when it's
    /// added; deleting the chat clears it.)
    pub async fn update_task_run(&self, run: &TaskRun) -> Result<TaskRun> {
        let output = cap(run.output.clone(), MAX_RUN_OUTPUT_CHARS);
        let error = cap(run.error.clone(), 2_000);
        let done = sqlx::query(
            "UPDATE task_runs SET status = ?, finished_at = ?, output = ?, error = ? WHERE id = ?",
        )
        .bind(run.status.as_str())
        .bind(run.finished_at)
        .bind(&output)
        .bind(&error)
        .bind(&run.id)
        .execute(&self.pool)
        .await?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound { what: "run" });
        }
        self.get_task_run(&run.id).await
    }

    pub async fn get_task_run(&self, id: &str) -> Result<TaskRun> {
        let row: Option<RunRow> =
            sqlx::query_as(&format!("SELECT {RUN_COLUMNS} FROM task_runs WHERE id = ?"))
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(run_from_row(
            row.ok_or(StoreError::NotFound { what: "run" })?,
        ))
    }

    /// A task's runs, newest first.
    pub async fn task_runs(&self, task_id: &str, limit: usize) -> Result<Vec<TaskRun>> {
        let rows: Vec<RunRow> = sqlx::query_as(&format!(
            "SELECT {RUN_COLUMNS} FROM task_runs WHERE task_id = ? ORDER BY seq DESC LIMIT ?"
        ))
        .bind(task_id)
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(run_from_row).collect())
    }

    /// Runs still marked running or waiting (after a restart: ones Oxide was closed during).
    pub async fn active_task_runs(&self) -> Result<Vec<TaskRun>> {
        let rows: Vec<RunRow> = sqlx::query_as(&format!(
            "SELECT {RUN_COLUMNS} FROM task_runs \
             WHERE status IN ('running', 'waitingApproval') ORDER BY seq"
        ))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(run_from_row).collect())
    }

    /// The run that wrote in a chat, if any.
    pub async fn task_run_for_chat(&self, conversation_id: &str) -> Result<Option<TaskRun>> {
        let row: Option<RunRow> = sqlx::query_as(&format!(
            "SELECT {RUN_COLUMNS} FROM task_runs WHERE conversation_id = ? \
             ORDER BY seq DESC LIMIT 1"
        ))
        .bind(conversation_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(run_from_row))
    }

    /// Keeps a task's newest `keep` runs (older rows go; their chats stay).
    pub async fn prune_task_runs(&self, task_id: &str, keep: usize) -> Result<()> {
        sqlx::query(
            "DELETE FROM task_runs WHERE task_id = ? AND seq NOT IN \
             (SELECT seq FROM task_runs WHERE task_id = ? ORDER BY seq DESC LIMIT ?)",
        )
        .bind(task_id)
        .bind(task_id)
        .bind(i64::try_from(keep).unwrap_or(i64::MAX))
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use oxide_core::{StartConversation, TaskSchedule};

    use super::*;
    use crate::test_util::store;

    fn input(name: &str) -> TaskInput {
        TaskInput {
            name: name.into(),
            prompt: "Summarize the day".into(),
            schedule: TaskSchedule::Cron {
                expr: "0 9 * * *".into(),
            },
            model: "lmstudio/qwen".into(),
            project_id: None,
            folders: vec!["C:\\work".into()],
            tools: ChatTools {
                disabled_builtins: vec!["web_fetch".into()],
                ..ChatTools::default()
            },
            thinking: true,
            approval: ApprovalMode::Auto,
            missed: MissedRunPolicy::Skip,
            enabled: true,
        }
    }

    fn run(task_id: &str, status: RunStatus) -> TaskRun {
        TaskRun {
            id: new_id(),
            task_id: task_id.into(),
            conversation_id: None,
            trigger: RunTrigger::Schedule,
            scheduled_for: Some(1_000),
            started_at: now_ms(),
            finished_at: None,
            status,
            output: None,
            error: None,
        }
    }

    #[tokio::test]
    async fn tasks_round_trip() {
        let s = store().await;
        let t = s
            .create_task(&input("Brief"), Some(5_000))
            .await
            .expect("creates");
        assert_eq!(t.input(), input("Brief"));
        assert_eq!(t.next_run_at, Some(5_000));
        assert_eq!(s.count_tasks().await.expect("counts"), 1);

        let mut changed = input("Brief 2");
        changed.schedule = TaskSchedule::Once { at: 9_000 };
        changed.approval = ApprovalMode::Ask;
        let u = s
            .update_task(&t.id, &changed, Some(9_000))
            .await
            .expect("updates");
        assert_eq!(u.input(), changed);

        s.set_task_schedule_state(&t.id, false, None)
            .await
            .expect("pauses");
        let p = s.get_task(&t.id).await.expect("loads");
        assert!(!p.enabled);
        assert_eq!(p.next_run_at, None);

        s.create_task(&input("another"), Some(1))
            .await
            .expect("creates");
        let names: Vec<String> = s
            .list_tasks()
            .await
            .expect("lists")
            .into_iter()
            .map(|t| t.name)
            .collect();
        assert_eq!(names, vec!["another", "Brief 2"]);

        s.delete_task(&t.id).await.expect("deletes");
        assert!(matches!(
            s.get_task(&t.id).await,
            Err(StoreError::NotFound { .. })
        ));
        assert!(s.delete_task(&t.id).await.is_err());
    }

    #[tokio::test]
    async fn due_tasks_are_enabled_and_past_their_time() {
        let s = store().await;
        let a = s.create_task(&input("a"), Some(100)).await.expect("a");
        let b = s.create_task(&input("b"), Some(50)).await.expect("b");
        let c = s.create_task(&input("c"), Some(500)).await.expect("c");
        let mut off = input("d");
        off.enabled = false;
        s.create_task(&off, Some(10)).await.expect("d");
        let due: Vec<String> = s
            .due_tasks(100)
            .await
            .expect("due")
            .into_iter()
            .map(|t| t.id)
            .collect();
        assert_eq!(due, vec![b.id, a.id]);
        assert_eq!(s.earliest_next_run().await.expect("earliest"), Some(50));
        let _ = c;
    }

    #[tokio::test]
    async fn runs_keep_history_and_go_with_their_chat() {
        let s = store().await;
        let t = s.create_task(&input("t"), None).await.expect("task");
        let (chat, _) = s
            .start_conversation(StartConversation {
                text: "Summarize the day".into(),
                model: "lmstudio/qwen".into(),
                project_id: None,
                attachments: vec![],
                folders: vec![],
                tools: None,
            })
            .await
            .expect("chat");
        let mut r = run(&t.id, RunStatus::Running);
        r.conversation_id = Some(chat.id.clone());
        s.add_task_run(&r).await.expect("adds");
        assert_eq!(s.active_task_runs().await.expect("active").len(), 1);
        assert_eq!(
            s.task_run_for_chat(&chat.id)
                .await
                .expect("by chat")
                .map(|r| r.id),
            Some(r.id.clone())
        );

        r.status = RunStatus::Done;
        r.finished_at = Some(now_ms());
        r.output = Some("x".repeat(MAX_RUN_OUTPUT_CHARS + 50));
        let saved = s.update_task_run(&r).await.expect("updates");
        assert_eq!(
            saved.output.as_deref().map(|o| o.chars().count()),
            Some(MAX_RUN_OUTPUT_CHARS)
        );
        assert!(s.active_task_runs().await.expect("active").is_empty());
        assert_eq!(
            s.get_task(&t.id)
                .await
                .expect("task")
                .last_run
                .map(|r| r.id),
            Some(r.id.clone())
        );

        // Deleting the run's chat takes the run out of the history; other runs stay.
        let other = s
            .add_task_run(&run(&t.id, RunStatus::Missed))
            .await
            .expect("adds");
        s.delete_conversation(&chat.id).await.expect("deletes chat");
        assert!(matches!(
            s.get_task_run(&r.id).await,
            Err(StoreError::NotFound { .. })
        ));
        assert!(s.get_task_run(&other.id).await.is_ok());
        assert!(
            s.task_run_for_chat(&chat.id)
                .await
                .expect("by chat")
                .is_none()
        );

        for _ in 0..5 {
            s.add_task_run(&run(&t.id, RunStatus::Missed))
                .await
                .expect("adds");
        }
        s.prune_task_runs(&t.id, 3).await.expect("prunes");
        let left = s.task_runs(&t.id, 10).await.expect("runs");
        assert_eq!(left.len(), 3);
        assert!(left.iter().all(|r| r.status == RunStatus::Missed));

        s.delete_task(&t.id).await.expect("deletes");
        assert!(s.task_runs(&t.id, 10).await.expect("runs").is_empty());
    }
}
