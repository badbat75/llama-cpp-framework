// Network interface enumeration for the "Bind to" combo in the Server tab.

use std::net::Ipv4Addr;

#[derive(Debug, Clone)]
pub struct BindOption {
    pub label: String,
    pub value: String,
}

pub fn list_options() -> Vec<BindOption> {
    let mut out = vec![
        BindOption {
            label: "localhost (only this machine)".into(),
            value: "localhost".into(),
        },
        BindOption {
            label: "0.0.0.0 (all interfaces, LAN-reachable)".into(),
            value: "0.0.0.0".into(),
        },
    ];

    let mut ifaces = match if_addrs::get_if_addrs() {
        Ok(v) => v,
        Err(_) => return out,
    };

    ifaces.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then_with(|| a.ip().to_string().cmp(&b.ip().to_string()))
    });

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

fn netmask_to_prefix(mask: Ipv4Addr) -> u32 {
    mask.octets().iter().map(|b| b.count_ones()).sum()
}

fn network_of(ip: Ipv4Addr, mask: Ipv4Addr) -> Ipv4Addr {
    let ip_u = u32::from(ip);
    let mk_u = u32::from(mask);
    Ipv4Addr::from(ip_u & mk_u)
}
