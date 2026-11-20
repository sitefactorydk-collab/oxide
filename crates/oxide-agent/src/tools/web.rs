//! Oxide's built-in web tools: `web_search` (Brave, Tavily or SearXNG) and `web_fetch`.

use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use oxide_core::provider::mask_key;
use oxide_core::{SearchKeyKind, WebSearchKeys, WebSearchProvider, WebSearchSettings};
use oxide_store::{Secret, SecretStore};
use serde_json::{Value, json};
use url::Url;

use super::html::{decode_entities, html_to_markdown};

/// Largest response `web_fetch` downloads.
pub const MAX_FETCH_BYTES: usize = 5 * 1024 * 1024;
/// Characters `web_fetch` returns at a time.
pub const DEFAULT_FETCH_CHARS: usize = 40_000;
/// Most characters a model may ask for at once.
pub const MAX_FETCH_CHARS: usize = 100_000;
/// Images larger than this aren't returned to the model.
const MAX_FETCH_IMAGE: usize = 5 * 1024 * 1024;

const BRAVE_URL: &str = "https://api.search.brave.com/res/v1/web/search";
const TAVILY_URL: &str = "https://api.tavily.com/search";

/// One search result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub age: Option<String>,
}

/// What `web_fetch` got.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fetched {
    /// A page or text file, as Markdown or plain text.
    Text {
        url: String,
        title: Option<String>,
        text: String,
        content_type: String,
    },
    Image {
        url: String,
        media_type: String,
        data_base64: String,
    },
}

/// A page's text as sent ([`WebTools::fetch_raw`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawPage {
    /// The URL after redirects.
    pub url: String,
    /// `text/html` for pages, else the server's type.
    pub content_type: String,
    pub text: String,
    /// The page was longer than Oxide reads.
    pub truncated: bool,
}

/// HTTP client and keys for the web tools.
#[derive(Clone)]
pub struct WebTools {
    http: reqwest::Client,
    secrets: Arc<dyn SecretStore>,
    brave_url: String,
    tavily_url: String,
}

fn user_agent() -> String {
    format!(
        "Mozilla/5.0 (compatible; Oxide/{}; desktop AI assistant)",
        oxide_core::version()
    )
}

async fn blocking<T: Send + 'static>(
    secrets: &Arc<dyn SecretStore>,
    f: impl FnOnce(&dyn SecretStore) -> Result<T, oxide_store::SecretError> + Send + 'static,
) -> Result<T, String> {
    let secrets = Arc::clone(secrets);
    tokio::task::spawn_blocking(move || f(secrets.as_ref()))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

impl WebTools {
    pub fn new(secrets: Arc<dyn SecretStore>) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(user_agent())
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(45))
            .redirect(reqwest::redirect::Policy::limited(10))
            .build()
            .unwrap_or_default();
        Self {
            http,
            secrets,
            brave_url: BRAVE_URL.into(),
            tavily_url: TAVILY_URL.into(),
        }
    }

    /// Points the hosted search APIs elsewhere (tests).
    pub fn with_urls(mut self, brave: &str, tavily: &str) -> Self {
        self.brave_url = brave.into();
        self.tavily_url = tavily.into();
        self
    }

    pub async fn key(&self, kind: SearchKeyKind) -> Option<String> {
        let name = kind.secret_name();
        blocking(&self.secrets, move |s| s.get(name))
            .await
            .ok()
            .flatten()
            .map(|s| s.expose().trim().to_owned())
            .filter(|k| !k.is_empty())
    }

    pub async fn set_key(&self, kind: SearchKeyKind, key: Option<String>) -> Result<(), String> {
        let name = kind.secret_name();
        match key.map(|k| k.trim().to_owned()).filter(|k| !k.is_empty()) {
            Some(k) => blocking(&self.secrets, move |s| s.set(name, &Secret::new(k))).await,
            None => blocking(&self.secrets, move |s| s.delete(name)).await,
        }
    }

    /// Masked hints of the stored keys.
    pub async fn keys(&self) -> WebSearchKeys {
        WebSearchKeys {
            brave: self.key(SearchKeyKind::Brave).await.map(|k| mask_key(&k)),
            tavily: self.key(SearchKeyKind::Tavily).await.map(|k| mask_key(&k)),
        }
    }

    /// Whether the configured search service can be used (it has a key or a URL).
    pub async fn search_ready(&self, s: &WebSearchSettings) -> bool {
        match s.provider {
            WebSearchProvider::None => false,
            WebSearchProvider::Brave => self.key(SearchKeyKind::Brave).await.is_some(),
            WebSearchProvider::Tavily => self.key(SearchKeyKind::Tavily).await.is_some(),
            WebSearchProvider::Searxng => !s.searxng_url.trim().is_empty(),
        }
    }

    /// Searches with the configured service. `key` overrides the stored key (Settings test).
    pub async fn search(
        &self,
        s: &WebSearchSettings,
        query: &str,
        max: u32,
        key: Option<String>,
    ) -> Result<Vec<SearchHit>, String> {
        let query = query.trim();
        if query.is_empty() {
            return Err("the search query is empty".into());
        }
        let max = max.clamp(1, 10);
        let key_for = |kind| async move {
            match key.clone().filter(|k| !k.trim().is_empty()) {
                Some(k) => Some(k.trim().to_owned()),
                None => self.key(kind).await,
            }
        };
        let hits = match s.provider {
            WebSearchProvider::None => {
                return Err("no web search service is set up (Settings → Web search)".into());
            }
            WebSearchProvider::Brave => {
                let key = key_for(SearchKeyKind::Brave)
                    .await
                    .ok_or("Brave Search needs an API key (Settings → Web search)")?;
                let v = self
                    .json(
                        self.http
                            .get(with_query(
                                &self.brave_url,
                                &[("q", query), ("count", &max.to_string())],
                            )?)
                            .header("Accept", "application/json")
                            .header("X-Subscription-Token", key),
                        "Brave Search",
                    )
                    .await?;
                parse_results(&v["web"]["results"], "description", "age")
            }
            WebSearchProvider::Tavily => {
                let key = key_for(SearchKeyKind::Tavily)
                    .await
                    .ok_or("Tavily needs an API key (Settings → Web search)")?;
                let v = self
                    .json(
                        self.http
                            .post(&self.tavily_url)
                            .bearer_auth(key)
                            .json(&json!({
                                "query": query,
                                "max_results": max,
                                "search_depth": "basic",
                            })),
                        "Tavily",
                    )
                    .await?;
                parse_results(&v["results"], "content", "published_date")
            }
            WebSearchProvider::Searxng => {
                let base = s.searxng_url.trim().trim_end_matches('/');
                if base.is_empty() {
                    return Err("SearXNG needs its URL (Settings → Web search)".into());
                }
                let v = self
                    .json(
                        self.http
                            .get(with_query(
                                &format!("{base}/search"),
                                &[("q", query), ("format", "json")],
                            )?)
                            .header("Accept", "application/json"),
                        "SearXNG",
                    )
                    .await?;
                parse_results(&v["results"], "content", "publishedDate")
            }
        };
        Ok(hits.into_iter().take(max as usize).collect())
    }

    async fn json(&self, req: reqwest::RequestBuilder, service: &str) -> Result<Value, String> {
        let res = req
            .send()
            .await
            .map_err(|e| format!("{service} couldn't be reached: {}", short_error(&e)))?;
        let status = res.status();
        let text = res
            .text()
            .await
            .map_err(|e| format!("{service}: {}", short_error(&e)))?;
        if !status.is_success() {
            let detail: String = text.chars().take(200).collect();
            return Err(match status.as_u16() {
                401 | 403 => format!("{service} refused the API key (HTTP {status})"),
                429 => format!("{service}: too many requests, or the plan's limit is used up"),
                _ => format!("{service} answered HTTP {status}: {detail}"),
            });
        }
        serde_json::from_str(&text).map_err(|_| {
            format!(
                "{service} didn't answer with JSON{}",
                if service == "SearXNG" {
                    " (enable `json` under `search.formats` in its settings.yml)"
                } else {
                    ""
                }
            )
        })
    }

    /// Downloads `url` and makes it readable. `max_bytes` caps the download.
    /// A page's text as the server sent it (HTML not converted), for Utility Apps that read
    /// the markup and for the crawler. Refuses images, PDFs and other binary files.
    pub async fn fetch_raw(&self, url: &str) -> Result<RawPage, String> {
        let (final_url, mime, bytes, truncated) = self.get_bytes(url).await?;
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]).to_ascii_lowercase();
        let html = mime.contains("html")
            || ((mime.is_empty() || mime == "text/plain")
                && (head.contains("<html") || head.contains("<!doctype html")));
        let texty = html
            || mime.is_empty()
            || mime.starts_with("text/")
            || mime.contains("json")
            || mime.contains("xml");
        if !texty {
            return Err(format!("this kind of file ({mime}) isn't text"));
        }
        Ok(RawPage {
            url: final_url.to_string(),
            content_type: if html { "text/html".into() } else { mime },
            text: String::from_utf8_lossy(&bytes).into_owned(),
            truncated,
        })
    }

    async fn get_bytes(&self, url: &str) -> Result<(Url, String, Vec<u8>, bool), String> {
        let parsed = Url::parse(url.trim()).map_err(|_| format!("“{url}” isn't a valid URL"))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err("only http and https URLs can be fetched".into());
        }
        let mut res = self
            .http
            .get(parsed.clone())
            .header(
                "Accept",
                "text/html,application/xhtml+xml,text/plain;q=0.9,application/json;q=0.9,*/*;q=0.5",
            )
            .send()
            .await
            .map_err(|e| {
                format!(
                    "couldn't reach {}: {}",
                    parsed.host_str().unwrap_or("the site"),
                    short_error(&e)
                )
            })?;
        let status = res.status();
        let final_url = res.url().clone();
        if !status.is_success() {
            return Err(format!("the server answered HTTP {status}"));
        }
        let content_type = res
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_ascii_lowercase();
        let mut bytes: Vec<u8> = Vec::new();
        let mut truncated = false;
        while let Some(chunk) = res.chunk().await.map_err(|e| short_error(&e))? {
            if bytes.len() + chunk.len() > MAX_FETCH_BYTES {
                bytes.extend_from_slice(&chunk[..MAX_FETCH_BYTES - bytes.len()]);
                truncated = true;
                break;
            }
            bytes.extend_from_slice(&chunk);
        }
        let mime = content_type
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_owned();
        Ok((final_url, mime, bytes, truncated))
    }

    pub async fn fetch(&self, url: &str) -> Result<Fetched, String> {
        let (final_url, mime, bytes, truncated) = self.get_bytes(url).await?;
        let sniff_html = || {
            let head = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]).to_ascii_lowercase();
            head.contains("<html") || head.contains("<!doctype html")
        };
        let url_s = final_url.to_string();
        if mime.starts_with("image/") {
            if truncated || bytes.len() > MAX_FETCH_IMAGE {
                return Err("the image is larger than 5 MB".into());
            }
            return Ok(Fetched::Image {
                url: url_s,
                media_type: mime,
                data_base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
            });
        }
        if mime == "application/pdf" || bytes.starts_with(b"%PDF-") {
            if truncated {
                return Err("the PDF is larger than 5 MB".into());
            }
            let pages = oxide_llm::media::pdf_pages(bytes)
                .await
                .map_err(|e| format!("the PDF's text couldn't be read: {e}"))?;
            return Ok(Fetched::Text {
                url: url_s,
                title: None,
                text: oxide_llm::media::join_pages(&pages),
                content_type: "application/pdf".into(),
            });
        }
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let is_html =
            mime.contains("html") || (mime.is_empty() || mime == "text/plain") && sniff_html();
        if is_html {
            let r = html_to_markdown(&text, Some(&final_url));
            return Ok(Fetched::Text {
                url: url_s,
                title: r.title,
                text: r.markdown,
                content_type: "text/html".into(),
            });
        }
        let texty = mime.is_empty()
            || mime.starts_with("text/")
            || mime.contains("json")
            || mime.contains("xml")
            || mime.contains("javascript")
            || mime.contains("yaml")
            || mime.contains("csv");
        if !texty {
            return Err(format!("this kind of file ({mime}) can't be read"));
        }
        Ok(Fetched::Text {
            url: url_s,
            title: None,
            text,
            content_type: mime,
        })
    }
}

fn with_query(base: &str, pairs: &[(&str, &str)]) -> Result<Url, String> {
    let mut url = Url::parse(base).map_err(|_| format!("“{base}” isn't a valid URL"))?;
    url.query_pairs_mut().extend_pairs(pairs);
    Ok(url)
}

fn short_error(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        return "it took too long to answer".into();
    }
    if e.is_connect() {
        return "the connection failed".into();
    }
    let s = e.to_string();
    s.chars().take(200).collect()
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    decode_entities(&out)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_results(list: &Value, snippet_key: &str, age_key: &str) -> Vec<SearchHit> {
    list.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|r| {
                    let url = r["url"].as_str()?.to_owned();
                    Some(SearchHit {
                        title: strip_tags(r["title"].as_str().unwrap_or(&url)),
                        snippet: strip_tags(r[snippet_key].as_str().unwrap_or("")),
                        age: r[age_key].as_str().map(str::to_owned),
                        url,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Search results as the model sees them.
pub fn format_hits(query: &str, hits: &[SearchHit]) -> String {
    if hits.is_empty() {
        return format!("No results for “{query}”.");
    }
    let mut s = format!("Search results for “{query}”:\n");
    for (i, h) in hits.iter().enumerate() {
        s.push_str(&format!("\n{}. {}\n   {}\n", i + 1, h.title, h.url));
        if let Some(age) = &h.age {
            s.push_str(&format!("   ({age})\n"));
        }
        if !h.snippet.is_empty() {
            let snippet: String = h.snippet.chars().take(500).collect();
            s.push_str(&format!("   {snippet}\n"));
        }
    }
    s
}

/// A slice of fetched text, starting at character `start`, with a note when there's more.
pub fn page_slice(text: &str, start: usize, max_chars: usize) -> String {
    let total = text.chars().count();
    if start >= total && total > 0 {
        return format!("[The page has only {total} characters; nothing after {start}.]");
    }
    let slice: String = text.chars().skip(start).take(max_chars).collect();
    let end = start + slice.chars().count();
    let mut s = slice;
    if end < total {
        s.push_str(&format!(
            "\n\n[Cut at character {end} of {total}. Call web_fetch again with \"start\": {end} to read on.]"
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxide_store::MemorySecrets;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Serves one canned answer per connection; returns the base URL and the requests seen.
    async fn serve(
        replies: Vec<(u16, &'static str, String)>,
    ) -> (String, tokio::sync::mpsc::UnboundedReceiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("binds");
        let addr = listener.local_addr().expect("addr");
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            for (status, ctype, body) in replies {
                let Ok((mut sock, _)) = listener.accept().await else {
                    return;
                };
                let mut buf = vec![0u8; 16 * 1024];
                let mut got = Vec::new();
                loop {
                    let n = sock.read(&mut buf).await.unwrap_or(0);
                    got.extend_from_slice(&buf[..n]);
                    let s = String::from_utf8_lossy(&got).to_string();
                    if let Some(head_end) = s.find("\r\n\r\n") {
                        let len = s[..head_end]
                            .lines()
                            .find_map(|l| {
                                l.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                            })
                            .unwrap_or(0);
                        if got.len() >= head_end + 4 + len {
                            break;
                        }
                    }
                    if n == 0 {
                        break;
                    }
                }
                let _ = tx.send(String::from_utf8_lossy(&got).to_string());
                let resp = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = sock.write_all(resp.as_bytes()).await;
                let _ = sock.shutdown().await;
            }
        });
        (format!("http://{addr}"), rx)
    }

    fn tools() -> WebTools {
        WebTools::new(Arc::new(MemorySecrets::default()))
    }

    #[tokio::test]
    async fn brave_and_tavily_send_keys_and_parse_results() {
        let brave = r#"{"web":{"results":[{"title":"Oxide <strong>app</strong>","url":"https://a.example","description":"A &amp; B","age":"2 days ago"},{"title":"No url"}]}}"#;
        let tavily = r#"{"results":[{"title":"T","url":"https://t.example","content":"tc"}]}"#;
        let (base, mut seen) = serve(vec![
            (200, "application/json", brave.into()),
            (200, "application/json", tavily.into()),
            (401, "application/json", "{}".into()),
        ])
        .await;
        let web = tools().with_urls(&format!("{base}/brave"), &format!("{base}/tavily"));
        web.set_key(SearchKeyKind::Brave, Some(" brave-key-123456 ".into()))
            .await
            .expect("saves");
        let mut s = WebSearchSettings {
            provider: WebSearchProvider::Brave,
            ..WebSearchSettings::default()
        };
        assert!(web.search_ready(&s).await);
        let hits = web
            .search(&s, "oxide app", 5, None)
            .await
            .expect("searches");
        assert_eq!(
            hits,
            vec![SearchHit {
                title: "Oxide app".into(),
                url: "https://a.example".into(),
                snippet: "A & B".into(),
                age: Some("2 days ago".into()),
            }]
        );
        let req = seen.recv().await.expect("request");
        assert!(req.starts_with("GET /brave?q=oxide+app&count=5 "), "{req}");
        assert!(
            req.to_lowercase()
                .contains("x-subscription-token: brave-key-123456")
        );
        assert_eq!(web.keys().await.brave.as_deref(), Some("••••3456"));

        s.provider = WebSearchProvider::Tavily;
        assert!(!web.search_ready(&s).await, "no Tavily key yet");
        let hits = web
            .search(&s, "q", 3, Some("tvly-typed".into()))
            .await
            .expect("searches");
        assert_eq!(hits[0].snippet, "tc");
        let req = seen.recv().await.expect("request");
        assert!(
            req.to_lowercase()
                .contains("authorization: bearer tvly-typed")
        );
        assert!(req.contains("\"max_results\":3"), "{req}");

        let err = web
            .search(&s, "q", 3, Some("bad".into()))
            .await
            .unwrap_err();
        assert!(err.contains("refused the API key"), "{err}");
        assert!(format_hits("q", &hits).contains("1. T\n   https://t.example"));
    }

    #[tokio::test]
    async fn searxng_needs_json() {
        let (base, _seen) = serve(vec![
            (200, "application/json", r#"{"results":[{"title":"S","url":"https://s.example","content":"sc","publishedDate":"2026-01-01"}]}"#.into()),
            (200, "text/html", "<html>nope</html>".into()),
        ])
        .await;
        let s = WebSearchSettings {
            provider: WebSearchProvider::Searxng,
            searxng_url: base.clone(),
            ..WebSearchSettings::default()
        };
        let web = tools();
        let hits = web.search(&s, "x", 5, None).await.expect("searches");
        assert_eq!(hits[0].age.as_deref(), Some("2026-01-01"));
        let err = web.search(&s, "x", 5, None).await.unwrap_err();
        assert!(err.contains("settings.yml"), "{err}");
    }

    #[tokio::test]
    async fn fetch_reads_html_text_and_refuses_binaries() {
        let page = "<html><head><title>T</title></head><body><main><h1>Hi</h1><p>Body <a href=\"/x\">x</a></p></main></body></html>";
        let (base, _seen) = serve(vec![
            (200, "text/html; charset=utf-8", page.into()),
            (200, "application/json", "{\"a\":1}".into()),
            (200, "application/octet-stream", "\u{1}\u{2}".into()),
            (404, "text/plain", "gone".into()),
        ])
        .await;
        let web = tools();
        match web.fetch(&format!("{base}/page")).await.expect("fetches") {
            Fetched::Text {
                title, text, url, ..
            } => {
                assert_eq!(title.as_deref(), Some("T"));
                assert_eq!(text, format!("# Hi\n\nBody [x]({base}/x)"));
                assert_eq!(url, format!("{base}/page"));
            }
            other => panic!("{other:?}"),
        }
        match web.fetch(&format!("{base}/a.json")).await.expect("fetches") {
            Fetched::Text { text, .. } => assert_eq!(text, "{\"a\":1}"),
            other => panic!("{other:?}"),
        }
        assert!(
            web.fetch(&format!("{base}/bin"))
                .await
                .unwrap_err()
                .contains("can't be read")
        );
        assert!(
            web.fetch(&format!("{base}/gone"))
                .await
                .unwrap_err()
                .contains("404")
        );
        assert!(
            web.fetch("file:///etc/passwd")
                .await
                .unwrap_err()
                .contains("http")
        );
        assert!(web.fetch("not a url").await.is_err());
    }

    #[test]
    fn slices_say_how_to_read_on() {
        let text = "abcdefghij";
        assert_eq!(page_slice(text, 0, 20), "abcdefghij");
        let s = page_slice(text, 2, 3);
        assert!(s.starts_with("cde\n\n[Cut at character 5 of 10."), "{s}");
        assert!(page_slice(text, 50, 3).contains("only 10 characters"));
    }
}
