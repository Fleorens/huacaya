//! Per-interface network rate histories, refreshed from each system snapshot (T38).

use crate::app::state::HISTORY_CAPACITY;
use crate::collect::net::NetSample;
use crate::util::History;

/// Refresh `net_rates` from `nets`: for each interface with both rates, push rx then tx (creating the
/// entry, in `nets`'s order, when missing); drop the entries whose interface is gone.
pub fn update_net_rates(net_rates: &mut Vec<(String, History, History)>, nets: &[NetSample]) {
    net_rates.retain(|(iface, _, _)| nets.iter().any(|n| n.iface == *iface));
    for n in nets {
        if let (Some(rx), Some(tx)) = (n.rx_bps, n.tx_bps) {
            // The entry's index, creating it (in `nets`'s order) when the interface is new.
            let idx = net_rates.iter().position(|(iface, _, _)| iface == &n.iface).unwrap_or_else(|| {
                net_rates.push((
                    n.iface.clone(),
                    History::new(HISTORY_CAPACITY),
                    History::new(HISTORY_CAPACITY),
                ));
                net_rates.len() - 1
            });
            net_rates[idx].1.push(rx as f32);
            net_rates[idx].2.push(tx as f32);
        }
    }
}
