// SPDX-License-Identifier: GPL-3.0-only

//! The panel applet's two popups in an ordinary window, for
//! `scripts/screenshots.sh`: a panel's own popups cannot be captured on
//! their own. Reads whatever settings and state `XDG_CONFIG_HOME` and
//! `XDG_STATE_HOME` point at, as the applet does.

fn main() -> cosmic::iced::Result {
    stellarshot::app::applet::preview()
}
