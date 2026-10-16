//! The Tensor placement picker: one override rule edited as a set of the
//! model's own tensors, ticked in a list, instead of as a regex. Pure logic (no
//! IO, no Slint state); `gui::models_tab` holds the open `Picker` and pushes its
//! rows into `AppState`.
//!
//! ## The list
//! The model's tensors (read from its GGUF, every shard: `gguf::read_tensors`)
//! are grouped the way the names are built. The tensors outside the blocks
//! (`token_embd.weight`, `output.weight`, ...) form one group, and every block
//! tensor joins the FAMILY of its name with the block number taken out
//! (`blk.12.ffn_up_exps.weight` is a member of `ffn_up_exps.weight`), one member
//! per layer. A group header ticks or clears all its members and folds them
//! away; nearly 1500 tensors on a hybrid MoE are only usable that way.
//!
//! ## What a tick writes
//! The rule stays a regex in `presets.ini`, so llama.cpp reads exactly what it
//! read before. `compile` writes the shortest one this module knows for the
//! ticked set, in order of preference:
//!   1. a `tensor_override::KINDS` pattern, over every layer or over some
//!      (`blk\.(10|11)\.ffn_up\.`), when it matches exactly the ticked set, so
//!      the row then reads as that family in the table's dropdown;
//!   2. otherwise the families with the same layers merged into one
//!      alternative, `^(blk\.(10|11)\.(ffn_up\.weight|ffn_down\.weight)|output\.weight)$`,
//!      `\d+` standing for every layer a family has.
//!
//! Whatever it writes is checked against the tensor list before it is returned:
//! a pattern that would match one tensor more or less than the ticked set falls
//! back to listing every name.
//!
//! ## Opening on an existing rule
//! The ticks start from what the rule matches today, with the `regex` crate.
//! Its syntax and llama.cpp's `std::regex` (ECMAScript) agree on everything this
//! table writes and on the common hand-written forms; a regex the crate cannot
//! read opens with nothing ticked and says so. Tensors an EARLIER rule already
//! matches are marked: llama.cpp applies the first rule that matches a tensor,
//! so ticking one of them here changes nothing for it.

use crate::gguf::TensorInfo;
use crate::tensor_override::{self, KINDS};
use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};

/// One line of the picker's list, as `gui` turns it into a Slint row.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Line {
    pub label: String,
    /// Size, member count, and who already takes the tensor.
    pub detail: String,
    pub checked: bool,
    /// A group with some but not all members ticked.
    pub partial: bool,
    pub header: bool,
    pub expanded: bool,
    /// Matched by an earlier rule, which wins.
    pub taken: bool,
}

/// What one line of the list stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Entry {
    Group(usize),
    Tensor(usize),
}

struct Group {
    label: String,
    /// Indices into `tensors`, in display order (by layer for a family).
    members: Vec<usize>,
}

/// The open picker: the model's tensors, what is ticked, what is folded.
pub struct Picker {
    /// The rule being edited (its row in the table).
    pub rule: usize,
    tensors: Vec<TensorInfo>,
    /// Per tensor: the 1-based number of the earlier rule that takes it, or 0.
    taken_by: Vec<usize>,
    selected: Vec<bool>,
    groups: Vec<Group>,
    expanded: Vec<bool>,
    /// The current list, line by line; rebuilt by `lines`.
    entries: Vec<Entry>,
    /// The rule's regex did not compile here: shown in the summary.
    unreadable: bool,
}

/// `blk.<n>.<rest>` → `(n, rest)`.
fn block_parts(name: &str) -> Option<(u32, &str)> {
    let rest = name.strip_prefix("blk.")?;
    let (n, tail) = rest.split_once('.')?;
    Some((n.parse().ok()?, tail))
}

/// The name as a regex literal. Only what the regex syntax gives a meaning to
/// is escaped (GGUF names are `[a-z0-9_.]` in practice): an escape both
/// engines read alike.
fn escape(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for c in name.chars() {
        if r"\.^$|?*+()[]{}".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// `1.21 GiB`, `312.0 MiB`, `16 KiB`.
pub fn fmt_bytes(b: u64) -> String {
    const KIB: f64 = 1024.0;
    let f = b as f64;
    if f >= KIB * KIB * KIB {
        format!("{:.2} GiB", f / (KIB * KIB * KIB))
    } else if f >= KIB * KIB {
        format!("{:.1} MiB", f / (KIB * KIB))
    } else {
        format!("{:.0} KiB", (f / KIB).ceil())
    }
}

/// The tensors `pattern` matches, as llama.cpp's `regex_search` would (found
/// anywhere in the name). `None` when the regex does not compile here.
pub fn matches(tensors: &[TensorInfo], pattern: &str) -> Option<Vec<bool>> {
    let re = Regex::new(pattern).ok()?;
    Some(tensors.iter().map(|t| re.is_match(&t.name)).collect())
}

/// Per rule of `s`, the line the table shows under it: how many tensors it
/// places and how much they weigh, counting only the ones no EARLIER rule took
/// (llama.cpp applies the first match). Empty strings when the model's tensors
/// are unknown.
pub fn coverage(s: &str, tensors: &[TensorInfo]) -> Vec<String> {
    let rules = tensor_override::parse(s);
    if tensors.is_empty() {
        return vec![String::new(); rules.len()];
    }
    let mut taken = vec![false; tensors.len()];
    rules
        .iter()
        .map(|rule| {
            if rule.pattern.is_empty() {
                return String::new();
            }
            let Some(hit) = matches(tensors, &rule.pattern) else {
                return "this regex cannot be read here, so its tensors are not shown".into();
            };
            let (mut n, mut bytes, mut shadowed) = (0usize, 0u64, 0usize);
            for (i, h) in hit.iter().enumerate() {
                if !h {
                    continue;
                }
                if taken[i] {
                    shadowed += 1;
                } else {
                    taken[i] = true;
                    n += 1;
                    bytes += tensors[i].bytes;
                }
            }
            let mut line = match n {
                0 if shadowed == 0 => "matches no tensor of this model".to_string(),
                0 => "every tensor it matches is taken by an earlier rule".to_string(),
                1 => format!("1 tensor, {}", fmt_bytes(bytes)),
                _ => format!("{n} tensors, {}", fmt_bytes(bytes)),
            };
            if n > 0 && shadowed > 0 {
                line.push_str(&format!("; {shadowed} more taken by an earlier rule"));
            }
            line
        })
        .collect()
}

/// The groups of `tensors`: outside the blocks first, then one per family in the
/// order the file first names it, members by layer.
fn group(tensors: &[TensorInfo]) -> Vec<Group> {
    let mut outside = Vec::new();
    let mut order: Vec<String> = Vec::new();
    let mut families: BTreeMap<String, Vec<(u32, usize)>> = BTreeMap::new();
    for (i, t) in tensors.iter().enumerate() {
        match block_parts(&t.name) {
            Some((n, rest)) => {
                if !families.contains_key(rest) {
                    order.push(rest.to_string());
                }
                families.entry(rest.to_string()).or_default().push((n, i));
            }
            None => outside.push(i),
        }
    }
    let mut out = Vec::new();
    if !outside.is_empty() {
        out.push(Group {
            label: "Outside the blocks".into(),
            members: outside,
        });
    }
    for fam in order {
        let mut m = families.remove(&fam).unwrap_or_default();
        m.sort();
        out.push(Group {
            label: format!("blk.N.{fam}"),
            members: m.into_iter().map(|(_, i)| i).collect(),
        });
    }
    out
}

impl Picker {
    /// Open the picker on rule `rule` of `s`: ticked = what that rule matches.
    pub fn open(s: &str, rule: usize, tensors: Vec<TensorInfo>) -> Self {
        let rules = tensor_override::parse(s);
        let mut taken_by = vec![0usize; tensors.len()];
        for (r, earlier) in rules.iter().enumerate().take(rule) {
            if let Some(hit) = matches(&tensors, &earlier.pattern) {
                for (i, h) in hit.into_iter().enumerate() {
                    if h && taken_by[i] == 0 {
                        taken_by[i] = r + 1;
                    }
                }
            }
        }
        let pattern = rules.get(rule).map(|r| r.pattern.as_str()).unwrap_or("");
        let (selected, unreadable) = if pattern.is_empty() {
            (vec![false; tensors.len()], false)
        } else {
            match matches(&tensors, pattern) {
                Some(hit) => (hit, false),
                None => (vec![false; tensors.len()], true),
            }
        };
        let groups = group(&tensors);
        // The outside group opens unfolded (a handful of lines), and so does any
        // family only partly ticked: that is the state worth seeing.
        let expanded = groups
            .iter()
            .enumerate()
            .map(|(g, grp)| {
                let n = grp.members.iter().filter(|&&i| selected[i]).count();
                (g == 0
                    && grp
                        .members
                        .iter()
                        .all(|&i| block_parts(&tensors[i].name).is_none()))
                    || (n > 0 && n < grp.members.len())
            })
            .collect();
        Picker {
            rule,
            tensors,
            taken_by,
            selected,
            groups,
            expanded,
            entries: Vec::new(),
            unreadable,
        }
    }

    /// The list as it should show now. Also records which entry each line is,
    /// which `toggle` / `expand` address by line.
    pub fn lines(&mut self) -> Vec<Line> {
        self.entries.clear();
        let mut out = Vec::new();
        for (g, grp) in self.groups.iter().enumerate() {
            let n = grp.members.iter().filter(|&&i| self.selected[i]).count();
            let bytes: u64 = grp.members.iter().map(|&i| self.tensors[i].bytes).sum();
            let taken = grp
                .members
                .iter()
                .filter(|&&i| self.taken_by[i] > 0)
                .count();
            let mut detail = format!("{n}/{}, {}", grp.members.len(), fmt_bytes(bytes));
            if taken > 0 {
                detail.push_str(&format!(", {taken} taken by an earlier rule"));
            }
            self.entries.push(Entry::Group(g));
            out.push(Line {
                label: grp.label.clone(),
                detail,
                checked: n == grp.members.len(),
                partial: n > 0 && n < grp.members.len(),
                header: true,
                expanded: self.expanded[g],
                taken: taken == grp.members.len(),
            });
            if !self.expanded[g] {
                continue;
            }
            for &i in &grp.members {
                let t = &self.tensors[i];
                let mut detail = fmt_bytes(t.bytes);
                if self.taken_by[i] > 0 {
                    detail.push_str(&format!(", taken by rule {}", self.taken_by[i]));
                }
                self.entries.push(Entry::Tensor(i));
                out.push(Line {
                    label: t.name.clone(),
                    detail,
                    checked: self.selected[i],
                    partial: false,
                    header: false,
                    expanded: false,
                    taken: self.taken_by[i] > 0,
                });
            }
        }
        out
    }

    /// Tick or clear the tensor or group on `line`: a group ticks all its
    /// members unless all are already ticked, which clears them.
    pub fn toggle(&mut self, line: usize) {
        match self.entries.get(line) {
            Some(&Entry::Tensor(i)) => self.selected[i] = !self.selected[i],
            Some(&Entry::Group(g)) => {
                let members = &self.groups[g].members;
                let all = members.iter().all(|&i| self.selected[i]);
                for &i in members {
                    self.selected[i] = !all;
                }
            }
            None => {}
        }
    }

    /// Fold or unfold the group on `line` (a tensor line does nothing).
    pub fn expand(&mut self, line: usize) {
        if let Some(&Entry::Group(g)) = self.entries.get(line) {
            self.expanded[g] = !self.expanded[g];
        }
    }

    /// Tick or clear every tensor of the model.
    pub fn set_all(&mut self, on: bool) {
        self.selected.iter_mut().for_each(|s| *s = on);
    }

    /// Fold or unfold every group.
    pub fn expand_all(&mut self, on: bool) {
        self.expanded.iter_mut().for_each(|e| *e = on);
    }

    pub fn any_selected(&self) -> bool {
        self.selected.iter().any(|&s| s)
    }

    /// The pattern the ticked set writes (empty when nothing is ticked).
    pub fn pattern(&self) -> String {
        compile(&self.tensors, &self.selected)
    }

    /// The line under the list: what is ticked, what it weighs, what will win.
    pub fn summary(&self) -> String {
        let n = self.selected.iter().filter(|&&s| s).count();
        let bytes: u64 = self
            .selected
            .iter()
            .zip(&self.tensors)
            .filter(|(s, _)| **s)
            .map(|(_, t)| t.bytes)
            .sum();
        let shadowed = self
            .selected
            .iter()
            .zip(&self.taken_by)
            .filter(|(s, t)| **s && **t > 0)
            .count();
        let mut line = if n == 0 {
            "nothing ticked".to_string()
        } else {
            format!(
                "{n} of {} tensors ticked, {}",
                self.tensors.len(),
                fmt_bytes(bytes)
            )
        };
        if shadowed > 0 {
            line.push_str(&format!(
                "; {shadowed} of them stay where an earlier rule puts them"
            ));
        }
        if self.unreadable {
            line.push_str("; the rule's regex could not be read here, so the ticks start empty");
        }
        line
    }
}

/// Whether `pattern` matches exactly the ticked tensors.
fn exact(tensors: &[TensorInfo], selected: &[bool], pattern: &str) -> bool {
    matches(tensors, pattern).is_some_and(|hit| hit == selected)
}

/// The regex for the ticked set; see the module header for the forms tried.
pub fn compile(tensors: &[TensorInfo], selected: &[bool]) -> String {
    if !selected.iter().any(|&s| s) {
        return String::new();
    }

    // 1. A named family, over every layer or over the ticked layers.
    let layers: BTreeSet<u32> = tensors
        .iter()
        .zip(selected)
        .filter(|(_, s)| **s)
        .filter_map(|(t, _)| block_parts(&t.name).map(|(n, _)| n))
        .collect();
    let layers: Vec<u32> = layers.into_iter().collect();
    for kind in KINDS.iter().filter(|k| !k.pattern.is_empty()) {
        if exact(tensors, selected, kind.pattern) {
            return kind.pattern.to_string();
        }
        if !kind.block.is_empty() && !layers.is_empty() {
            let p = tensor_override::compose(kind, Some(layers.as_slice()));
            if exact(tensors, selected, &p) {
                return p;
            }
        }
    }

    // 2. Families merged by layer set.
    let mut outside: Vec<String> = Vec::new();
    // family → (ticked layers, every layer it has)
    let mut fams: BTreeMap<&str, (BTreeSet<u32>, BTreeSet<u32>)> = BTreeMap::new();
    for (t, &s) in tensors.iter().zip(selected) {
        match block_parts(&t.name) {
            Some((n, rest)) => {
                let e = fams.entry(rest).or_default();
                e.1.insert(n);
                if s {
                    e.0.insert(n);
                }
            }
            None if s => outside.push(escape(&t.name)),
            None => {}
        }
    }
    // layer spec → families with exactly those ticked layers
    let mut by_spec: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (fam, (ticked, all)) in &fams {
        if ticked.is_empty() {
            continue;
        }
        let spec = if ticked == all {
            r"\d+".to_string()
        } else if ticked.len() == 1 {
            ticked.iter().next().unwrap().to_string()
        } else {
            let nums: Vec<String> = ticked.iter().map(u32::to_string).collect();
            format!("({})", nums.join("|"))
        };
        by_spec.entry(spec).or_default().push(escape(fam));
    }
    let mut alts = outside;
    for (spec, fams) in by_spec {
        let tail = if fams.len() == 1 {
            fams[0].clone()
        } else {
            format!("({})", fams.join("|"))
        };
        alts.push(format!(r"blk\.{spec}\.{tail}"));
    }
    let p = if alts.len() == 1 {
        format!("^{}$", alts[0])
    } else {
        format!("^({})$", alts.join("|"))
    };
    if exact(tensors, selected, &p) {
        return p;
    }

    // 3. Never reached with GGUF names, kept so a surprise cannot write a rule
    //    that places other tensors than the ticked ones.
    let names: Vec<String> = tensors
        .iter()
        .zip(selected)
        .filter(|(_, s)| **s)
        .map(|(t, _)| escape(&t.name))
        .collect();
    format!("^({})$", names.join("|"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(name: &str, bytes: u64) -> TensorInfo {
        TensorInfo {
            name: name.into(),
            bytes,
            ty: 0,
            ne1: 0,
        }
    }

    /// A small hybrid MoE in the shape of the real ones: 4 blocks, attention
    /// and SSM alternating, experts and shared experts everywhere, plus the
    /// three tensors outside the blocks (and Gemma 3n's per-layer embedding,
    /// whose name contains `token_embd`).
    fn model() -> Vec<TensorInfo> {
        let mut v = vec![
            t("token_embd.weight", 1 << 30),
            t("per_layer_token_embd.weight", 1 << 28),
        ];
        for b in 0..4 {
            let mix = if b % 2 == 0 { "attn_qkv" } else { "ssm_out" };
            v.push(t(&format!("blk.{b}.attn_norm.weight"), 4096));
            v.push(t(&format!("blk.{b}.{mix}.weight"), 1 << 20));
            v.push(t(&format!("blk.{b}.attn_output.weight"), 1 << 20));
            for p in ["up", "gate", "down"] {
                v.push(t(&format!("blk.{b}.ffn_{p}_exps.weight"), 1 << 24));
                v.push(t(&format!("blk.{b}.ffn_{p}_shexp.weight"), 1 << 18));
            }
            v.push(t(&format!("blk.{b}.ffn_gate_inp.weight"), 1 << 16));
        }
        v.push(t("output_norm.weight", 4096));
        v.push(t("output.weight", 1 << 29));
        v
    }

    fn select(tensors: &[TensorInfo], names: &[&str]) -> Vec<bool> {
        tensors
            .iter()
            .map(|t| names.contains(&t.name.as_str()))
            .collect()
    }

    fn names(tensors: &[TensorInfo], pattern: &str) -> Vec<String> {
        let hit = matches(tensors, pattern).expect("compiles");
        tensors
            .iter()
            .zip(hit)
            .filter(|(_, h)| *h)
            .map(|(t, _)| t.name.clone())
            .collect()
    }

    // Whatever is ticked, the pattern written matches exactly that: the one
    // property the picker exists for. Checked over many shapes of selection.
    #[test]
    fn the_written_pattern_matches_exactly_the_ticked_set() {
        let m = model();
        let picks: &[&[&str]] = &[
            &["output.weight"],
            &["token_embd.weight"],
            &["blk.2.ffn_up_exps.weight"],
            &["blk.0.ffn_up_exps.weight", "blk.3.ffn_down_exps.weight"],
            &[
                "blk.1.ssm_out.weight",
                "blk.3.ssm_out.weight",
                "output.weight",
            ],
            &["blk.0.attn_norm.weight", "per_layer_token_embd.weight"],
        ];
        for pick in picks {
            let sel = select(&m, pick);
            let p = compile(&m, &sel);
            assert!(!p.contains([',', '=']), "{p}");
            let mut got = names(&m, &p);
            let mut want: Vec<String> = pick.iter().map(|s| s.to_string()).collect();
            got.sort();
            want.sort();
            assert_eq!(got, want, "{p}");
        }
    }

    // A set a family describes is written as that family, so the table's row
    // reads it back as the dropdown entry rather than as a Custom regex.
    #[test]
    fn a_family_selection_is_written_as_the_family() {
        let m = model();
        let exps: Vec<bool> =
            matches(&m, KINDS.iter().find(|k| k.id == "exps").unwrap().pattern).unwrap();
        assert_eq!(
            compile(&m, &exps),
            r"\.ffn_(up|down|gate|gate_up)_(ch|)exps"
        );

        let sel = select(
            &m,
            &["blk.1.ffn_down_exps.weight", "blk.2.ffn_down_exps.weight"],
        );
        assert_eq!(compile(&m, &sel), r"blk\.(1|2)\.ffn_down_(ch|)exps");

        let out = select(&m, &["output.weight"]);
        assert_eq!(compile(&m, &out), r"^output\.weight");
    }

    // Families with the same ticked layers share one alternative, and a family
    // ticked on every layer it has is written with `\d+`.
    #[test]
    fn families_merge_by_layer_set() {
        let m = model();
        let sel = select(
            &m,
            &[
                "blk.0.attn_qkv.weight",
                "blk.2.attn_qkv.weight",
                "blk.1.ssm_out.weight",
                "blk.3.ssm_out.weight",
                "blk.2.ffn_up_shexp.weight",
            ],
        );
        let p = compile(&m, &sel);
        assert_eq!(
            p,
            r"^(blk\.2\.ffn_up_shexp\.weight|blk\.\d+\.(attn_qkv\.weight|ssm_out\.weight))$"
        );
    }

    #[test]
    fn nothing_ticked_writes_nothing() {
        let m = model();
        assert_eq!(compile(&m, &vec![false; m.len()]), "");
    }

    // Open on a rule: its matches start ticked, an earlier rule's matches are
    // marked taken, and the groups fold the way the module header says.
    #[test]
    fn opening_ticks_what_the_rule_matches_and_marks_earlier_rules() {
        let m = model();
        let s = r"^output\.weight=CUDA0,blk\.1\.ffn_(up|down)_exps=ROCm0";
        let mut p = Picker::open(s, 1, m.clone());
        assert_eq!(
            names(&m, &p.pattern()),
            ["blk.1.ffn_up_exps.weight", "blk.1.ffn_down_exps.weight"]
        );
        let lines = p.lines();
        let outside = &lines[0];
        assert!(outside.header && outside.expanded, "outside opens unfolded");
        let output = lines.iter().find(|l| l.label == "output.weight").unwrap();
        assert!(output.taken && output.detail.contains("rule 1"));
        let up = lines
            .iter()
            .find(|l| l.label == "blk.N.ffn_up_exps.weight")
            .unwrap();
        assert!(up.partial && up.expanded, "a partly ticked family opens");
        let gate = lines
            .iter()
            .find(|l| l.label == "blk.N.ffn_gate_exps.weight")
            .unwrap();
        assert!(!gate.expanded && !gate.checked && !gate.partial);
    }

    #[test]
    fn a_group_tick_takes_every_member_and_a_second_clears_them() {
        let m = model();
        let mut p = Picker::open("", 0, m.clone());
        p.lines();
        let g = p
            .entries
            .iter()
            .position(
                |e| matches!(e, Entry::Group(g) if p.groups[*g].label == "blk.N.ssm_out.weight"),
            )
            .unwrap();
        p.toggle(g);
        assert_eq!(p.pattern(), r"\.ssm_");
        p.lines();
        p.toggle(g);
        assert!(!p.any_selected());
    }

    #[test]
    fn coverage_counts_first_match_wins() {
        let m = model();
        let s = r"\.ffn_up_(ch|)exps=CPU,\.ffn_(up|down|gate|gate_up)_(ch|)exps=ROCm0,nothing=CPU,[=CPU";
        let c = coverage(s, &m);
        assert_eq!(c[0], "4 tensors, 64.0 MiB");
        assert_eq!(
            c[1],
            "8 tensors, 128.0 MiB; 4 more taken by an earlier rule"
        );
        assert_eq!(c[2], "matches no tensor of this model");
        assert!(c[3].contains("cannot be read"));
        assert_eq!(coverage(s, &[]), vec![String::new(); 4]);
    }

    // The unanchored `output\.weight` a hand-written rule often carries takes
    // every attn_output too: the coverage line is where that shows.
    #[test]
    fn an_unanchored_output_rule_shows_what_it_really_takes() {
        let m = model();
        assert_eq!(names(&m, r"output\.weight").len(), 5);
        assert_eq!(names(&m, r"^output\.weight"), ["output.weight"]);
    }
}
