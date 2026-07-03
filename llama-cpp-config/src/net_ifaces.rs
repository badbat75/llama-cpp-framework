// Network interface enumeration for the "Bind to" combo in the Server tab.

use std::net::Ipv4Addr;

#[derive(Debug, Clone)]
pub struct BindOption {
    pub label: String,
    pub value: String,
}

/// The two fixed bind choices that always lead the dropdown, before any detected
/// interface: loopback-only, then all-interfaces. `build_options` inserts a stale
/// saved value right after these, so their count is the single source for that slot.
fn fixed_rows() -> Vec<BindOption> {
    vec![
        BindOption {
            label: "localhost (only this machine)".into(),
            value: "localhost".into(),
        },
        BindOption {
            label: "0.0.0.0 (all interfaces, LAN-reachable)".into(),
            value: "0.0.0.0".into(),
        },
    ]
}

/// Enumerate the machine's usable IPv4 interfaces (loopback / link-local
/// filtered out), sorted by name then address, each as an `ip (name — net/prefix)`
/// row. Empty when enumeration fails.
pub fn interfaces() -> Vec<BindOption> {
    let mut ifaces = match if_addrs::get_if_addrs() {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    ifaces.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then_with(|| a.ip().to_string().cmp(&b.ip().to_string()))
    });

    let mut out = Vec::new();
    for iface in ifaces {
        let if_addrs::IfAddr::V4(v4) = iface.addr else {
            continue;
        };
        if v4.ip.is_loopback() || v4.ip.is_link_local() {
            continue;
        }
        let prefix = netmask_to_prefix(v4.netmask);
        let network = network_of(v4.ip, v4.netmask);
        let label = format!(
            "{ip} ({name} — {net}/{prefix})",
            ip = v4.ip,
            name = iface.name,
            net = network,
            prefix = prefix,
        );
        out.push(BindOption {
            label,
            value: v4.ip.to_string(),
        });
    }

    out
}

/// Build the `(labels, values, selected_index)` triple for the "Bind to" combo,
/// mirroring `devices::build_options`: the two fixed rows lead, `ifaces` follow.
/// A `current` value matching none of them is preserved as a "(no longer present)"
/// row right after the fixed rows, so a stale saved hostname never silently
/// vanishes; an empty/among-list `current` selects it (defaulting to localhost).
pub fn build_options(ifaces: &[BindOption], current: &str) -> (Vec<String>, Vec<String>, i32) {
    let mut opts = fixed_rows();
    let fixed = opts.len();
    opts.extend_from_slice(ifaces);

    // Case-insensitive, like the sibling builders (devices matches ids
    // case-insensitively, model_scan uses paths_eq): a hand-edited
    // `Hostname = Localhost` must select the localhost row, not spawn a
    // spurious "(no longer present)" twin.
    let mut index = opts
        .iter()
        .position(|o| o.value.eq_ignore_ascii_case(current));
    if index.is_none() && !current.is_empty() {
        opts.insert(
            fixed,
            BindOption {
                label: format!("{current} (no longer present)"),
                value: current.to_string(),
            },
        );
        index = Some(fixed);
    }

    let labels = opts.iter().map(|o| o.label.clone()).collect();
    let values = opts.iter().map(|o| o.value.clone()).collect();
    (labels, values, index.unwrap_or(0) as i32)
}

fn netmask_to_prefix(mask: Ipv4Addr) -> u32 {
    mask.octets().iter().map(|b| b.count_ones()).sum()
}

fn network_of(ip: Ipv4Addr, mask: Ipv4Addr) -> Ipv4Addr {
    let ip_u = u32::from(ip);
    let mk_u = u32::from(mask);
    Ipv4Addr::from(ip_u & mk_u)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iface(value: &str) -> BindOption {
        BindOption {
            label: format!("{value} (eth0 — {value}/24)"),
            value: value.to_string(),
        }
    }

    #[test]
    fn build_options_defaults_to_localhost_when_empty() {
        let (_labels, values, idx) = build_options(&[], "");
        assert_eq!(idx, 0);
        assert_eq!(values[0], "localhost");
    }

    #[test]
    fn build_options_selects_a_fixed_row() {
        let (_labels, values, idx) = build_options(&[], "0.0.0.0");
        assert_eq!(values[idx as usize], "0.0.0.0");
    }

    #[test]
    fn build_options_selects_a_detected_interface() {
        let ifaces = [iface("192.168.1.10")];
        let (_labels, values, idx) = build_options(&ifaces, "192.168.1.10");
        assert_eq!(values[idx as usize], "192.168.1.10");
        assert_eq!(values.len(), 3); // 2 fixed rows + 1 interface
    }

    #[test]
    fn build_options_matches_current_case_insensitively() {
        let (labels, values, idx) = build_options(&[], "Localhost");
        assert_eq!(values[idx as usize], "localhost");
        assert!(!labels.iter().any(|l| l.contains("no longer present")));
    }

    #[test]
    fn build_options_preserves_stale_value_after_fixed_rows() {
        let (labels, values, idx) = build_options(&[], "10.0.0.5");
        assert_eq!(idx, 2); // right after the two fixed rows
        assert_eq!(values[2], "10.0.0.5");
        assert!(labels[2].contains("no longer present"));
    }
}
