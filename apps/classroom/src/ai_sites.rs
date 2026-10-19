//! AI and chatbot websites: the Teacher's editable list, blocking it as one
//! switch, and noticing a Student on one of them.
//!
//! Blocking reuses the Internet rule a Student already enforces: the Teacher
//! sends the *combination* of its own Internet rule and this list, so a site is
//! blocked when either asks for it (OR), and a conflict is always settled in
//! favour of blocking. Students need nothing new.
use crate::guard::{
    rules::{parse_line, Entry, RuleSet},
    InternetPolicy, Mode,
};
use anyhow::Result;
use serde_json::Value;

/// The list the Teacher starts with, and returns to with "Khôi phục danh sách gốc".
///
/// Only host names can be blocked at network level (HTTPS hides paths), so an
/// entry written with a path (`github.com/features/copilot`) blocks its whole
/// site.
pub const DEFAULT_SITES: [&str; 56] = [
    "https://chatgpt.com",
    "https://gemini.google.com",
    "https://claude.ai",
    "https://www.perplexity.ai",
    "https://grok.com",
    "https://copilot.microsoft.com",
    "https://chat.deepseek.com",
    "https://chat.qwen.ai",
    "https://kimi.com",
    "https://www.meta.ai",
    "https://chat.mistral.ai",
    "https://poe.com",
    "https://you.com",
    "https://www.phind.com",
    "https://character.ai",
    "https://pi.ai",
    "https://notebooklm.google.com",
    "https://aistudio.google.com",
    "https://platform.openai.com",
    "https://console.anthropic.com",
    "https://huggingface.co",
    "https://openrouter.ai",
    "https://replicate.com",
    "https://fal.ai",
    "https://www.midjourney.com",
    "https://leonardo.ai",
    "https://ideogram.ai",
    "https://www.adobe.com/products/firefly.html",
    "https://www.canva.com",
    "https://runwayml.com",
    "https://pika.art",
    "https://klingai.com",
    "https://lumalabs.ai",
    "https://suno.com",
    "https://www.udio.com",
    "https://elevenlabs.io",
    "https://www.heygen.com",
    "https://www.synthesia.io",
    "https://www.cursor.com",
    "https://github.com/features/copilot",
    "https://replit.com",
    "https://lovable.dev",
    "https://bolt.new",
    "https://v0.app",
    "https://sora.com",
    "https://gamma.app",
    "https://www.notion.so/product/ai",
    "https://www.deepl.com",
    "https://www.recraft.ai",
    "https://www.krea.ai",
    "https://higgsfield.ai",
    "https://manus.im",
    "https://www.jasper.ai",
    "https://www.capcut.com",
    "https://www.blackbox.ai",
    "https://www.deepai.org",
];

/// Most entries the list may hold (a Student accepts 400 Internet rules).
pub const MAX_SITES: usize = 200;

pub fn defaults() -> Vec<String> {
    DEFAULT_SITES.iter().map(|s| (*s).to_owned()).collect()
}

/// The host name an entry stands for, or an error saying what is wrong.
pub fn host_of(entry: &str) -> Result<String> {
    match parse_line(entry)? {
        Some(Entry::Name(crate::guard::rules::Pattern::Domain(host))) => Ok(host),
        Some(Entry::Name(crate::guard::rules::Pattern::Glob(_))) => {
            anyhow::bail!("không dùng ký tự * ở đây; nhập tên website, ví dụ chatgpt.com")
        }
        Some(Entry::Net(_)) => anyhow::bail!("nhập tên website, không phải địa chỉ IP"),
        None => anyhow::bail!("dòng trống"),
    }
}

/// A clean list from what the Teacher typed: trimmed, each entry checked,
/// duplicates (same host) removed, order kept.
pub fn normalize_list(entries: &[String]) -> Result<Vec<String>> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for raw in entries {
        let entry = raw.trim();
        if entry.is_empty() {
            continue;
        }
        anyhow::ensure!(entry.chars().count() <= 300, "“{entry}”: quá dài");
        let host = host_of(entry).map_err(|e| anyhow::anyhow!("“{entry}”: {e}"))?;
        if seen.insert(host) {
            out.push(entry.to_owned());
        }
    }
    anyhow::ensure!(out.len() <= MAX_SITES, "Tối đa {MAX_SITES} website");
    Ok(out)
}

/// The Internet rule a Student must enforce so that it blocks everything the
/// Teacher's own `internet` rule blocks **and** every site in `sites`.
pub fn merge_into(internet: &InternetPolicy, sites: &[String]) -> InternetPolicy {
    let sites: Vec<String> = sites
        .iter()
        .filter(|s| host_of(s).is_ok())
        .cloned()
        .collect();
    if sites.is_empty() {
        return internet.clone();
    }
    match internet.mode {
        // Already blocks everything.
        Mode::BlockAll => internet.clone(),
        Mode::Open => InternetPolicy {
            mode: Mode::BlockOnly,
            rules: sites,
        },
        Mode::BlockOnly => {
            let mut rules = internet.rules.clone();
            let have: std::collections::HashSet<String> =
                rules.iter().filter_map(|r| host_of(r).ok()).collect();
            rules.extend(
                sites
                    .into_iter()
                    .filter(|s| host_of(s).is_ok_and(|h| !have.contains(&h))),
            );
            InternetPolicy {
                mode: Mode::BlockOnly,
                rules,
            }
        }
        Mode::AllowOnly => {
            // Block wins: anything the allow-list lets through that is an AI
            // site (or a pattern covering one) is taken out of it.
            let ai: Vec<String> = sites.iter().filter_map(|s| host_of(s).ok()).collect();
            let ai_set = RuleSet::parse(&ai).unwrap_or_default();
            let rules: Vec<String> = internet
                .rules
                .iter()
                .filter(|rule| {
                    // Out if it is an AI site or one of its sub-domains, or a
                    // pattern wide enough to let an AI site through.
                    let inside_ai = host_of(rule).is_ok_and(|host| ai_set.matches_name(&host));
                    let covers_ai = RuleSet::parse(std::slice::from_ref(*rule))
                        .is_ok_and(|set| ai.iter().any(|host| set.matches_name(host)));
                    !(inside_ai || covers_ai)
                })
                .cloned()
                .collect();
            let anything_left = rules.iter().any(|r| matches!(parse_line(r), Ok(Some(_))));
            if anything_left {
                InternetPolicy {
                    mode: Mode::AllowOnly,
                    rules,
                }
            } else {
                InternetPolicy {
                    mode: Mode::BlockAll,
                    rules: Vec::new(),
                }
            }
        }
    }
}

/// `input` of a `tool_policy` command with its Internet rule replaced by the
/// combination with `sites` (when AI blocking is on); anything unreadable is
/// returned untouched.
pub fn effective_policy_input(input: &Value, sites: Option<&[String]>) -> Value {
    let Some(sites) = sites else {
        return input.clone();
    };
    let Ok(internet) =
        serde_json::from_value::<InternetPolicy>(input["policy"]["internet"].clone())
    else {
        return input.clone();
    };
    let mut out = input.clone();
    if let Ok(merged) = serde_json::to_value(merge_into(&internet, sites)) {
        out["policy"]["internet"] = merged;
    }
    out
}

/// How a tab title identifies a service when its address is not known yet.
enum Title {
    /// The word may appear anywhere (distinctive brand names).
    Anywhere(&'static str),
    /// The title is the name, or starts with it followed by a separator
    /// (`Claude`, `Claude - New chat`): names that are also ordinary words.
    Leading(&'static str),
}

/// Titles that give a listed site away. Only for hosts in the Teacher's list.
const TITLES: &[(&str, &[Title])] = &[
    ("chatgpt.com", &[Title::Anywhere("chatgpt")]),
    ("claude.ai", &[Title::Leading("claude")]),
    (
        "gemini.google.com",
        &[Title::Leading("gemini"), Title::Leading("google gemini")],
    ),
    ("www.perplexity.ai", &[Title::Anywhere("perplexity")]),
    ("grok.com", &[Title::Leading("grok")]),
    (
        "copilot.microsoft.com",
        &[
            Title::Leading("microsoft copilot"),
            Title::Leading("copilot"),
        ],
    ),
    ("chat.deepseek.com", &[Title::Anywhere("deepseek")]),
    ("chat.qwen.ai", &[Title::Anywhere("qwen")]),
    ("kimi.com", &[Title::Leading("kimi")]),
    (
        "chat.mistral.ai",
        &[Title::Leading("le chat"), Title::Leading("mistral")],
    ),
    ("poe.com", &[Title::Leading("poe")]),
    ("notebooklm.google.com", &[Title::Anywhere("notebooklm")]),
    (
        "aistudio.google.com",
        &[
            Title::Leading("google ai studio"),
            Title::Leading("ai studio"),
        ],
    ),
    ("huggingface.co", &[Title::Anywhere("hugging face")]),
    ("openrouter.ai", &[Title::Anywhere("openrouter")]),
    ("www.midjourney.com", &[Title::Anywhere("midjourney")]),
    ("character.ai", &[Title::Anywhere("character.ai")]),
    ("elevenlabs.io", &[Title::Anywhere("elevenlabs")]),
    ("www.heygen.com", &[Title::Anywhere("heygen")]),
    ("www.synthesia.io", &[Title::Anywhere("synthesia")]),
    ("lovable.dev", &[Title::Leading("lovable")]),
    ("sora.com", &[Title::Leading("sora")]),
];

fn title_matches(title: &str, pattern: &Title) -> bool {
    let title = title.trim().to_lowercase();
    match pattern {
        Title::Anywhere(word) => title.contains(word),
        // The name alone, or the name then a dash/bar/colon ("Claude - New chat"),
        // never the name followed by more words ("Claude Monet").
        Title::Leading(word) => title.strip_prefix(word).is_some_and(|rest| {
            let rest = rest.trim_start();
            rest.is_empty()
                || rest
                    .chars()
                    .next()
                    .is_some_and(|c| matches!(c, '-' | '–' | '—' | '|' | ':' | '·'))
        }),
    }
}

/// The path part of a listed entry (`/features/copilot`), lower-cased and
/// without a trailing slash; `None` for a plain host.
fn path_of(entry: &str) -> Option<String> {
    let rest = entry
        .trim()
        .split_once("://")
        .map_or(entry.trim(), |(_, r)| r);
    let slash = rest.find('/')?;
    let end = rest.find(['?', '#']).unwrap_or(rest.len());
    let path = rest[slash..end.max(slash)]
        .trim_end_matches('/')
        .to_ascii_lowercase();
    (!path.is_empty()).then_some(path)
}

/// Finds which listed sites a set of addresses belongs to. An address is
/// `host` or `host/path`; an entry that names a path (`github.com/features/copilot`)
/// only matches addresses under that path, so ordinary pages of the same site
/// (the rest of GitHub) are not mistaken for it.
#[derive(Clone, Default)]
pub struct Matcher {
    entries: Vec<(String, RuleSet, Option<String>)>,
}
impl Matcher {
    pub fn new(sites: &[String]) -> Self {
        Self {
            entries: sites
                .iter()
                .filter_map(|site| {
                    let host = host_of(site).ok()?;
                    let set = RuleSet::parse(std::slice::from_ref(site)).ok()?;
                    Some((host, set, path_of(site)))
                })
                .collect(),
        }
    }
    /// What an entry is called when reported: its host, plus its path if it has one.
    fn label(host: &str, path: &Option<String>) -> String {
        format!("{host}{}", path.as_deref().unwrap_or(""))
    }
    /// The listed entry (label) that `address` falls under, if any.
    pub fn matching(&self, address: &str) -> Option<String> {
        let address = address.trim().to_ascii_lowercase();
        let (host, path) = address
            .split_once('/')
            .map_or((address.as_str(), ""), |(h, p)| (h, p));
        let path = format!("/{path}");
        self.entries.iter().find_map(|(listed, set, wanted)| {
            let hit = set.matches_name(host)
                && wanted
                    .as_deref()
                    .is_none_or(|prefix| path == prefix || path.starts_with(&format!("{prefix}/")));
            hit.then(|| Self::label(listed, wanted))
        })
    }
    /// The listed hosts recognised from tab titles, for tabs whose address
    /// is not known (yet). A page title is a hint, not proof, so this is only
    /// used to warn the Teacher, never to block.
    pub fn hits_in_titles(&self, titles: &[String]) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for (listed, _, path) in &self.entries {
            if path.is_some() {
                continue; // a page inside a site cannot be told from its title
            }
            let Some((_, patterns)) = TITLES.iter().find(|(host, _)| host == listed) else {
                continue;
            };
            if titles
                .iter()
                .any(|t| patterns.iter().any(|p| title_matches(t, p)))
                && !out.contains(listed)
            {
                out.push(listed.clone());
            }
        }
        out
    }

    /// The listed entries (without duplicates) that any of `addresses` falls under.
    pub fn hits(&self, addresses: &[String]) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for address in addresses {
            if let Some(label) = self.matching(address) {
                if !out.contains(&label) {
                    out.push(label);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(mode: Mode, rules: &[&str]) -> InternetPolicy {
        InternetPolicy {
            mode,
            rules: rules.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    #[test]
    fn the_shipped_list_is_complete_valid_and_unique() {
        assert_eq!(DEFAULT_SITES.len(), 56);
        let list = defaults();
        let clean = normalize_list(&list).unwrap();
        assert_eq!(clean.len(), 56, "no entry is rejected or merged");
        assert_eq!(
            host_of("https://www.adobe.com/products/firefly.html").unwrap(),
            "www.adobe.com"
        );
        assert_eq!(host_of("https://chatgpt.com").unwrap(), "chatgpt.com");
    }

    #[test]
    fn typed_entries_are_checked_and_duplicates_dropped() {
        let ok = normalize_list(&[
            " chatgpt.com ".into(),
            "https://CHATGPT.com/".into(),
            "".into(),
            "claude.ai/new".into(),
        ])
        .unwrap();
        assert_eq!(ok, ["chatgpt.com", "claude.ai/new"]);
        assert!(normalize_list(&["10.0.0.1".into()]).is_err());
        assert!(normalize_list(&["*.ai".into()]).is_err());
        assert!(normalize_list(&["not a host!".into()]).is_err());
        let many: Vec<String> = (0..=MAX_SITES)
            .map(|i| format!("site{i}.example"))
            .collect();
        assert!(normalize_list(&many).is_err());
    }

    #[test]
    fn any_internet_rule_plus_the_ai_list_blocks_the_union() {
        let ai = vec!["https://chatgpt.com".to_owned(), "claude.ai".to_owned()];
        // No other rule: the AI sites are blocked, nothing else.
        let open = merge_into(&InternetPolicy::default(), &ai);
        assert_eq!(open.mode, Mode::BlockOnly);
        assert_eq!(open.rules, ai);
        // Teacher already blocks some sites (one of them an AI site): the union, once.
        let both = merge_into(
            &policy(Mode::BlockOnly, &["facebook.com", "claude.ai"]),
            &ai,
        );
        assert_eq!(both.mode, Mode::BlockOnly);
        assert_eq!(
            both.rules,
            ["facebook.com", "claude.ai", "https://chatgpt.com"]
        );
        // Everything is already blocked.
        assert_eq!(
            merge_into(&policy(Mode::BlockAll, &[]), &ai),
            policy(Mode::BlockAll, &[])
        );
        // Switch off / empty list: the Teacher's own rule exactly.
        assert_eq!(
            merge_into(&policy(Mode::BlockOnly, &["a.com"]), &[]),
            policy(Mode::BlockOnly, &["a.com"])
        );
    }

    #[test]
    fn blocking_wins_over_an_allow_list() {
        let ai = vec!["chatgpt.com".to_owned()];
        // The allow-list names an AI site and its sub-domain: both come out.
        let allow = merge_into(
            &policy(
                Mode::AllowOnly,
                &["wikipedia.org", "chatgpt.com", "api.chatgpt.com"],
            ),
            &ai,
        );
        assert_eq!(allow, policy(Mode::AllowOnly, &["wikipedia.org"]));
        // A wildcard that would let it through is dropped too.
        let glob = merge_into(&policy(Mode::AllowOnly, &["*.com", "edu.vn"]), &ai);
        assert_eq!(glob, policy(Mode::AllowOnly, &["edu.vn"]));
        // Nothing left to allow: everything is blocked.
        let none = merge_into(&policy(Mode::AllowOnly, &["chatgpt.com"]), &ai);
        assert_eq!(none.mode, Mode::BlockAll);
    }

    #[test]
    fn the_combined_rule_is_one_a_student_accepts() {
        let ai = defaults();
        for base in [
            InternetPolicy::default(),
            policy(Mode::BlockOnly, &["facebook.com"]),
            policy(Mode::AllowOnly, &["wikipedia.org"]),
            policy(Mode::BlockAll, &[]),
        ] {
            let merged = merge_into(&base, &ai);
            let candidate = crate::guard::Policy {
                internet: merged,
                ..Default::default()
            };
            candidate.validate().unwrap();
        }
    }

    #[test]
    fn a_policy_command_is_rewritten_only_while_blocking_is_on() {
        let input = serde_json::json!({"policy": {"internet": {"mode": "open", "rules": []}, "copilot_block": true}});
        assert_eq!(effective_policy_input(&input, None), input);
        let sites = vec!["claude.ai".to_owned()];
        let out = effective_policy_input(&input, Some(&sites));
        assert_eq!(out["policy"]["internet"]["mode"], "block_only");
        assert_eq!(out["policy"]["internet"]["rules"][0], "claude.ai");
        assert_eq!(out["policy"]["copilot_block"], true);
        let junk = serde_json::json!({"policy": 5});
        assert_eq!(effective_policy_input(&junk, Some(&sites)), junk);
    }

    #[test]
    fn an_entry_with_a_path_only_matches_that_part_of_the_site() {
        let matcher = Matcher::new(&defaults());
        // Ordinary GitHub is not an AI site; its Copilot page is.
        assert!(matcher
            .hits(&["github.com".into(), "github.com/rust-lang/rust".into()])
            .is_empty());
        assert_eq!(
            matcher.hits(&["github.com/features/copilot".into()]),
            ["github.com/features/copilot"]
        );
        assert_eq!(
            matcher.hits(&["github.com/features/copilot/plans".into()]),
            ["github.com/features/copilot"]
        );
        assert!(matcher
            .hits(&["github.com/features/copilotx".into()])
            .is_empty());
        assert!(matcher.hits(&["www.notion.so/product".into()]).is_empty());
        assert_eq!(
            matcher.hits(&["www.notion.so/product/ai".into()]),
            ["www.notion.so/product/ai"]
        );
        assert_eq!(
            matcher.matching("CHATGPT.com/c/123").as_deref(),
            Some("chatgpt.com")
        );
    }

    #[test]
    fn a_page_title_gives_the_site_away_before_the_address_is_known() {
        let matcher = Matcher::new(&defaults());
        let titles = |t: &[&str]| t.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            matcher.hits_in_titles(&titles(&["ChatGPT"])),
            ["chatgpt.com"]
        );
        assert_eq!(
            matcher.hits_in_titles(&titles(&["Giải bài tập - ChatGPT"])),
            ["chatgpt.com"]
        );
        assert_eq!(matcher.hits_in_titles(&titles(&["Claude"])), ["claude.ai"]);
        assert_eq!(
            matcher.hits_in_titles(&titles(&["Claude - New chat"])),
            ["claude.ai"]
        );
        assert_eq!(
            matcher.hits_in_titles(&titles(&["Google Gemini"])),
            ["gemini.google.com"]
        );
        // Ordinary pages that merely contain a common word are left alone.
        for innocent in [
            "Claude Monet - Wikipedia",
            "Edgar Allan Poe",
            "Poetry",
            "Gemini (constellation)",
            "tinteo.vn",
            "Sora Aoi",
        ] {
            assert!(
                matcher.hits_in_titles(&titles(&[innocent])).is_empty(),
                "{innocent} is not an AI site"
            );
        }
        // Only for sites the Teacher still lists.
        let trimmed = Matcher::new(&["claude.ai".to_owned()]);
        assert!(trimmed.hits_in_titles(&titles(&["ChatGPT"])).is_empty());
        assert_eq!(trimmed.hits_in_titles(&titles(&["Claude"])), ["claude.ai"]);
    }

    #[test]
    fn a_student_on_an_ai_site_is_noticed_even_in_a_background_tab() {
        let matcher = Matcher::new(&defaults());
        let hits = matcher.hits(&[
            "www.google.com".into(),
            "chatgpt.com".into(),
            "docs.rs".into(),
        ]);
        assert_eq!(hits, ["chatgpt.com"]);
        // Sub-domains of a listed host count; unrelated look-alikes do not.
        assert_eq!(
            matcher.hits(&["api.openai.com".into()]),
            Vec::<String>::new()
        );
        assert_eq!(
            matcher.hits(&["platform.openai.com".into()]),
            ["platform.openai.com"]
        );
        assert_eq!(
            matcher.hits(&["notchatgpt.com".into()]),
            Vec::<String>::new()
        );
        assert!(matcher.hits(&[]).is_empty());
    }
}
