//! Project repository (instructions and chats; knowledge files arrive in Phase 7).

use oxide_core::{NewProject, Project, ProjectPatch};

use crate::{Result, Store, StoreError, new_id, now_ms};

#[derive(sqlx::FromRow)]
struct ProjectRow {
    id: String,
    name: String,
    description: String,
    instructions: String,
    created_at: i64,
    updated_at: i64,
}

impl From<ProjectRow> for Project {
    fn from(r: ProjectRow) -> Self {
        Project {
            id: r.id,
            name: r.name,
            description: r.description,
            instructions: r.instructions,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

const COLUMNS: &str = "id, name, description, instructions, created_at, updated_at";

fn clean_name(name: &str) -> Result<String> {
    let n = name.trim();
    if n.is_empty() {
        return Err(StoreError::Invalid("project name is empty".into()));
    }
    Ok(n.chars().take(120).collect())
}

impl Store {
    /// All projects, most recently updated first.
    pub async fn list_projects(&self) -> Result<Vec<Project>> {
        let rows: Vec<ProjectRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM projects ORDER BY updated_at DESC, name"
        ))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn get_project(&self, id: &str) -> Result<Project> {
        let row: Option<ProjectRow> =
            sqlx::query_as(&format!("SELECT {COLUMNS} FROM projects WHERE id = ?"))
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        row.map(Into::into)
            .ok_or(StoreError::NotFound { what: "project" })
    }

    pub async fn create_project(&self, input: NewProject) -> Result<Project> {
        let now = now_ms();
        let project = Project {
            id: new_id(),
            name: clean_name(&input.name)?,
            description: input.description.trim().to_owned(),
            instructions: String::new(),
            created_at: now,
            updated_at: now,
        };
        sqlx::query(
            "INSERT INTO projects (id, name, description, instructions, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&project.id)
        .bind(&project.name)
        .bind(&project.description)
        .bind(&project.instructions)
        .bind(project.created_at)
        .bind(project.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(project)
    }

    pub async fn update_project(&self, id: &str, patch: ProjectPatch) -> Result<Project> {
        let mut p = self.get_project(id).await?;
        if let Some(name) = patch.name {
            p.name = clean_name(&name)?;
        }
        if let Some(d) = patch.description {
            p.description = d.trim().to_owned();
        }
        if let Some(i) = patch.instructions {
            p.instructions = i.trim().to_owned();
        }
        p.updated_at = now_ms();
        sqlx::query(
            "UPDATE projects SET name = ?, description = ?, instructions = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(&p.name)
        .bind(&p.description)
        .bind(&p.instructions)
        .bind(p.updated_at)
        .bind(&p.id)
        .execute(&self.pool)
        .await?;
        Ok(p)
    }

    /// Deletes a project. Its chats stay, detached from the project.
    pub async fn delete_project(&self, id: &str) -> Result<()> {
        let done = sqlx::query("DELETE FROM projects WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        if done.rows_affected() == 0 {
            return Err(StoreError::NotFound { what: "project" });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::store;
    use oxide_core::StartConversation;

    #[tokio::test]
    async fn create_update_delete() {
        let s = store().await;
        let p = s
            .create_project(NewProject {
                name: " Oxide ".into(),
                description: "Rust app".into(),
            })
            .await
            .expect("creates");
        assert_eq!(p.name, "Oxide");
        let p2 = s
            .update_project(
                &p.id,
                ProjectPatch {
                    instructions: Some("Be terse.".into()),
                    ..ProjectPatch::default()
                },
            )
            .await
            .expect("updates");
        assert_eq!(p2.instructions, "Be terse.");
        assert_eq!(p2.name, "Oxide");
        assert_eq!(s.list_projects().await.expect("ok"), vec![p2]);
        assert!(matches!(
            s.create_project(NewProject {
                name: " ".into(),
                description: String::new()
            })
            .await,
            Err(StoreError::Invalid(_))
        ));
    }

    #[tokio::test]
    async fn deleting_a_project_keeps_its_chats() {
        let s = store().await;
        let p = s
            .create_project(NewProject {
                name: "P".into(),
                description: String::new(),
            })
            .await
            .expect("creates");
        let (c, _) = s
            .start_conversation(StartConversation {
                text: "in project".into(),
                model: "m".into(),
                project_id: Some(p.id.clone()),
                attachments: Vec::new(),
            })
            .await
            .expect("creates");
        assert_eq!(c.project_id.as_deref(), Some(p.id.as_str()));
        s.delete_project(&p.id).await.expect("deletes");
        assert_eq!(
            s.get_conversation(&c.id).await.expect("ok").project_id,
            None
        );
    }
}
