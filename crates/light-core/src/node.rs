//! Node and device accounting.
//!
//! Formal definitions (approved in F1 authorisation §2.3):
//!
//! | term | definition |
//! |---|---|
//! | **Identity** | The local light seed. Holds a root key; its hash is the `<br>`anonymous id. |
//! | **Device** | One installation / running instance. Replaceable, recoverable. |
//! | **Node** | One anonymous seed identity participating in the network. |
//! | **Model Family** | A de-duplicated family from the registry. |
//! | **Ignition Event** | One valid light-up: a verified family first counted for a node. |
//! | **Transmission** | One assistance relation between two nodes. |
//! | **Activation** | The receiver's first *verified* usage after the relation formed. |
//!
//! Hard rules:
//!
//! - Restoring the same seed on several devices is **one node**.
//! - Different seeds are **never merged** into one person. There is deliberately
//!   no API on this type that would do so.
//! - Counts here are node counts. They must never be presented to users as a
//!   number of people.

use std::collections::{BTreeMap, BTreeSet};

/// Tracks which devices belong to which node, and which nodes a node activated.
#[derive(Debug, Default, Clone)]
pub struct NodeBook {
    /// device_id -> anon_id. One node may own many devices.
    devices: BTreeMap<String, String>,
    /// helper anon_id -> the distinct helped anon_ids that reached ACTIVATED.
    activations: BTreeMap<String, BTreeSet<String>>,
}

impl NodeBook {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that `device_id` is running the seed `anon_id`.
    ///
    /// Re-registering the same device is idempotent. A device cannot silently
    /// switch identity: that would mean data from two nodes got mixed.
    pub fn attach_device(&mut self, device_id: &str, anon_id: &str) -> Result<(), NodeBookError> {
        if let Some(existing) = self.devices.get(device_id) {
            if existing != anon_id {
                return Err(NodeBookError::DeviceIdentityConflict {
                    device_id: device_id.to_string(),
                });
            }
            return Ok(());
        }
        self.devices.insert(device_id.to_string(), anon_id.to_string());
        Ok(())
    }

    /// Distinct nodes. Restoring one seed onto N devices still yields 1.
    pub fn node_count(&self) -> usize {
        self.devices.values().collect::<BTreeSet<_>>().len()
    }

    pub fn device_count(&self) -> usize {
        self.devices.len()
    }

    /// Devices currently bound to a node, sorted for deterministic output.
    pub fn devices_of(&self, anon_id: &str) -> Vec<&str> {
        self.devices
            .iter()
            .filter(|(_, node)| node.as_str() == anon_id)
            .map(|(device, _)| device.as_str())
            .collect()
    }

    pub fn is_same_node(&self, device_a: &str, device_b: &str) -> bool {
        match (self.devices.get(device_a), self.devices.get(device_b)) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        }
    }

    /// Record a completed activation of `helped` by `helper`.
    ///
    /// The same pair only ever counts once (matches the
    /// `unique (helper_id, helped_id)` constraint in the server schema).
    pub fn record_activation(&mut self, helper: &str, helped: &str) {
        self.activations
            .entry(helper.to_string())
            .or_default()
            .insert(helped.to_string());
    }

    /// Activated node count for the transmission title.
    pub fn activated_node_count(&self, helper: &str) -> u32 {
        self.activations
            .get(helper)
            .map(|s| s.len() as u32)
            .unwrap_or(0)
    }

    /// True if any node has activated `helped` — used to enforce
    /// "first qualifying relation wins".
    pub fn already_activated_by(&self, helped: &str) -> Option<&str> {
        self.activations
            .iter()
            .find(|(_, helped_set)| helped_set.contains(helped))
            .map(|(helper, _)| helper.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NodeBookError {
    #[error("device `{device_id}` is already bound to a different node identity")]
    DeviceIdentityConflict { device_id: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    const NODE_A: &str = "fl1_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const NODE_B: &str = "fl1_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn same_seed_on_many_devices_is_one_node() {
        let mut book = NodeBook::new();
        book.attach_device("macbook", NODE_A).unwrap();
        book.attach_device("iphone", NODE_A).unwrap();
        book.attach_device("ipad", NODE_A).unwrap();

        assert_eq!(book.device_count(), 3);
        assert_eq!(book.node_count(), 1, "restoring one seed must not create new nodes");
        assert!(book.is_same_node("macbook", "ipad"));
        assert_eq!(book.devices_of(NODE_A), vec!["ipad", "iphone", "macbook"]);
    }

    #[test]
    fn different_seeds_are_never_merged() {
        let mut book = NodeBook::new();
        book.attach_device("macbook", NODE_A).unwrap();
        book.attach_device("phone", NODE_B).unwrap();
        assert_eq!(book.node_count(), 2);
        assert!(!book.is_same_node("macbook", "phone"));
        // There is intentionally no API to declare these the same person.
    }

    #[test]
    fn reattaching_the_same_device_is_idempotent() {
        let mut book = NodeBook::new();
        book.attach_device("macbook", NODE_A).unwrap();
        book.attach_device("macbook", NODE_A).unwrap();
        assert_eq!(book.device_count(), 1);
        assert_eq!(book.node_count(), 1);
    }

    #[test]
    fn a_device_cannot_silently_switch_identity() {
        let mut book = NodeBook::new();
        book.attach_device("macbook", NODE_A).unwrap();
        let err = book.attach_device("macbook", NODE_B).unwrap_err();
        assert_eq!(
            err,
            NodeBookError::DeviceIdentityConflict { device_id: "macbook".into() }
        );
        // state unchanged
        assert_eq!(book.node_count(), 1);
    }

    #[test]
    fn activations_dedupe_by_node_pair() {
        let mut book = NodeBook::new();
        book.record_activation(NODE_A, NODE_B);
        book.record_activation(NODE_A, NODE_B);
        book.record_activation(NODE_A, NODE_B);
        assert_eq!(book.activated_node_count(NODE_A), 1);
        assert_eq!(book.already_activated_by(NODE_B), Some(NODE_A));
    }

    #[test]
    fn restoring_a_seed_does_not_inflate_activation_counts() {
        // The receiver restores onto a second device and activates again.
        let mut book = NodeBook::new();
        book.attach_device("mac", NODE_A).unwrap();
        book.attach_device("phone", NODE_B).unwrap();
        book.attach_device("tablet", NODE_B).unwrap();
        book.record_activation(NODE_A, NODE_B);
        book.record_activation(NODE_A, NODE_B);
        assert_eq!(book.node_count(), 2);
        assert_eq!(book.device_count(), 3);
        assert_eq!(book.activated_node_count(NODE_A), 1);
    }

    #[test]
    fn unknown_node_has_zero_activations() {
        let book = NodeBook::new();
        assert_eq!(book.activated_node_count(NODE_A), 0);
        assert_eq!(book.node_count(), 0);
    }
}
