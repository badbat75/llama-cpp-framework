//! The configure phase's report: one row per path or tool, and the gaps that
//! make the phase fail.

use crate::common::ui;

/// The report rows, and the gaps that make the phase fail.
pub struct Rows {
    pub gaps: Vec<String>,
}

impl Rows {
    pub fn ok(&self, name: &str, v: impl AsRef<str>) {
        ui::ok(format!("  [OK] {name:<15}: {}", v.as_ref()));
    }
    pub fn opt(&self, name: &str, v: impl AsRef<str>) {
        ui::warn(format!("  [--] {name:<15}: {}", v.as_ref()));
    }
    pub fn miss(&mut self, name: &str, hint: &str) {
        ui::err(format!("  [!!] {name:<15}: NOT FOUND"));
        self.gaps.push(format!("{name}: {hint}"));
    }
    /// The summary under the rows.
    pub fn summary(&self) {
        ui::say("");
        if self.gaps.is_empty() {
            ui::ok("  All required dependencies found!");
        } else {
            ui::err(format!("  Gaps found ({}):", self.gaps.len()));
            for g in &self.gaps {
                ui::err(format!("    - {g}"));
            }
        }
        ui::say("");
    }
}
