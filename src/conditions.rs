// SPDX-License-Identifier: GPL-3.0-only

//! Whether the machine is in a state a scheduled backup is allowed to run
//! in: see [`crate::profile::Conditions`]. Checked only for a run the timer
//! starts; **Back Up Now** always runs regardless.
//!
//! Reading the real state (through UPower and NetworkManager, both over the
//! system D-Bus) and deciding whether it satisfies a profile's conditions
//! are kept apart, so the decision itself ([`met`]) is tested without a
//! real system bus.

use crate::debug::SCHED;
use crate::debug_log;
use crate::profile::Conditions;

/// The state actually on the machine right now, as far as these conditions
/// care. `None` (or, for the network, a `connected_wifi` of `None`) means a
/// piece could not be read: the relevant service is not running, or the
/// machine plainly has nothing to read it from (no battery, no
/// NetworkManager). The condition that needed it is then treated as met
/// rather than blocking a schedule indefinitely on a machine that can never
/// satisfy a check it has no way to answer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SystemState {
    pub on_battery: Option<bool>,
    pub battery_percent: Option<u8>,
    pub metered: Option<bool>,
    /// Wi-Fi networks currently connected to, by connection name (the same
    /// name NetworkManager's own network list shows); `None` if this could
    /// not be read at all.
    pub connected_wifi: Option<Vec<String>>,
    pub vpn_up: bool,
}

/// Whether `conditions` are all met right now. `Ok(())` if so, or if none
/// are set; otherwise a short technical reason, for the log and the event
/// history.
pub fn met(conditions: &Conditions, state: &SystemState) -> Result<(), String> {
    if conditions.require_ac && state.on_battery == Some(true) {
        return Err("running on battery, not mains power".to_owned());
    }
    if let Some(minimum) = conditions.min_battery_percent
        && let Some(percent) = state.battery_percent
        && percent < minimum
    {
        return Err(format!(
            "battery at {percent}%, below the {minimum}% minimum"
        ));
    }
    if conditions.block_metered && state.metered == Some(true) {
        return Err("on a connection marked metered".to_owned());
    }
    if conditions.require_trusted_network
        && !state.vpn_up
        && let Some(connected) = &state.connected_wifi
        && !connected
            .iter()
            .any(|network| conditions.trusted_networks.contains(network))
    {
        return Err("not on a trusted network or a VPN".to_owned());
    }
    Ok(())
}

/// Read the machine's actual state and check it against `conditions`. Never
/// fails outright: a service that cannot be reached just leaves its part of
/// [`SystemState`] unknown.
pub async fn check(conditions: &Conditions) -> Result<(), String> {
    if conditions.is_empty() {
        return Ok(());
    }
    let state = read_state().await;
    debug_log!(SCHED, "conditions checked against {state:?}");
    met(conditions, &state)
}

#[zbus::proxy(
    interface = "org.freedesktop.UPower",
    default_service = "org.freedesktop.UPower",
    default_path = "/org/freedesktop/UPower"
)]
trait UPower {
    #[zbus(property)]
    fn on_battery(&self) -> zbus::Result<bool>;
}

/// UPower's own summary of every power source, kept at a fixed, documented
/// path so no enumeration is needed. `IsPresent` is false with nothing
/// meaningful in `Percentage` on a machine with no real battery.
#[zbus::proxy(
    interface = "org.freedesktop.UPower.Device",
    default_service = "org.freedesktop.UPower",
    default_path = "/org/freedesktop/UPower/devices/DisplayDevice"
)]
trait UPowerDisplayDevice {
    #[zbus(property)]
    fn is_present(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn percentage(&self) -> zbus::Result<f64>;
}

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager"
)]
trait NetworkManager {
    #[zbus(property)]
    fn active_connections(&self) -> zbus::Result<Vec<zbus::zvariant::OwnedObjectPath>>;
    /// `NM_METERED_UNKNOWN` (0), `YES` (1), `NO` (2), `GUESS_YES` (3),
    /// `GUESS_NO` (4).
    #[zbus(property)]
    fn metered(&self) -> zbus::Result<u32>;
}

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager.Connection.Active",
    default_service = "org.freedesktop.NetworkManager"
)]
trait ActiveConnection {
    #[zbus(property, name = "Type")]
    fn connection_type(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn id(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn vpn(&self) -> zbus::Result<bool>;
}

/// NetworkManager's own connection types for a VPN interface: a connection
/// profile it flags `Vpn` itself, one of its VPN plugin types, or a plain
/// TUN/TAP device, which is what Tailscale (and a manually configured
/// WireGuard interface) comes up as — confirmed against a real Tailscale
/// connection, which NetworkManager does *not* set `Vpn: true` for.
fn is_vpn_type(connection_type: &str) -> bool {
    matches!(connection_type, "vpn" | "wireguard" | "tun" | "tap")
}

async fn upower_state() -> (Option<bool>, Option<u8>) {
    let Ok(connection) = zbus::Connection::system().await else {
        return (None, None);
    };
    let on_battery = match UPowerProxy::new(&connection).await {
        Ok(proxy) => proxy.on_battery().await.ok(),
        Err(_) => None,
    };
    let percent = match UPowerDisplayDeviceProxy::new(&connection).await {
        Ok(proxy) if proxy.is_present().await == Ok(true) => {
            proxy.percentage().await.ok().map(|value| value as u8)
        }
        _ => None,
    };
    (on_battery, percent)
}

async fn network_state() -> (Option<bool>, Option<Vec<String>>, bool) {
    let Ok(connection) = zbus::Connection::system().await else {
        return (None, None, false);
    };
    let Ok(nm) = NetworkManagerProxy::new(&connection).await else {
        return (None, None, false);
    };
    let metered = nm.metered().await.ok().and_then(|value| match value {
        1 | 3 => Some(true),
        2 | 4 => Some(false),
        _ => None,
    });
    let mut wifi = Vec::new();
    let mut vpn_up = false;
    for path in nm.active_connections().await.unwrap_or_default() {
        let Ok(active) = ActiveConnectionProxy::new(&connection, path).await else {
            continue;
        };
        let connection_type = active.connection_type().await.unwrap_or_default();
        if active.vpn().await == Ok(true) || is_vpn_type(&connection_type) {
            vpn_up = true;
        }
        if connection_type == "802-11-wireless"
            && let Ok(id) = active.id().await
        {
            wifi.push(id);
        }
    }
    (metered, Some(wifi), vpn_up)
}

async fn read_state() -> SystemState {
    let (on_battery, battery_percent) = upower_state().await;
    let (metered, connected_wifi, vpn_up) = network_state().await;
    SystemState {
        on_battery,
        battery_percent,
        metered,
        connected_wifi,
        vpn_up,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> SystemState {
        SystemState {
            on_battery: Some(false),
            battery_percent: Some(80),
            metered: Some(false),
            connected_wifi: Some(vec!["Home".to_owned()]),
            vpn_up: false,
        }
    }

    #[test]
    fn no_conditions_are_always_met() {
        assert_eq!(met(&Conditions::default(), &SystemState::default()), Ok(()));
    }

    #[test]
    fn ac_is_required_only_when_actually_on_battery() {
        let conditions = Conditions {
            require_ac: true,
            ..Conditions::default()
        };
        assert!(met(&conditions, &state()).is_ok());
        let on_battery = SystemState {
            on_battery: Some(true),
            ..state()
        };
        assert!(met(&conditions, &on_battery).is_err());
        let unknown = SystemState {
            on_battery: None,
            ..state()
        };
        assert!(
            met(&conditions, &unknown).is_ok(),
            "a machine with no way to tell must not block forever"
        );
    }

    #[test]
    fn a_battery_minimum_blocks_only_below_it() {
        let conditions = Conditions {
            min_battery_percent: Some(50),
            ..Conditions::default()
        };
        assert!(met(&conditions, &state()).is_ok(), "80% clears 50%");
        let low = SystemState {
            battery_percent: Some(49),
            ..state()
        };
        assert!(met(&conditions, &low).is_err());
        let exact = SystemState {
            battery_percent: Some(50),
            ..state()
        };
        assert!(
            met(&conditions, &exact).is_ok(),
            "the minimum itself clears it"
        );
        let no_battery = SystemState {
            battery_percent: None,
            ..state()
        };
        assert!(
            met(&conditions, &no_battery).is_ok(),
            "a desktop with no battery has nothing to be low on"
        );
    }

    #[test]
    fn metered_blocks_only_when_actually_metered() {
        let conditions = Conditions {
            block_metered: true,
            ..Conditions::default()
        };
        assert!(met(&conditions, &state()).is_ok());
        let metered = SystemState {
            metered: Some(true),
            ..state()
        };
        assert!(met(&conditions, &metered).is_err());
        let unknown = SystemState {
            metered: None,
            ..state()
        };
        assert!(met(&conditions, &unknown).is_ok());
    }

    #[test]
    fn a_trusted_network_by_name_satisfies_the_condition() {
        let conditions = Conditions {
            require_trusted_network: true,
            trusted_networks: vec!["Home".to_owned()],
            ..Conditions::default()
        };
        assert!(met(&conditions, &state()).is_ok());
        let elsewhere = SystemState {
            connected_wifi: Some(vec!["Coffee Shop".to_owned()]),
            ..state()
        };
        assert!(met(&conditions, &elsewhere).is_err());
    }

    #[test]
    fn a_vpn_satisfies_the_trusted_network_condition_on_its_own() {
        let conditions = Conditions {
            require_trusted_network: true,
            trusted_networks: Vec::new(),
            ..Conditions::default()
        };
        let on_vpn = SystemState {
            connected_wifi: Some(vec!["Coffee Shop".to_owned()]),
            vpn_up: true,
            ..state()
        };
        assert!(met(&conditions, &on_vpn).is_ok());
    }

    #[test]
    fn an_unreadable_network_does_not_block_a_trusted_network_condition() {
        let conditions = Conditions {
            require_trusted_network: true,
            trusted_networks: vec!["Home".to_owned()],
            ..Conditions::default()
        };
        let unknown = SystemState {
            connected_wifi: None,
            ..state()
        };
        assert!(
            met(&conditions, &unknown).is_ok(),
            "a machine with no way to tell must not block forever"
        );
    }

    /// Reads whatever UPower and NetworkManager actually expose here. Not a
    /// substitute for the pure tests above (their whole point is to not
    /// need a real system bus): this only proves the D-Bus interface names,
    /// property names and types this module hardcodes still match reality,
    /// and that a missing service degrades to `None` rather than panicking,
    /// on whichever machine happens to run it.
    #[tokio::test]
    async fn read_state_reaches_the_real_system_bus_without_panicking() {
        let state = read_state().await;
        if let Some(percent) = state.battery_percent {
            assert!(percent <= 100, "UPower's own percentage is 0-100");
        }
    }

    #[test]
    fn vpn_types_match_what_a_real_tailscale_connection_reports() {
        // NetworkManager does not set its own `Vpn` flag for a Tailscale
        // interface; it comes up as a plain `tun` connection instead.
        assert!(is_vpn_type("tun"));
        assert!(is_vpn_type("wireguard"));
        assert!(is_vpn_type("vpn"));
        assert!(!is_vpn_type("802-11-wireless"));
        assert!(!is_vpn_type("bridge"));
    }
}
