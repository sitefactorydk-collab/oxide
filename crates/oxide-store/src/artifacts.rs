//! Artifacts and their versions (Phase 8).

use oxide_core::artifact::MAX_ARTIFACT_CHARS;
use oxide_core::{Artifact, ArtifactDetail, ArtifactKind, ArtifactSummary, ArtifactVersion};

use crate::conversations::map_fk;
use crate::{Result, Store, StoreError, new_id, now_ms};

/// Characters of the newest version the gallery shows.
const EXCERPT_CHARS: usize = 160;

/// A version to save (see [`Store::save_artifact_version`]).
#[derive(Debug, Clone)]
pub struct NewArtifactVersion {
    pub conversation_id: String,
    pub key: String,
    pub kind: ArtifactKind,
    pub title: String,
    pub language: Option<String>,
    pub content: String,
    pub message_id: Option<String>,
}

#[derive(sqlx::FromRow)]
struct ArtifactRow {
    id: String,
    conversation_id: String,
    key: String,
    kind: String,
    title: String,
    language: Option<String>,
    version: i64,
    created_at: i64,
    updated_at: i64,
}

fn kind(s: &str) -> Result<ArtifactKind> {
    ArtifactKind::parse(s).ok_or_else(|| StoreError::Corrupt(format!("artifact kind {s}")))
}

fn version_num(v: i64) -> u32 {
    u32::try_from(v).unwrap_or(0)
}

impl TryFrom<ArtifactRow> for Artifact {
    type Error = StoreError;
    fn try_from(r: ArtifactRow) -> Result<Self> {
        Ok(Artifact {
            kind: kind(&r.kind)?,
            id: r.id,
            conversation_id: r.conversation_id,
            key: r.key,
            title: r.title,
            language: r.language,
            version: version_num(r.version),
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
    }
}

#[derive(sqlx::FromRow)]
struct VersionRow {
    artifact_id: String,
    version: i64,
    kind: String,
    title: String,
    language: Option<String>,
    content: String,
    message_id: Option<String>,
    created_at: i64,
}

impl TryFrom<VersionRow> for ArtifactVersion {
    type Error = StoreError;
    fn try_from(r: VersionRow) -> Result<Self> {
        Ok(ArtifactVersion {
            kind: kind(&r.kind)?,
            artifact_id: r.artifact_id,
            version: version_num(r.version),
            title: r.title,
            language: r.language,
            content: r.content,
            message_id: r.message_id,
            created_at: r.created_at,
        })
    }
}

const COLUMNS: &str =
    "id, conversation_id, key, kind, title, language, version, created_at, updated_at";
const VERSION_COLUMNS: &str =
    "artifact_id, version, kind, title, language, content, message_id, created_at";
const NOT_FOUND: StoreError = StoreError::NotFound { what: "artifact" };

fn excerpt(content: &str) -> String {
    let one = content.split_whitespace().collect::<Vec<_>>().join(" ");
    one.chars().take(EXCERPT_CHARS).collect()
}

impl Store {
    /// Saves a new version of the chat's artifact `key`, creating the artifact if it's new.
    pub async fn save_artifact_version(
        &self,
        v: NewArtifactVersion,
    ) -> Result<(Artifact, ArtifactVersion)> {
        let chars = v.content.chars().count();
        if chars > MAX_ARTIFACT_CHARS {
            return Err(StoreError::Invalid(format!(
                "the artifact is too long ({chars} characters; the limit is {MAX_ARTIFACT_CHARS})"
            )));
        }
        let now = now_ms();
        let mut tx = self.pool.begin().await?;
        let existing: Option<ArtifactRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM artifacts WHERE conversation_id = ? AND key = ?"
        ))
        .bind(&v.conversation_id)
        .bind(&v.key)
        .fetch_optional(&mut *tx)
        .await?;
        let artifact = match existing {
            Some(row) => {
                let mut a = Artifact::try_from(row)?;
                a.version += 1;
                a.kind = v.kind;
                a.title = v.title.clone();
                a.language = v.language.clone();
                a.updated_at = now;
                sqlx::query(
                    "UPDATE artifacts SET kind = ?, title = ?, language = ?, version = ?,
                     updated_at = ? WHERE id = ?",
                )
                .bind(a.kind.as_str())
                .bind(&a.title)
                .bind(&a.language)
                .bind(i64::from(a.version))
                .bind(now)
                .bind(&a.id)
                .execute(&mut *tx)
                .await?;
                a
            }
            None => {
                let a = Artifact {
                    id: new_id(),
                    conversation_id: v.conversation_id.clone(),
                    key: v.key.clone(),
                    kind: v.kind,
                    title: v.title.clone(),
                    language: v.language.clone(),
                    version: 1,
                    created_at: now,
                    updated_at: now,
                };
                sqlx::query(&format!(
                    "INSERT INTO artifacts ({COLUMNS}) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
                ))
                .bind(&a.id)
                .bind(&a.conversation_id)
                .bind(&a.key)
                .bind(a.kind.as_str())
                .bind(&a.title)
                .bind(&a.language)
                .bind(i64::from(a.version))
                .bind(a.created_at)
                .bind(a.updated_at)
                .execute(&mut *tx)
                .await
                .map_err(|e| map_fk(e, "conversation"))?;
                a
            }
        };
        let version = ArtifactVersion {
            artifact_id: artifact.id.clone(),
            version: artifact.version,
            kind: v.kind,
            title: v.title,
            language: v.language,
            content: v.content,
            message_id: v.message_id,
            created_at: now,
        };
        sqlx::query(&format!(
            "INSERT INTO artifact_versions ({VERSION_COLUMNS}) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"
        ))
        .bind(&version.artifact_id)
        .bind(i64::from(version.version))
        .bind(version.kind.as_str())
        .bind(&version.title)
        .bind(&version.language)
        .bind(&version.content)
        .bind(&version.message_id)
        .bind(version.created_at)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok((artifact, version))
    }

    /// The chat's artifact `key` and its newest content.
    pub async fn artifact_by_key(
        &self,
        conversation_id: &str,
        key: &str,
    ) -> Result<Option<(Artifact, String)>> {
        let row: Option<ArtifactRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM artifacts WHERE conversation_id = ? AND key = ?"
        ))
        .bind(conversation_id)
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let a = Artifact::try_from(row)?;
        let content: String = sqlx::query_scalar(
            "SELECT content FROM artifact_versions WHERE artifact_id = ? AND version = ?",
        )
        .bind(&a.id)
        .bind(i64::from(a.version))
        .fetch_one(&self.pool)
        .await?;
        Ok(Some((a, content)))
    }

    /// A chat's artifacts, oldest first.
    pub async fn conversation_artifacts(&self, conversation_id: &str) -> Result<Vec<Artifact>> {
        let rows: Vec<ArtifactRow> = sqlx::query_as(&format!(
            "SELECT {COLUMNS} FROM artifacts WHERE conversation_id = ? ORDER BY created_at, id"
        ))
        .bind(conversation_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(Artifact::try_from).collect()
    }

    /// Every artifact with its chat's title, most recently changed first.
    pub async fn list_artifacts(&self) -> Result<Vec<ArtifactSummary>> {
        #[derive(sqlx::FromRow)]
        struct Row {
            #[sqlx(flatten)]
            a: ArtifactRow,
            conversation_title: String,
            content: String,
        }
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT a.id, a.conversation_id, a.key, a.kind, a.title, a.language, a.version,
                    a.created_at, a.updated_at, c.title AS conversation_title,
                    substr(v.content, 1, 2000) AS content
             FROM artifacts a
             JOIN conversations c ON c.id = a.conversation_id
             JOIN artifact_versions v ON v.artifact_id = a.id AND v.version = a.version
             ORDER BY a.updated_at DESC, a.id DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                Ok(ArtifactSummary {
                    excerpt: excerpt(&r.content),
                    conversation_title: r.conversation_title,
                    artifact: Artifact::try_from(r.a)?,
                })
            })
            .collect()
    }

    /// An artifact with every version.
    pub async fn get_artifact(&self, id: &str) -> Result<ArtifactDetail> {
        let row: Option<ArtifactRow> =
            sqlx::query_as(&format!("SELECT {COLUMNS} FROM artifacts WHERE id = ?"))
                .bind(id)
                .fetch_optional(&self.pool)
                .await?;
        let artifact = Artifact::try_from(row.ok_or(NOT_FOUND)?)?;
        let rows: Vec<VersionRow> = sqlx::query_as(&format!(
            "SELECT {VERSION_COLUMNS} FROM artifact_versions WHERE artifact_id = ?
             ORDER BY version"
        ))
        .bind(id)
        .fetch_all(&self.pool)
        .await?;
        let versions = rows
            .into_iter()
            .map(ArtifactVersion::try_from)
            .collect::<Result<Vec<_>>>()?;
        Ok(ArtifactDetail { artifact, versions })
    }

    /// One version (`None`: the newest).
    pub async fn artifact_version(
        &self,
        id: &str,
        version: Option<u32>,
    ) -> Result<ArtifactVersion> {
        let row: Option<VersionRow> = match version {
            Some(n) => {
                sqlx::query_as(&format!(
                    "SELECT {VERSION_COLUMNS} FROM artifact_versions
                     WHERE artifact_id = ? AND version = ?"
                ))
                .bind(id)
                .bind(i64::from(n))
                .fetch_optional(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as(&format!(
                    "SELECT {VERSION_COLUMNS} FROM artifact_versions
                     WHERE artifact_id = ? ORDER BY version DESC LIMIT 1"
                ))
                .bind(id)
                .fetch_optional(&self.pool)
                .await?
            }
        };
        ArtifactVersion::try_from(row.ok_or(NOT_FOUND)?)
    }

    /// Deletes an artifact and its versions.
    pub async fn delete_artifact(&self, id: &str) -> Result<()> {
        let done = sqlx::query("DELETE FROM artifacts WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        if done.rows_affected() == 0 {
            return Err(NOT_FOUND);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::store;
    use oxide_core::StartConversation;

    async fn chat(s: &Store, text: &str) -> String {
        s.start_conversation(StartConversation {
            text: text.into(),
            model: "anthropic/sonnet".into(),
            project_id: None,
            attachments: Vec::new(),
            folders: vec![],
        })
        .await
        .expect("starts")
        .0
        .id
    }

    fn new(cid: &str, key: &str, content: &str) -> NewArtifactVersion {
        NewArtifactVersion {
            conversation_id: cid.into(),
            key: key.into(),
            kind: ArtifactKind::Markdown,
            title: format!("Doc {key}"),
            language: None,
            content: content.into(),
            message_id: Some("m1".into()),
        }
    }

    #[tokio::test]
    async fn versions_accumulate_per_key() {
        let s = store().await;
        let cid = chat(&s, "make docs").await;
        let (a, v1) = s
            .save_artifact_version(new(&cid, "plan", "one"))
            .await
            .expect("saves");
        assert_eq!((a.version, v1.version), (1, 1));
        let mut second = new(&cid, "plan", "two");
        second.kind = ArtifactKind::Code;
        second.language = Some("rust".into());
        second.title = "Renamed".into();
        let (a2, v2) = s.save_artifact_version(second).await.expect("saves");
        assert_eq!(a2.id, a.id);
        assert_eq!((a2.version, v2.version), (2, 2));
        assert_eq!(a2.kind, ArtifactKind::Code);
        assert_eq!(a2.title, "Renamed");

        let (found, content) = s
            .artifact_by_key(&cid, "plan")
            .await
            .expect("reads")
            .expect("exists");
        assert_eq!((found.version, content.as_str()), (2, "two"));
        assert!(
            s.artifact_by_key(&cid, "other")
                .await
                .expect("reads")
                .is_none()
        );

        let detail = s.get_artifact(&a.id).await.expect("reads");
        let contents: Vec<&str> = detail.versions.iter().map(|v| v.content.as_str()).collect();
        assert_eq!(contents, vec!["one", "two"]);
        assert_eq!(detail.versions[0].kind, ArtifactKind::Markdown);
        assert_eq!(
            s.artifact_version(&a.id, Some(1))
                .await
                .expect("v1")
                .content,
            "one"
        );
        assert_eq!(
            s.artifact_version(&a.id, None)
                .await
                .expect("newest")
                .content,
            "two"
        );
        assert!(matches!(
            s.artifact_version(&a.id, Some(9)).await,
            Err(StoreError::NotFound { .. })
        ));

        // Another chat may use the same key.
        let other = chat(&s, "other").await;
        let (b, _) = s
            .save_artifact_version(new(&other, "plan", "x"))
            .await
            .expect("saves");
        assert_ne!(b.id, a.id);
        assert_eq!(
            s.conversation_artifacts(&cid).await.expect("lists").len(),
            1
        );
    }

    #[tokio::test]
    async fn gallery_lists_newest_first_with_chat_titles() {
        let s = store().await;
        let first = chat(&s, "First chat").await;
        let second = chat(&s, "Second chat").await;
        s.save_artifact_version(new(&first, "a", "alpha\n\n  text"))
            .await
            .expect("saves");
        s.save_artifact_version(new(&second, "b", "beta"))
            .await
            .expect("saves");
        let list = s.list_artifacts().await.expect("lists");
        let keys: Vec<&str> = list.iter().map(|r| r.artifact.key.as_str()).collect();
        // Same millisecond: ids (time-ordered) break the tie.
        assert_eq!(keys, vec!["b", "a"]);
        assert_eq!(list[1].conversation_title, "First chat");
        assert_eq!(list[1].excerpt, "alpha text");
    }

    #[tokio::test]
    async fn artifacts_go_with_their_chat_and_can_be_deleted() {
        let s = store().await;
        let cid = chat(&s, "x").await;
        let (a, _) = s
            .save_artifact_version(new(&cid, "a", "1"))
            .await
            .expect("saves");
        let (b, _) = s
            .save_artifact_version(new(&cid, "b", "1"))
            .await
            .expect("saves");
        s.delete_artifact(&a.id).await.expect("deletes");
        assert!(matches!(
            s.get_artifact(&a.id).await,
            Err(StoreError::NotFound { .. })
        ));
        assert!(matches!(
            s.delete_artifact(&a.id).await,
            Err(StoreError::NotFound { .. })
        ));
        s.delete_conversation(&cid).await.expect("deletes chat");
        assert!(matches!(
            s.get_artifact(&b.id).await,
            Err(StoreError::NotFound { .. })
        ));
        assert!(s.list_artifacts().await.expect("lists").is_empty());
    }

    #[tokio::test]
    async fn limits_and_unknown_chats() {
        let s = store().await;
        let cid = chat(&s, "x").await;
        let big = "x".repeat(MAX_ARTIFACT_CHARS + 1);
        assert!(matches!(
            s.save_artifact_version(new(&cid, "a", &big)).await,
            Err(StoreError::Invalid(_))
        ));
        assert!(matches!(
            s.save_artifact_version(new("nope", "a", "x")).await,
            Err(StoreError::NotFound {
                what: "conversation"
            })
        ));
    }
}
