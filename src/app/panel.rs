// SPDX-License-Identifier: GPL-3.0-only

//! Whether Stellarshot's applet is on a COSMIC panel or dock: what closing
//! the window minimizes to. Without it, nothing on screen would show the app
//! still running.

use cosmic::cosmic_config::{Config, ConfigGet};

use crate::constants::APPLET_ID;

/// The configuration that lists COSMIC's panels and docks by name.
const PANELS: &str = "com.system76.CosmicPanel";

/// Whether any COSMIC panel or dock lists the applet. Reads COSMIC's own
/// configuration (with its system defaults), a few small files. Anything
/// unreadable, or not running COSMIC at all, counts as "not on a panel":
/// keeping an invisible process alive is the mistake to avoid.
pub fn applet_on_a_panel() -> bool {
    let Ok(panels) = Config::new(PANELS, 1) else {
        return false;
    };
    let names: Vec<String> = panels.get("entries").unwrap_or_default();
    names.iter().any(|name| {
        let Ok(panel) = Config::new(&format!("{PANELS}.{name}"), 1) else {
            return false;
        };
        lists_applet(
            panel.get("plugins_center").ok().flatten(),
            panel.get("plugins_wings").ok().flatten(),
        )
    })
}

/// Whether a panel whose applets are `center` and `wings` (left and right)
/// includes Stellarshot's.
fn lists_applet(center: Option<Vec<String>>, wings: Option<(Vec<String>, Vec<String>)>) -> bool {
    let (left, right) = wings.unwrap_or_default();
    center
        .unwrap_or_default()
        .iter()
        .chain(&left)
        .chain(&right)
        .any(|applet| applet == APPLET_ID)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_applet_is_found_in_the_center_or_either_wing() {
        let ours = || vec![APPLET_ID.to_owned()];
        let others = || vec!["com.system76.CosmicAppletTime".to_owned()];
        assert!(lists_applet(Some(ours()), None));
        assert!(lists_applet(None, Some((ours(), others()))));
        assert!(lists_applet(Some(others()), Some((others(), ours()))));
        assert!(!lists_applet(Some(others()), Some((others(), others()))));
        assert!(!lists_applet(None, None));
    }
}
