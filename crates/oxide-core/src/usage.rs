//! Token usage across chats, for Settings → Usage (Phase 18).
//!
//! Built from the token counts providers report for each saved reply (`Message.usage`, the
//! sum over a reply's tool-loop requests). Titles, compaction summaries and memory updates
//! aren't saved as replies, so they aren't counted; deleted chats take their usage with them.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{Timestamp, Usage};

/// The longest range a report covers day by day.
pub const MAX_USAGE_DAYS: u32 = 366;

/// One saved reply's usage (read from the store).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageRecord {
    pub conversation_id: String,
    /// Stored model id (`<providerId>/<model>`), or empty when unknown.
    pub model: String,
    pub created_at: Timestamp,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UsageTotals {
    /// Replies counted (with or without token counts).
    pub replies: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

impl UsageTotals {
    fn add(&mut self, usage: Option<&Usage>) {
        self.replies += 1;
        if let Some(u) = usage {
            self.input_tokens += u64::from(u.input_tokens);
            self.output_tokens += u64::from(u.output_tokens);
            self.cache_read_tokens += u64::from(u.cache_read_tokens);
            self.cache_write_tokens += u64::from(u.cache_write_tokens);
        }
    }

    pub fn tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UsageByProvider {
    /// Endpoint id (`anthropic`, `lmstudio`, a custom endpoint's id), or `""` if unknown.
    pub provider_id: String,
    pub totals: UsageTotals,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UsageByModel {
    pub provider_id: String,
    /// The provider's model id.
    pub model: String,
    pub totals: UsageTotals,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UsageDay {
    /// Local date, `YYYY-MM-DD`.
    pub day: String,
    pub totals: UsageTotals,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UsageReport {
    /// Start of the range (`None`: everything saved).
    pub since: Option<Timestamp>,
    pub totals: UsageTotals,
    /// Chats with at least one counted reply.
    pub chats: u64,
    /// Replies whose provider reported no token counts.
    pub unreported: u64,
    /// Most tokens first.
    pub by_provider: Vec<UsageByProvider>,
    /// Most tokens first.
    pub by_model: Vec<UsageByModel>,
    /// Every day of the range, oldest first (at most [`MAX_USAGE_DAYS`]; for "all time", from
    /// the first reply).
    pub days: Vec<UsageDay>,
}

/// Days since 1970-01-01 → (year, month, day) (Howard Hinnant's algorithm).
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

const DAY_MS: i64 = 86_400_000;

/// The local day number of `at` for a zone `offset_minutes` east of UTC.
fn local_day(at: Timestamp, offset_minutes: i32) -> i64 {
    (at + i64::from(offset_minutes) * 60_000).div_euclid(DAY_MS)
}

fn day_label(day: i64) -> String {
    let (y, m, d) = civil_from_days(day);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Builds the report for the last `days` local days including today (`0`: everything), in a
/// zone `offset_minutes` east of UTC.
pub fn build_usage_report(
    records: &[UsageRecord],
    now: Timestamp,
    days: u32,
    offset_minutes: i32,
) -> UsageReport {
    let days = days.min(MAX_USAGE_DAYS);
    let today = local_day(now, offset_minutes);
    let first_day = if days == 0 {
        None
    } else {
        Some(today - i64::from(days) + 1)
    };
    let since = first_day.map(|d| d * DAY_MS - i64::from(offset_minutes) * 60_000);

    let mut report = UsageReport {
        since,
        ..Default::default()
    };
    let mut providers: BTreeMap<String, UsageTotals> = BTreeMap::new();
    let mut models: BTreeMap<(String, String), UsageTotals> = BTreeMap::new();
    let mut by_day: BTreeMap<i64, UsageTotals> = BTreeMap::new();
    let mut chats: std::collections::BTreeSet<&str> = Default::default();

    for r in records {
        let day = local_day(r.created_at, offset_minutes);
        if first_day.is_some_and(|f| day < f) || day > today {
            continue;
        }
        let (provider, model) = crate::model::split_model_id(&r.model)
            .map(|(p, m)| (p.to_owned(), m.to_owned()))
            .unwrap_or_else(|| (String::new(), r.model.clone()));
        report.totals.add(r.usage.as_ref());
        if r.usage.is_none() {
            report.unreported += 1;
        }
        providers
            .entry(provider.clone())
            .or_default()
            .add(r.usage.as_ref());
        models
            .entry((provider, model))
            .or_default()
            .add(r.usage.as_ref());
        by_day.entry(day).or_default().add(r.usage.as_ref());
        chats.insert(r.conversation_id.as_str());
    }
    report.chats = chats.len() as u64;

    report.by_provider = providers
        .into_iter()
        .map(|(provider_id, totals)| UsageByProvider {
            provider_id,
            totals,
        })
        .collect();
    report
        .by_provider
        .sort_by_key(|p| std::cmp::Reverse(p.totals.tokens()));
    report.by_model = models
        .into_iter()
        .map(|((provider_id, model), totals)| UsageByModel {
            provider_id,
            model,
            totals,
        })
        .collect();
    report
        .by_model
        .sort_by_key(|m| std::cmp::Reverse(m.totals.tokens()));

    let start = first_day.or_else(|| by_day.keys().next().copied());
    if let Some(start) = start {
        let start = start.max(today - i64::from(MAX_USAGE_DAYS) + 1);
        report.days = (start..=today)
            .map(|d| UsageDay {
                day: day_label(d),
                totals: by_day.get(&d).copied().unwrap_or_default(),
            })
            .collect();
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(chat: &str, model: &str, at: Timestamp, i: u32, o: u32) -> UsageRecord {
        UsageRecord {
            conversation_id: chat.into(),
            model: model.into(),
            created_at: at,
            usage: Some(Usage {
                input_tokens: i,
                output_tokens: o,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
            }),
        }
    }

    // 2026-11-15 12:00 UTC.
    const NOW: Timestamp = 1_794_744_000_000;

    #[test]
    fn days_are_labelled_in_local_time() {
        assert_eq!(day_label(local_day(NOW, 0)), "2026-11-15");
        // 23:30 UTC is the next day in Copenhagen (UTC+1).
        let late = NOW + 11 * 3_600_000 + 30 * 60_000;
        assert_eq!(day_label(local_day(late, 0)), "2026-11-15");
        assert_eq!(day_label(local_day(late, 60)), "2026-11-16");
        assert_eq!(civil_from_days(0), (1970, 1, 1));
    }

    #[test]
    fn reports_group_by_provider_model_and_day() {
        let records = vec![
            rec("a", "anthropic/claude-x", NOW, 100, 50),
            rec("a", "anthropic/claude-x", NOW - DAY_MS, 10, 5),
            rec("b", "lmstudio/qwen/qwen3", NOW - 2 * DAY_MS, 1_000, 500),
            rec("c", "anthropic/claude-y", NOW - 40 * DAY_MS, 7, 7),
            UsageRecord {
                conversation_id: "b".into(),
                model: "lmstudio/qwen/qwen3".into(),
                created_at: NOW,
                usage: None,
            },
        ];
        let r = build_usage_report(&records, NOW, 7, 60);
        assert_eq!(r.days.len(), 7);
        assert_eq!(r.days.last().map(|d| d.day.as_str()), Some("2026-11-15"));
        assert_eq!(r.totals.replies, 4);
        assert_eq!(r.totals.input_tokens, 1_110);
        assert_eq!(r.unreported, 1);
        assert_eq!(r.chats, 2);
        assert_eq!(r.by_provider[0].provider_id, "lmstudio");
        assert_eq!(r.by_provider[1].totals.tokens(), 165);
        assert_eq!(r.by_model[0].model, "qwen/qwen3");
        assert_eq!(r.days[6].totals.replies, 2);
        assert_eq!(r.days[4].totals.input_tokens, 1_000);

        let all = build_usage_report(&records, NOW, 0, 60);
        assert_eq!(all.since, None);
        assert_eq!(all.totals.replies, 5);
        assert_eq!(all.days.len(), 41);
        assert_eq!(all.by_model.len(), 3);
    }

    #[test]
    fn an_empty_store_gives_an_empty_report() {
        let r = build_usage_report(&[], NOW, 0, 0);
        assert!(r.days.is_empty());
        assert_eq!(r.totals, UsageTotals::default());
        let r = build_usage_report(&[], NOW, 30, 0);
        assert_eq!(r.days.len(), 30);
    }
}
