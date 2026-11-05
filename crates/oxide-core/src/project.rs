//! Projects group chats under shared instructions (knowledge files arrive in Phase 7).

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::Timestamp;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NewProject {
    pub name: String,
    pub description: String,
}

/// Partial update: `None` leaves a field as it is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectPatch {
    pub name: Option<String>,
    pub description: Option<String>,
    pub instructions: Option<String>,
}
