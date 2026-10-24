//! A local language model, talked to over HTTP on the user's own machine.
//!
//! ## Why local, and why this is not a server dependency
//!
//! CrawlStat's promise is that crawl data never leaves the machine. An SEO
//! report written by a cloud model would break that promise in the most
//! literal way possible — it would post the customer's site structure, titles
//! and problems to someone else's computer.
//!
//! [Ollama](https://ollama.com) runs the model on the same machine. The
//! default endpoint is `http://127.0.0.1:11434`, a loopback address: the
//! request cannot leave the host. So the feature is genuinely private, and it
//! does not violate the rule about server dependencies either, because
//! **nothing here is required**. The action plan is computed in full by
//! [`crate::plan`] with no model involved; this only writes prose on top of
//! numbers that already exist. With Ollama absent the app loses a paragraph,
//! not a feature.
//!
//! ## Thinking models
//!
//! Qwen 3, DeepSeek R1 and friends "think" before they answer, and Ollama
//! counts that thinking against the same token budget as the answer. Left on,
//! a report request spends its whole budget reasoning and comes back empty.
//! So every request turns thinking off, and anything that still arrives
//! wrapped in `<think>` tags is dropped before it reaches the user.
//!
//! ## What is sent
//!
//! A compact brief: the ranked jobs, their counts and scores, and a handful of
//! example URLs. Not page bodies, not the crawl database. Small enough to fit
//! a modest context window and to be read in full by anybody who wants to
//! check what their machine is doing.

use std::io::{BufRead, BufReader};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::plan::{ActionPlan, Effort};

/// Where Ollama listens unless the user says otherwise.
///
/// Loopback by design. If someone points this at a remote host they have
/// chosen to send their data there, and the UI says so plainly.
pub const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:11434";

/// A reasonable default for a machine that can run a small model comfortably.
pub const DEFAULT_MODEL: &str = "qwen3:8b";

#[derive(Debug, thiserror::Error)]
pub enum OllamaError {
    #[error("could not reach Ollama at {endpoint} — is it running? ({source})")]
    Unreachable {
        endpoint: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    #[error("Ollama answered {status}: {body}")]
    Refused { status: u16, body: String },
    #[error("the model '{0}' is not installed — run `ollama pull {0}`")]
    NoSuchModel(String),
    #[error("Ollama's reply could not be read: {0}")]
    BadResponse(String),
}

/// One model the local Ollama has available.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledModel {
    pub name: String,
    pub size_bytes: u64,
    /// False for embedding-only models, which cannot write anything.
    pub generates: bool,
}

#[derive(Deserialize)]
struct TagsResponse {
    #[serde(default)]
    models: Vec<TagEntry>,
}

#[derive(Deserialize)]
struct TagEntry {
    name: String,
    #[serde(default)]
    size: u64,
    /// Missing on older Ollama versions, which is read as "can generate".
    #[serde(default)]
    capabilities: Vec<String>,
}

#[derive(Serialize)]
struct GenerateRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    system: &'a str,
    stream: bool,
    /// `None` leaves the field out, for an Ollama that rejects it.
    #[serde(skip_serializing_if = "Option::is_none")]
    think: Option<bool>,
    options: GenerateOptions,
}

#[derive(Serialize)]
struct GenerateOptions {
    /// Low, not zero: the same crawl should give the same advice twice, but a
    /// model pinned to zero writes noticeably worse prose.
    temperature: f32,
    num_predict: i32,
}

/// One line of a streamed reply.
#[derive(Deserialize)]
struct GenerateChunk {
    #[serde(default)]
    response: String,
    #[serde(default)]
    done: bool,
    #[serde(default)]
    error: Option<String>,
}

fn client(timeout: Duration) -> reqwest::blocking::Client {
    // Building a client panics without one, even for plain-HTTP loopback, and
    // this may be the first network call the process makes.
    crate::fetch::install_crypto_provider();
    reqwest::blocking::Client::builder()
        .timeout(timeout)
        .build()
        .expect("a blocking client with a timeout is always constructible")
}

/// Which models the local Ollama has, so the UI can offer a list rather than
/// a text field the user has to spell correctly.
pub fn installed_models(endpoint: &str) -> Result<Vec<InstalledModel>, OllamaError> {
    let url = format!("{}/api/tags", endpoint.trim_end_matches('/'));
    let response = client(Duration::from_secs(5))
        .get(&url)
        .send()
        .map_err(|error| OllamaError::Unreachable {
            endpoint: endpoint.to_string(),
            source: Box::new(error),
        })?;

    let status = response.status();
    if !status.is_success() {
        return Err(OllamaError::Refused {
            status: status.as_u16(),
            body: response.text().unwrap_or_default(),
        });
    }

    let tags: TagsResponse = response
        .json()
        .map_err(|error| OllamaError::BadResponse(error.to_string()))?;
    Ok(tags
        .models
        .into_iter()
        .map(|entry| InstalledModel {
            generates: entry.capabilities.is_empty()
                || entry.capabilities.iter().any(|c| c == "completion"),
            name: entry.name,
            size_bytes: entry.size,
        })
        .collect())
}

/// The installed model to use when the user asked for `wanted`.
///
/// Ollama names carry an implicit `:latest`, so `llama3` and `llama3:latest`
/// are the same model. When the wanted model is not installed at all, the
/// first one that can write is used instead: someone who has a model running
/// wants a report, not an instruction to download a different one. Ollama
/// lists the most recently used model first, which is the best guess at the
/// one they meant.
pub fn pick_model(installed: &[InstalledModel], wanted: &str) -> Option<String> {
    let base = |name: &str| name.strip_suffix(":latest").unwrap_or(name).to_string();
    let wanted = base(wanted.trim());
    installed
        .iter()
        .find(|model| base(&model.name) == wanted)
        .or_else(|| installed.iter().find(|model| model.generates))
        .map(|model| model.name.clone())
}

/// Ask the model to write up the plan.
///
/// Blocking and generous with its timeout: a local model on a laptop takes
/// tens of seconds, and the alternative — a half-written report — is worse
/// than a wait the UI has already warned about.
pub fn summarise(
    endpoint: &str,
    model: &str,
    plan: &ActionPlan,
    site: &str,
) -> Result<String, OllamaError> {
    summarise_streaming(endpoint, model, plan, site, |_| true)
}

/// [`summarise`], reporting the report as it is written.
///
/// `progress` is handed the whole visible text so far, not the latest
/// fragment, and returns false to stop early — the user left the screen.
pub fn summarise_streaming(
    endpoint: &str,
    model: &str,
    plan: &ActionPlan,
    site: &str,
    mut progress: impl FnMut(&str) -> bool,
) -> Result<String, OllamaError> {
    let url = format!("{}/api/generate", endpoint.trim_end_matches('/'));
    let brief = brief(plan, site);
    let request = |think| GenerateRequest {
        model,
        system: SYSTEM_PROMPT,
        prompt: &brief,
        stream: true,
        think,
        options: GenerateOptions {
            temperature: 0.3,
            num_predict: 1_600,
        },
    };
    let send = |body: &GenerateRequest| {
        client(Duration::from_secs(600))
            .post(&url)
            .json(body)
            .send()
            .map_err(|error| OllamaError::Unreachable {
                endpoint: endpoint.to_string(),
                source: Box::new(error),
            })
    };

    let mut response = send(&request(Some(false)))?;
    // An Ollama old enough not to know about thinking, or a model it refuses
    // the flag for, gets the request again without it.
    if response.status() == reqwest::StatusCode::BAD_REQUEST {
        let body = response.text().unwrap_or_default();
        if !body.contains("think") {
            return Err(OllamaError::Refused { status: 400, body });
        }
        response = send(&request(None))?;
    }

    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(OllamaError::NoSuchModel(model.to_string()));
    }
    if !status.is_success() {
        return Err(OllamaError::Refused {
            status: status.as_u16(),
            body: response.text().unwrap_or_default(),
        });
    }

    let mut raw = String::new();
    let mut shown = 0;
    for line in BufReader::new(response).lines() {
        let line = line.map_err(|error| OllamaError::BadResponse(error.to_string()))?;
        if line.trim().is_empty() {
            continue;
        }
        let chunk: GenerateChunk = serde_json::from_str(&line)
            .map_err(|error| OllamaError::BadResponse(error.to_string()))?;
        if let Some(error) = chunk.error {
            return Err(OllamaError::Refused {
                status: 500,
                body: error,
            });
        }
        raw.push_str(&chunk.response);

        let visible = without_thinking(&raw);
        if visible.len() > shown {
            shown = visible.len();
            if !progress(visible) {
                break;
            }
        }
        if chunk.done {
            break;
        }
    }
    Ok(without_thinking(&raw).trim_end().to_string())
}

/// The part of a reply meant for the reader.
///
/// Grows only by appending as more of the reply arrives, which is what lets
/// the UI show it while it streams: a half-open `<think>` block, or the first
/// few characters of its opening tag, shows as nothing rather than as noise.
fn without_thinking(raw: &str) -> &str {
    const OPEN: &str = "<think>";
    const CLOSE: &str = "</think>";
    let text = raw.trim_start();
    if let Some(rest) = text.strip_prefix(OPEN) {
        return match rest.find(CLOSE) {
            Some(end) => rest[end + CLOSE.len()..].trim_start(),
            None => "",
        };
    }
    if OPEN.starts_with(text) {
        ""
    } else {
        text
    }
}

const SYSTEM_PROMPT: &str = "\
You are a senior technical SEO consultant writing the executive summary of \
a site audit for a client who is not an SEO specialist. You are given a ranked \
list of problems found by a crawler, already scored for impact. Explain what \
to do and why, in plain English. Do not re-list the data.

Write Markdown with exactly this structure:

# A one-line verdict on the site, under twelve words

Two or three sentences on the overall picture, using the page counts and the \
share of link value at risk.

## Fix this first
One short paragraph on the single most valuable job and why it beats the rest.

## The plan
1. **Name of the job**: why it matters and what to change, in one or two sentences.
(Three to six numbered items, in the order to do them. Group jobs that share \
a cause, and say when one template change fixes many pages.)

## Quick wins
- Up to three bullets for cheap fixes that reach many pages.

## Can wait
- Up to three bullets for low-impact items, or for things that look \
deliberate rather than broken.

Rules:
- Start directly with the # line. No preamble, no closing remarks.
- Leave out Quick wins or Can wait if nothing belongs there.
- Use only the numbers in the brief. Never invent traffic, rankings, revenue \
or percentages.
- Bold only the one key figure or action in a sentence. No tables, code \
blocks or links.
- Do not mention that you are an AI, and do not describe the brief itself.";

/// The brief sent to the model.
///
/// Built here rather than in the UI so that what leaves the process is defined
/// in one reviewable place. Everything in it is already on the screen the user
/// is looking at.
pub fn brief(plan: &ActionPlan, site: &str) -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    let _ = writeln!(out, "Site: {site}");
    let _ = writeln!(
        out,
        "Pages crawled: {}. Pages with at least one problem: {}.",
        plan.total_pages, plan.affected_pages
    );
    let _ = writeln!(
        out,
        "Share of the site's internal link value affected by an error: {:.0}%.",
        plan.value_at_risk * 100.0
    );
    let _ = writeln!(out, "\nRanked problems (impact is 0-100, relative):");

    // Ten is enough for a summary and keeps the prompt inside a small model's
    // comfortable context.
    for (index, item) in plan.items.iter().take(10).enumerate() {
        let effort = match item.effort {
            Effort::Template => "one template change fixes most of them",
            Effort::Bulk => "a batch of related changes",
            Effort::PerPage => "one change per page",
        };
        let _ = writeln!(
            out,
            "\n{}. {} [{}] — impact {:.0}, {} page(s), {} distinct fix(es), {}.",
            index + 1,
            item.title,
            item.severity.as_str(),
            item.impact,
            item.pages,
            item.distinct_fixes,
            effort
        );
        let _ = writeln!(out, "   Standard advice: {}", item.how_to_fix);
        if let Some(example) = item.examples.first() {
            let _ = writeln!(out, "   Example page: {}", example.url);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::Severity;
    use crate::plan::{PlanExample, PlanItem};

    fn plan_with(items: Vec<PlanItem>) -> ActionPlan {
        ActionPlan {
            items,
            affected_pages: 42,
            total_pages: 100,
            value_at_risk: 0.25,
        }
    }

    fn item(title: &str, effort: Effort) -> PlanItem {
        PlanItem {
            issue_id: "x".into(),
            title: title.into(),
            severity: Severity::Error,
            how_to_fix: "Do the thing.".into(),
            docs_slug: "x".into(),
            pages: 12,
            distinct_fixes: 1,
            leverage: 12.0,
            reach: 0.3,
            impact: 88.0,
            effort,
            examples: vec![PlanExample {
                url: "https://a.test/one".into(),
                detail: None,
                depth: 1,
            }],
        }
    }

    #[test]
    fn the_brief_carries_the_numbers_the_model_must_not_invent() {
        let brief = brief(
            &plan_with(vec![item("Missing titles", Effort::Template)]),
            "a.test",
        );
        assert!(brief.contains("a.test"));
        assert!(brief.contains("100"), "page count missing");
        assert!(brief.contains("Missing titles"));
        assert!(brief.contains("25%"), "value at risk missing");
    }

    #[test]
    fn the_brief_spells_out_effort_in_words_rather_than_a_code() {
        let brief = brief(
            &plan_with(vec![item("Thin content", Effort::PerPage)]),
            "a.test",
        );
        assert!(brief.contains("one change per page"), "{brief}");
    }

    #[test]
    fn the_brief_never_contains_page_content() {
        // The whole privacy argument rests on this: what goes to the model is
        // a summary of findings, not the pages themselves.
        let brief = brief(
            &plan_with(vec![item("Missing titles", Effort::Template)]),
            "a.test",
        );
        assert!(!brief.to_lowercase().contains("<html"));
        assert!(!brief.to_lowercase().contains("<body"));
    }

    #[test]
    fn the_brief_is_capped_so_a_small_model_can_read_it() {
        let items: Vec<PlanItem> = (0..40)
            .map(|i| item(&format!("Issue {i}"), Effort::Bulk))
            .collect();
        let brief = brief(&plan_with(items), "a.test");
        assert!(brief.contains("Issue 9"));
        assert!(!brief.contains("Issue 10"), "more than ten jobs were sent");
    }

    #[test]
    fn an_endpoint_with_a_trailing_slash_does_not_produce_a_double_slash() {
        // Users paste URLs with and without the slash; both must work.
        assert_eq!(
            format!(
                "{}/api/tags",
                "http://127.0.0.1:11434/".trim_end_matches('/')
            ),
            "http://127.0.0.1:11434/api/tags"
        );
    }

    fn installed(name: &str, generates: bool) -> InstalledModel {
        InstalledModel {
            name: name.into(),
            size_bytes: 1,
            generates,
        }
    }

    #[test]
    fn the_wanted_model_is_used_when_it_is_installed() {
        let models = [
            installed("llama3:latest", true),
            installed("qwen3:8b", true),
        ];
        assert_eq!(pick_model(&models, "qwen3:8b").as_deref(), Some("qwen3:8b"));
    }

    #[test]
    fn a_missing_latest_tag_still_finds_the_model() {
        let models = [installed("qwen3.8:latest", true)];
        assert_eq!(
            pick_model(&models, "qwen3.8").as_deref(),
            Some("qwen3.8:latest")
        );
    }

    #[test]
    fn a_model_that_is_not_installed_falls_back_to_one_that_can_write() {
        // The default is qwen3:8b; someone with only qwen3.8 installed must
        // still get a report rather than a download instruction.
        let models = [
            installed("nomic-embed-text:latest", false),
            installed("qwen3.8:latest", true),
        ];
        assert_eq!(
            pick_model(&models, DEFAULT_MODEL).as_deref(),
            Some("qwen3.8:latest")
        );
        assert_eq!(pick_model(&[], DEFAULT_MODEL), None);
    }

    #[test]
    fn thinking_never_reaches_the_reader() {
        assert_eq!(
            without_thinking("<think>hmm</think>\n\n# Report"),
            "# Report"
        );
        assert_eq!(without_thinking("<think>still going"), "");
        assert_eq!(without_thinking("<thi"), "", "a half-arrived tag leaked");
        assert_eq!(without_thinking("\n# Report"), "# Report");
    }

    #[test]
    fn the_default_endpoint_is_loopback() {
        // If this ever stops being true, the privacy claim in this module's
        // documentation stops being true with it.
        assert!(
            DEFAULT_ENDPOINT.contains("127.0.0.1") || DEFAULT_ENDPOINT.contains("localhost"),
            "the default endpoint must not leave the machine"
        );
    }
}
