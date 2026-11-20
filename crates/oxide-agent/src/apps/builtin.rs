//! The Utility Apps that ship with Oxide (`utility-apps/<id>/` in the repository, compiled
//! into the binary). They are read-only; Clone makes an editable copy.

use oxide_core::{APP_MANIFEST, AppFile, AppManifest, UtilityApp};

/// One built-in app: its id and files (`(path, text)`).
pub struct Builtin {
    pub id: &'static str,
    pub files: &'static [(&'static str, &'static str)],
}

macro_rules! app {
    ($id:literal: $($file:literal),+ $(,)?) => {
        Builtin {
            id: $id,
            files: &[
                $(($file, include_str!(concat!("../../../../utility-apps/", $id, "/", $file)))),+
            ],
        }
    };
}

static APPS: &[Builtin] = &[
    app!("ai-meta-tag-generator": "oxide-app.json", "App.tsx"),
    app!("readability-tone-analyzer": "oxide-app.json", "App.tsx"),
    app!("ai-html-table-from-image": "oxide-app.json", "App.tsx"),
    app!("ai-svg-creator": "oxide-app.json", "App.tsx"),
];

pub fn all() -> &'static [Builtin] {
    APPS
}

pub fn find(id: &str) -> Option<&'static Builtin> {
    APPS.iter().find(|b| b.id == id)
}

impl Builtin {
    pub fn manifest(&self) -> Result<AppManifest, String> {
        let raw = self
            .files
            .iter()
            .find(|(p, _)| *p == APP_MANIFEST)
            .map(|(_, t)| *t)
            .ok_or_else(|| format!("{APP_MANIFEST} is missing"))?;
        let m: AppManifest =
            serde_json::from_str(raw).map_err(|e| format!("{APP_MANIFEST} isn't valid: {e}"))?;
        m.validate()?;
        Ok(m)
    }

    pub fn app(&self) -> Result<UtilityApp, String> {
        Ok(UtilityApp {
            manifest: self.manifest()?,
            builtin: true,
            folder: None,
            problem: None,
        })
    }

    pub fn text_files(&self) -> Vec<AppFile> {
        self.files
            .iter()
            .map(|(p, t)| AppFile {
                path: (*p).to_owned(),
                content: (*t).to_owned(),
            })
            .collect()
    }

    pub fn raw_files(&self) -> Vec<(String, Vec<u8>)> {
        self.files
            .iter()
            .map(|(p, t)| ((*p).to_owned(), t.as_bytes().to_vec()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_apps_are_valid() {
        assert_eq!(all().len(), 4);
        for b in all() {
            let m = b.manifest().unwrap_or_else(|e| panic!("{}: {e}", b.id));
            assert_eq!(m.id, b.id);
            assert!(
                b.files.iter().any(|(p, _)| *p == m.entry),
                "{}: entry {} missing",
                b.id,
                m.entry
            );
            assert!(!m.capabilities.is_empty(), "{}", b.id);
            assert!(!m.description.is_empty(), "{}", b.id);
            // Apps import only what the frame provides.
            for (path, text) in b.files {
                for part in text
                    .split(" from ")
                    .skip(1)
                    .chain(text.split("\nimport ").skip(1))
                {
                    let quoted = part.trim_start();
                    let Some(q) = quoted.chars().next().filter(|c| *c == '"' || *c == '\'') else {
                        continue;
                    };
                    let from = quoted[1..].split(q).next().unwrap_or("");
                    assert!(
                        from.starts_with('.')
                            || [
                                "react",
                                "oxide",
                                "lucide-react",
                                "react-markdown",
                                "remark-gfm",
                                "jszip"
                            ]
                            .contains(&from),
                        "{}/{path}: imports {from}",
                        b.id
                    );
                }
            }
        }
    }
}
