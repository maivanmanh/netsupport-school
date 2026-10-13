//! The Teacher's memory of the rules it has set for the class today.
//!
//! Enforcement itself lives on each Student (and persists there across
//! restarts and lost connections). This is the other half: so that a computer
//! which was off, joins late, or comes back after a Student reinstall still
//! gets every rule, the Teacher keeps what it last asked for — and the
//! Teacher itself can restart without forgetting. Rules are stamped with the
//! local day and are all void on any other day, matching the Students.
use crate::{
    app_control::{policy_hash as app_hash, AppPolicy},
    guard::{fingerprint as internet_fingerprint, InternetPolicy},
    vscode_control::VsCodePolicy,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Scoped<T> {
    pub value: T,
    /// `None`: every computer. `Some`: only these Student ids.
    pub scope: Option<Vec<String>>,
}
impl<T> Scoped<T> {
    pub fn new(value: T, scope: Option<Vec<String>>) -> Self {
        Self { value, scope }
    }
    pub fn covers(&self, student: &str) -> bool {
        self.scope.as_ref().is_none_or(|ids| ids.iter().any(|id| id == student))
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ClassRules {
    /// Local day (`yyyymmdd`) these rules were set.
    pub day: u32,
    pub internet: Option<Scoped<InternetPolicy>>,
    pub copilot: Option<Scoped<bool>>,
    pub vscode: Option<Scoped<VsCodePolicy>>,
    pub apps: Option<Scoped<AppPolicy>>,
    pub clipboard: Option<Scoped<bool>>,
    /// Class-wide activity collection switch.
    pub activity: bool,
}

/// One corrective job for one Student: the Teacher action name and its input.
pub type Fix = (&'static str, Value);

impl ClassRules {
    /// Rules from disk, or none if they are from another day or unreadable.
    pub fn load(doc: &classroom_store::Doc) -> Self {
        let today = crate::local_day::today();
        match doc.read::<Self>().ok().flatten() {
            Some(rules) if rules.day == today => rules,
            _ => Self { day: today, ..Self::default() },
        }
    }
    pub fn save(&self, doc: &classroom_store::Doc) -> anyhow::Result<()> {
        doc.write(self)?;
        Ok(())
    }
    /// Starts a new day: every rule is dropped. Returns whether anything changed.
    pub fn roll_over(&mut self) -> bool {
        let today = crate::local_day::today();
        if self.day == today {
            return false;
        }
        *self = Self { day: today, ..Self::default() };
        true
    }

    /// What this Student still needs, judged from the state it reports
    /// (`caps`: its capability document) against the rules that cover it.
    pub fn fixes_for(&self, student: &str, caps: &Value) -> Vec<Fix> {
        let mut fixes = Vec::new();
        if caps["guard"] != 1 {
            return fixes; // cannot enforce anything (not Windows/administrator)
        }
        if let Some(rule) = self.internet.as_ref().filter(|r| r.covers(student)) {
            let wanted = internet_fingerprint(&rule.value);
            let in_sync = match caps["internet_fp"].as_str() {
                Some(reported) => reported == wanted,
                None => caps["internet"].as_str().unwrap_or("open") == rule.value.mode.name(),
            };
            if !in_sync {
                fixes.push((
                    "policy",
                    json!({"policy": {"internet": rule.value, "lease_minutes": 1440}}),
                ));
            }
        }
        if let Some(rule) = self.copilot.as_ref().filter(|r| r.covers(student)) {
            if caps["copilot_network"] == 1 && (caps["copilot_blocked"] == 1) != rule.value {
                fixes.push(("copilot_policy", json!({"enabled": rule.value})));
            }
        }
        if let Some(rule) = self.vscode.as_ref().filter(|r| r.covers(student)) {
            if caps["vscode"] == 1 && caps["vscode_fp"].as_str().unwrap_or("") != rule.value.fingerprint() {
                fixes.push(("vscode_policy", json!({"policy": rule.value})));
            }
        }
        if let Some(rule) = self.apps.as_ref().filter(|r| r.covers(student)) {
            let wanted = app_hash(&rule.value);
            // A rule from before hashes existed reports an empty hash while
            // still blocking; when the class rule is "off" that flag matters.
            let in_sync = caps["apps_policy"].as_str().unwrap_or("") == wanted
                && (!wanted.is_empty() || caps["apps_blocked"] != 1);
            if caps["apps"] == 1 && !in_sync {
                fixes.push(("app_policy", json!({"policy": rule.value})));
            }
        }
        if let Some(rule) = self.clipboard.as_ref().filter(|r| r.covers(student)) {
            if caps["power"] == 1 && (caps["clipboard_blocked"] == 1) != rule.value {
                let op = if rule.value { "clipboard_block" } else { "clipboard_allow" };
                fixes.push(("power", json!({"op": op, "message": ""})));
            }
        }
        fixes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{app_control::MODE_BLOCK, guard::Mode};

    fn rules() -> ClassRules {
        ClassRules {
            day: crate::local_day::today(),
            internet: Some(Scoped::new(
                InternetPolicy { mode: Mode::BlockAll, rules: vec![] },
                None,
            )),
            copilot: Some(Scoped::new(true, Some(vec!["a".into()]))),
            vscode: Some(Scoped::new(
                VsCodePolicy { enabled: true, allowed: vec!["ms-python.python".into()] },
                None,
            )),
            apps: Some(Scoped::new(
                AppPolicy { enabled: true, mode: MODE_BLOCK.into(), ids: vec!["0123456789ab".into()] },
                None,
            )),
            clipboard: Some(Scoped::new(true, Some(vec!["a".into()]))),
            activity: true,
        }
    }

    #[test]
    fn a_fresh_student_gets_every_rule_that_covers_it() {
        let caps = json!({"guard":1,"copilot_network":1,"vscode":1,"apps":1,"power":1});
        let names: Vec<&str> = rules().fixes_for("a", &caps).iter().map(|f| f.0).collect();
        assert_eq!(names, ["policy", "copilot_policy", "vscode_policy", "app_policy", "power"]);
        // Scoped rules skip computers outside their group.
        let names: Vec<&str> = rules().fixes_for("b", &caps).iter().map(|f| f.0).collect();
        assert_eq!(names, ["policy", "vscode_policy", "app_policy"]);
    }

    #[test]
    fn a_student_already_in_step_gets_nothing() {
        let rules = rules();
        let caps = json!({
            "guard":1,"copilot_network":1,"copilot_blocked":1,"vscode":1,"apps":1,"power":1,"clipboard_blocked":1,
            "internet_fp": internet_fingerprint(&rules.internet.as_ref().unwrap().value),
            "vscode_fp": rules.vscode.as_ref().unwrap().value.fingerprint(),
            "apps_policy": app_hash(&rules.apps.as_ref().unwrap().value),
        });
        // Mode is block_all => fingerprint non-empty and matching.
        assert!(rules.fixes_for("a", &caps).is_empty());
    }

    #[test]
    fn computers_that_cannot_enforce_are_left_alone() {
        assert!(rules().fixes_for("a", &json!({"guard":0})).is_empty());
    }

    #[test]
    fn lifting_a_rule_also_reaches_a_computer_that_missed_it() {
        let mut rules = rules();
        rules.internet = Some(Scoped::new(InternetPolicy::default(), None));
        let caps = json!({"guard":1,"internet_fp":"abcd","internet":"block_all"});
        let fixes = rules.fixes_for("z", &caps);
        assert_eq!(fixes[0].0, "policy");
    }

    #[test]
    fn rules_from_another_day_are_dropped() {
        let store: classroom_store::Shared =
            std::sync::Arc::new(classroom_store::Store::open_in_memory().unwrap());
        let path = store.doc(classroom_store::ns::RULES, "today");
        let mut old = rules();
        old.day -= 1;
        old.save(&path).unwrap();
        assert!(ClassRules::load(&path).internet.is_none());
        let today = rules();
        today.save(&path).unwrap();
        assert!(ClassRules::load(&path).internet.is_some());
        let mut rolled = old;
        assert!(rolled.roll_over());
        assert!(rolled.internet.is_none() && !rolled.activity);
    }
}
