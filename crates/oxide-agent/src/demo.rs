//! A long demo conversation for checking rendering and scrolling by hand (Phase 4 exit
//! check): 50 messages with code, math, tables, lists and images. Nothing is sent to a model.

use std::io::Cursor;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgb};
use oxide_core::{
    AttachmentSource, ContentBlock, Conversation, Role, StartConversation, StopReason, Usage,
};
use oxide_store::{NewReply, Store, new_id};

use crate::AgentError;
use crate::attachments::AttachmentStore;

/// Number of question/answer pairs (two messages each).
pub const DEMO_EXCHANGES: usize = 25;

const QUESTIONS: [&str; 6] = [
    "Show me a Rust function that parses a semver string.",
    "Explain Euler's identity with the math.",
    "Compare three sorting algorithms in a table.",
    "Write a small TypeScript debounce helper and a Python equivalent.",
    "What's in this picture?",
    "Give me a checklist for a release, with nested steps.",
];

fn answer(i: usize) -> String {
    match i % 6 {
        0 => r#"Here's a small parser with no dependencies:

```rust
#[derive(Debug, PartialEq)]
pub struct Version { pub major: u64, pub minor: u64, pub patch: u64 }

pub fn parse(s: &str) -> Option<Version> {
    let mut parts = s.trim().trim_start_matches('v').splitn(3, '.');
    let mut next = || parts.next()?.parse().ok();
    Some(Version { major: next()?, minor: next()?, patch: next()? })
}
```

It accepts an optional leading `v`, so `parse("v1.2.3")` works too. Pre-release tags such as `1.2.3-beta` are **not** handled; split on `-` first if you need them."#
            .to_owned(),
        1 => r#"Euler's identity links five fundamental constants:

$$
e^{i\pi} + 1 = 0
$$

It follows from Euler's formula $e^{ix} = \cos x + i\sin x$ with $x = \pi$, since $\cos\pi = -1$ and $\sin\pi = 0$. A related series:

$$
e^{x} = \sum_{n=0}^{\infty} \frac{x^n}{n!}
$$

(And no, a coffee costing $4 and a sandwich costing $7 isn't math.)"#
            .to_owned(),
        2 => r#"| Algorithm | Average | Worst | Stable | Memory |
|---|---|---|:---:|---|
| Quicksort | $O(n \log n)$ | $O(n^2)$ | No | $O(\log n)$ |
| Merge sort | $O(n \log n)$ | $O(n \log n)$ | Yes | $O(n)$ |
| Heapsort | $O(n \log n)$ | $O(n \log n)$ | No | $O(1)$ |

Merge sort is the safe default when stability matters; quicksort usually wins on raw speed."#
            .to_owned(),
        3 => r#"TypeScript:

```ts
export function debounce<A extends unknown[]>(fn: (...args: A) => void, ms: number) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  return (...args: A) => {
    clearTimeout(timer);
    timer = setTimeout(() => fn(...args), ms);
  };
}
```

Python:

```python
import threading

def debounce(ms):
    def wrap(fn):
        timer = None
        def debounced(*args, **kwargs):
            nonlocal timer
            if timer:
                timer.cancel()
            timer = threading.Timer(ms / 1000, fn, args, kwargs)
            timer.start()
        return debounced
    return wrap
```

A very long line to check horizontal scrolling and the wrap toggle: `const config = { retries: 3, backoffMs: 250, maxBackoffMs: 30_000, jitter: true, onRetry: (attempt) => console.log("retrying", attempt) };`"#
            .to_owned(),
        4 => "It's a generated gradient: red increases from left to right and green from top \
              to bottom, with a constant blue channel. There are no objects in it; it's a test \
              pattern."
            .to_owned(),
        _ => r#"1. **Freeze** the branch
   - Announce the freeze
   - Merge only fixes
2. **Verify**
   - [x] All checks green
   - [ ] Manual smoke test on Windows
   - [ ] Release notes reviewed
3. **Ship**
   > Tag the release, then publish the installers.

---

See the [Keep a Changelog](https://keepachangelog.com) format for the notes."#
            .to_owned(),
    }
}

fn gradient_png(w: u32, h: u32, seed: u8) -> Vec<u8> {
    let img = ImageBuffer::from_fn(w, h, |x, y| {
        Rgb([
            u8::try_from(x * 255 / w.max(1)).unwrap_or(255),
            u8::try_from(y * 255 / h.max(1)).unwrap_or(255),
            seed,
        ])
    });
    let mut out = Vec::new();
    // Writing a PNG to memory can't fail for an RGB buffer.
    let _ = DynamicImage::ImageRgb8(img).write_to(&mut Cursor::new(&mut out), ImageFormat::Png);
    out
}

/// Creates the demo chat on `model` and returns it.
pub async fn create_demo_conversation(
    store: &Store,
    attachments: &AttachmentStore,
    model: &str,
) -> Result<Conversation, AgentError> {
    let mut parent: Option<String> = None;
    let mut conversation: Option<Conversation> = None;
    for i in 0..DEMO_EXCHANGES {
        let question = QUESTIONS[i % QUESTIONS.len()];
        let mut content = Vec::new();
        if i % 6 == 4 {
            let a = attachments
                .prepare(AttachmentSource::Bytes {
                    name: format!("gradient-{i}.png"),
                    data: STANDARD.encode(gradient_png(640, 400, u8::try_from(i * 9).unwrap_or(0))),
                })
                .await?;
            content.push(a.to_block());
        }
        if i % 6 == 5 {
            let pdf = oxide_llm::media::sample_pdf(&["Release checklist", "Sign-off"]);
            let a = attachments
                .prepare(AttachmentSource::Bytes {
                    name: "release-checklist.pdf".into(),
                    data: STANDARD.encode(pdf),
                })
                .await?;
            content.push(a.to_block());
        }
        content.push(ContentBlock::text(format!("{question} (#{})", i + 1)));
        let user = match &conversation {
            None => {
                let (c, first) = store
                    .start_conversation(StartConversation {
                        text: format!("{question} (#1)"),
                        model: model.to_owned(),
                        project_id: None,
                        attachments: Vec::new(),
                        folders: vec![],
                        tools: None,
                    })
                    .await?;
                let c = store
                    .rename_conversation(&c.id, "Demo: Markdown, code, math and images")
                    .await?;
                conversation = Some(c);
                first
            }
            Some(c) => {
                store
                    .add_message(&c.id, parent.as_deref(), Role::User, content, None)
                    .await?
            }
        };
        let cid = user.conversation_id.clone();
        let reply = store
            .add_reply(NewReply {
                id: new_id(),
                conversation_id: cid,
                parent_id: user.id.clone(),
                content: vec![ContentBlock::text(answer(i))],
                model: model.to_owned(),
                stop_reason: StopReason::EndTurn,
                usage: Some(Usage {
                    input_tokens: 1_200 + 40 * u32::try_from(i).unwrap_or(0),
                    output_tokens: 180,
                    cache_read_tokens: 1_000,
                    cache_write_tokens: 0,
                }),
            })
            .await?;
        parent = Some(reply.id);
    }
    let c = conversation.ok_or(AgentError::NotFound("conversation"))?;
    // Re-read so `updated_at` reflects the last message.
    Ok(store.get_conversation(&c.id).await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_demo_has_fifty_messages_and_images() {
        let store = Store::open_in_memory().await.expect("store");
        let tmp = tempfile::tempdir().expect("dir");
        let files = AttachmentStore::new(tmp.path().join("attachments"));
        let c = create_demo_conversation(&store, &files, "demo/model")
            .await
            .expect("creates");
        let messages = store.list_messages(&c.id).await.expect("lists");
        assert_eq!(messages.len(), DEMO_EXCHANGES * 2);
        let images = messages
            .iter()
            .flat_map(|m| &m.content)
            .filter(|b| matches!(b, ContentBlock::Image { .. }))
            .count();
        assert!(images >= 4, "{images}");
        // One linear thread: each message's parent is the one before it.
        for pair in messages.windows(2) {
            assert_eq!(pair[1].parent_id.as_deref(), Some(pair[0].id.as_str()));
        }
    }
}
