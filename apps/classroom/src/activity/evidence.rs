//! Choosing the moment to photograph a Student's screen as evidence that an AI
//! website is being used.
//!
//! A picture taken as the page finishes loading shows a login box or an empty
//! chat; one taken too often only overwrites itself. So the planner waits for
//! signs of real use and then spaces the few pictures it is allowed:
//!
//! * the site's tab must be the one in front (otherwise the picture would not
//!   show it) and the Student must have touched keyboard or mouse in the last
//!   20 s;
//! * the first picture is taken when the page visibly changed after loading
//!   (a conversation started: new title or path) and the Student has been there
//!   15 s, or, if the page never changes, after `first_seconds` of use;
//! * later pictures need at least `gap_seconds` of further use, and a changed
//!   page (or twice that time);
//! * at most `per_site` pictures per site per day.
//!
//! Time spent elsewhere is not counted, so switching away and back does not
//! trigger an early picture. Pure logic: no clock, no screen.
use crate::ai_sites::Matcher;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Longest gap between two observations that still counts as continuous use.
const MAX_TICK_MS: i64 = 15_000;
/// Input newer than this means the Student is working in the page.
const ACTIVE_WITHIN_MS: u64 = 20_000;
/// Time on the page before its title/path are taken as the "just loaded" state.
const SETTLE_MS: i64 = 6_000;
/// A page change counts as a conversation after this long.
const CHANGE_AFTER_MS: i64 = 15_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub per_site: u32,
    pub first_seconds: u64,
    pub gap_seconds: u64,
}

/// What the Student is looking at right now.
pub struct Seen<'a> {
    pub now_ms: i64,
    /// `host/path` of the active tab of the front window, if that is a browser.
    pub address: Option<&'a str>,
    pub title: &'a str,
    /// Time since the last keyboard/mouse input; `None` when unknown.
    pub idle_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    /// The listed entry that matched (`chatgpt.com`).
    pub label: String,
    pub address: String,
    pub title: String,
    /// `conversation`, `in_use` or `continued`.
    pub reason: &'static str,
}

#[derive(Default)]
struct Site {
    taken: u32,
    last_capture_ms: Option<i64>,
    /// Active time since the last picture (or first arrival).
    dwell_ms: i64,
    /// Title and path once the page had settled.
    baseline: Option<(String, String)>,
}

/// What survives a restart of the Student program: pictures already taken today.
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq, Eq)]
pub struct Counts {
    pub day: u32,
    pub taken: HashMap<String, u32>,
}

pub struct Planner {
    day: u32,
    sites: HashMap<String, Site>,
    active: Option<String>,
    last_tick: Option<i64>,
}

impl Planner {
    pub fn new(day: u32) -> Self {
        Self {
            day,
            sites: HashMap::new(),
            active: None,
            last_tick: None,
        }
    }

    pub fn restore(counts: &Counts, today: u32) -> Self {
        let mut planner = Self::new(today);
        if counts.day == today {
            for (label, taken) in &counts.taken {
                planner.sites.entry(label.clone()).or_default().taken = *taken;
            }
        }
        planner
    }

    pub fn counts(&self) -> Counts {
        Counts {
            day: self.day,
            taken: self
                .sites
                .iter()
                .filter(|(_, s)| s.taken > 0)
                .map(|(label, s)| (label.clone(), s.taken))
                .collect(),
        }
    }

    /// Looks at the current screen state; returns a plan when a picture should be taken now.
    pub fn observe(
        &mut self,
        matcher: &Matcher,
        seen: &Seen,
        settings: &Settings,
        today: u32,
    ) -> Option<Plan> {
        if today != self.day {
            *self = Self::new(today);
        }
        let dt = self
            .last_tick
            .map_or(0, |last| (seen.now_ms - last).clamp(0, MAX_TICK_MS));
        self.last_tick = Some(seen.now_ms);

        let Some(address) = seen.address else {
            self.active = None;
            return None;
        };
        let Some(label) = matcher.matching(address) else {
            self.active = None;
            return None;
        };
        let site = self.sites.entry(label.clone()).or_default();
        if self.active.as_deref() == Some(label.as_str()) {
            site.dwell_ms += dt;
        } else {
            // Arrived (or came back): the page state is read afresh.
            self.active = Some(label.clone());
            site.baseline = None;
        }
        let now = (seen.title.to_owned(), path_of(address));
        if site.baseline.is_none() && site.dwell_ms >= SETTLE_MS {
            site.baseline = Some(now.clone());
        }
        if site.taken >= settings.per_site {
            return None;
        }
        let gap_ms = i64::try_from(settings.gap_seconds).unwrap_or(i64::MAX) * 1000;
        if site
            .last_capture_ms
            .is_some_and(|last| seen.now_ms - last < gap_ms)
        {
            return None;
        }
        if !seen.idle_ms.is_none_or(|idle| idle < ACTIVE_WITHIN_MS) {
            return None;
        }
        let changed = site.baseline.as_ref().is_some_and(|b| *b != now);
        let first_ms = i64::try_from(settings.first_seconds).unwrap_or(i64::MAX) * 1000;
        let reason = if site.taken == 0 {
            if changed && site.dwell_ms >= CHANGE_AFTER_MS {
                Some("conversation")
            } else if site.dwell_ms >= first_ms {
                Some("in_use")
            } else {
                None
            }
        } else if site.dwell_ms >= gap_ms && (changed || site.dwell_ms >= 2 * gap_ms) {
            Some("continued")
        } else {
            None
        };
        let reason = reason?;
        site.taken += 1;
        site.last_capture_ms = Some(seen.now_ms);
        site.dwell_ms = 0;
        site.baseline = Some(now);
        Some(Plan {
            label,
            address: address.to_owned(),
            title: seen.title.to_owned(),
            reason,
        })
    }
}

fn path_of(address: &str) -> String {
    address.split_once('/').map_or(String::new(), |(_, p)| p.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matcher() -> Matcher {
        Matcher::new(&[
            "https://chatgpt.com".to_owned(),
            "https://claude.ai".to_owned(),
            "https://github.com/features/copilot".to_owned(),
        ])
    }
    fn settings() -> Settings {
        Settings { per_site: 3, first_seconds: 60, gap_seconds: 180 }
    }
    /// Feeds one observation per `step` seconds for `seconds`, returning the
    /// second at which each plan was produced.
    fn run(
        planner: &mut Planner,
        from: i64,
        seconds: i64,
        step: i64,
        address: Option<&str>,
        title: impl Fn(i64) -> String,
        idle: Option<u64>,
    ) -> Vec<(i64, Plan)> {
        let m = matcher();
        let mut out = Vec::new();
        let mut t = 0;
        while t <= seconds {
            let title = title(t);
            let seen = Seen { now_ms: (from + t) * 1000, address, title: &title, idle_ms: idle };
            if let Some(plan) = planner.observe(&m, &seen, &settings(), 20261016) {
                out.push((from + t, plan));
            }
            t += step;
        }
        out
    }
    fn fixed(t: &str) -> impl Fn(i64) -> String + '_ {
        move |_| t.to_owned()
    }

    #[test]
    fn a_page_that_just_loaded_is_not_photographed() {
        let mut p = Planner::new(20261016);
        // 50 s on an unchanged page, typing the whole time: still too early.
        let plans = run(&mut p, 0, 50, 5, Some("chatgpt.com"), fixed("ChatGPT"), Some(1000));
        assert!(plans.is_empty());
        // ...a little later it is taken, once the Student has really been using it.
        let plans = run(&mut p, 55, 20, 5, Some("chatgpt.com"), fixed("ChatGPT"), Some(1000));
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].1.reason, "in_use");
        assert!((60..=65).contains(&plans[0].0), "taken at {}", plans[0].0);
    }

    #[test]
    fn a_conversation_starting_is_caught_soon_after() {
        let mut p = Planner::new(20261016);
        // Loads as "ChatGPT" (/), then the chat gets a title and a /c/ address at 30 s.
        let m = matcher();
        let mut plans = Vec::new();
        for t in (0..=60).step_by(3) {
            let (addr, title) = if t < 30 {
                ("chatgpt.com", "ChatGPT")
            } else {
                ("chatgpt.com/c/abc", "Giải bài tập hình học")
            };
            let seen = Seen { now_ms: t * 1000, address: Some(addr), title, idle_ms: Some(2000) };
            if let Some(plan) = p.observe(&m, &seen, &settings(), 20261016) {
                plans.push((t, plan));
            }
        }
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].1.reason, "conversation");
        assert_eq!(plans[0].1.label, "chatgpt.com");
        assert!(plans[0].0 >= 30 && plans[0].0 <= 36, "taken at {}", plans[0].0);
        assert_eq!(plans[0].1.title, "Giải bài tập hình học");
    }

    #[test]
    fn time_away_from_the_page_does_not_count() {
        let mut p = Planner::new(20261016);
        assert!(run(&mut p, 0, 40, 5, Some("chatgpt.com"), fixed("ChatGPT"), Some(500)).is_empty());
        // Five minutes in another window (front window is not a browser).
        assert!(run(&mut p, 45, 300, 5, None, fixed("Code"), Some(500)).is_empty());
        // Back on the page: only ~40 s of use so far, so nothing yet...
        let early = run(&mut p, 350, 10, 5, Some("chatgpt.com"), fixed("ChatGPT"), Some(500));
        assert!(early.is_empty());
        // ...and it is taken once ~60 s of use have really accumulated.
        let later = run(&mut p, 365, 40, 5, Some("chatgpt.com"), fixed("ChatGPT"), Some(500));
        assert_eq!(later.len(), 1);
    }

    #[test]
    fn nothing_is_taken_while_the_student_is_not_touching_the_computer() {
        let mut p = Planner::new(20261016);
        let idle = run(&mut p, 0, 200, 5, Some("chatgpt.com"), fixed("ChatGPT"), Some(120_000));
        assert!(idle.is_empty(), "walked away from the keyboard");
        let back = run(&mut p, 205, 10, 5, Some("chatgpt.com"), fixed("ChatGPT"), Some(1_000));
        assert_eq!(back.len(), 1, "the moment they type again");
    }

    #[test]
    fn pictures_are_spaced_and_capped_per_site() {
        let mut p = Planner::new(20261016);
        let mut title = 0;
        let mut all = Vec::new();
        // An hour of steady use with the page changing every minute.
        let m = matcher();
        for t in (0..=3600).step_by(5) {
            if t % 60 == 0 {
                title += 1;
            }
            let name = format!("Cuộc trò chuyện {title}");
            let seen = Seen { now_ms: t * 1000, address: Some("chatgpt.com/c/x"), title: &name, idle_ms: Some(1000) };
            if p.observe(&m, &seen, &settings(), 20261016).is_some() {
                all.push(t);
            }
        }
        assert_eq!(all.len(), 3, "never more than three: {all:?}");
        assert!(all[1] - all[0] >= 180 && all[2] - all[1] >= 180, "spaced out: {all:?}");
    }

    #[test]
    fn each_site_has_its_own_allowance_and_other_sites_are_ignored() {
        let mut p = Planner::new(20261016);
        let a = run(&mut p, 0, 70, 5, Some("chatgpt.com"), fixed("ChatGPT"), Some(100));
        let b = run(&mut p, 75, 70, 5, Some("claude.ai"), fixed("Claude"), Some(100));
        assert_eq!((a.len(), b.len()), (1, 1));
        assert_eq!(b[0].1.label, "claude.ai");
        let other = run(&mut p, 150, 600, 5, Some("tinteo.vn"), fixed("Tin tức"), Some(100));
        assert!(other.is_empty());
        // A site listed with a path is only watched under that path.
        let github = run(&mut p, 800, 200, 5, Some("github.com/rust-lang/rust"), fixed("rust"), Some(100));
        assert!(github.is_empty());
        let copilot = run(&mut p, 1100, 80, 5, Some("github.com/features/copilot"), fixed("Copilot"), Some(100));
        assert_eq!(copilot.len(), 1);
    }

    #[test]
    fn a_new_day_starts_again_and_counts_survive_a_restart() {
        let mut p = Planner::new(20261016);
        assert_eq!(run(&mut p, 0, 70, 5, Some("chatgpt.com"), fixed("ChatGPT"), Some(100)).len(), 1);
        let saved = p.counts();
        assert_eq!(saved.taken["chatgpt.com"], 1);
        // Restarted the same day: the picture already taken is remembered.
        let mut again = Planner::restore(&saved, 20261016);
        assert_eq!(again.counts().taken["chatgpt.com"], 1);
        let s = Settings { per_site: 1, ..settings() };
        let m = matcher();
        let seen = Seen { now_ms: 10_000_000, address: Some("chatgpt.com"), title: "x", idle_ms: Some(0) };
        for i in 0..100 {
            let seen = Seen { now_ms: seen.now_ms + i * 5000, ..Seen { ..seen_clone(&seen) } };
            assert!(again.observe(&m, &seen, &s, 20261016).is_none(), "allowance already used");
        }
        // The next day the old counts are void.
        let next = Planner::restore(&saved, 20261017);
        assert!(next.counts().taken.is_empty());
        // And a planner that sees the date change resets itself.
        let mut p2 = Planner::restore(&saved, 20261016);
        let seen = Seen { now_ms: 0, address: None, title: "", idle_ms: None };
        p2.observe(&m, &seen, &settings(), 20261017);
        assert!(p2.counts().taken.is_empty());
    }
    fn seen_clone<'a>(s: &Seen<'a>) -> Seen<'a> {
        Seen { now_ms: s.now_ms, address: s.address, title: s.title, idle_ms: s.idle_ms }
    }

    #[test]
    fn unknown_idle_time_is_treated_as_active() {
        let mut p = Planner::new(20261016);
        let plans = run(&mut p, 0, 70, 5, Some("chatgpt.com"), fixed("ChatGPT"), None);
        assert_eq!(plans.len(), 1, "platforms that cannot report idle time still work");
    }
}
