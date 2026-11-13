//! Adapters for plugins that answer over a protocol of their own.
//!
//! A plugin is meant to claim its namespace with `register_expansion`. Some
//! published plugins already answer other plugins over a documented IPC
//! protocol and have not registered one, so their values exist and no
//! placeholder reaches them. An adapter is the provider asking such a plugin in
//! that plugin's own protocol.
//!
//! An adapter is a stopgap by design. A registered expansion always wins, so
//! the day the plugin claims its namespace its adapter goes quiet and can be
//! deleted. An adapter only reads what the plugin's author published as an API
//! for other plugins.
//!
//! Everything here is plain data in and plain data out. The provider hands an
//! adapter the viewer as a [`Viewer`] and the IPC call as an [`Ask`], so an
//! adapter is tested on the host with a closure in place of the other plugin.

use pumpkin_plugin_api::uuid::Uuid;

mod pvault;
mod simplelang;

/// The player a value is resolved for.
pub struct Viewer {
    /// The player's unique id.
    pub uuid: Uuid,
    /// The language the player's client is set to, such as `en_us`.
    pub locale: String,
}

/// Sends one message to the adapter's plugin and returns its reply, or [`None`]
/// when the plugin is not installed or refused the bytes.
pub type Ask<'a> = &'a dyn Fn(&[u8]) -> Option<Vec<u8>>;

/// One plugin the provider asks in that plugin's own protocol.
pub struct Adapter {
    /// The namespace its placeholders live under.
    pub namespace: &'static str,
    /// The plugin's name, which is its IPC address.
    pub plugin: &'static str,
    /// Every name the adapter answers, with one line about each for `/pie`.
    ///
    /// Unlike an expansion's list this one is a filter: a name that is not here
    /// is never sent to the plugin.
    pub names: &'static [(&'static str, &'static str)],
    /// How long the provider may reuse a value, per viewer, in milliseconds.
    pub ttl_ms: u64,
    /// Whether the plugin is installed and speaks the protocol the adapter does.
    pub probe: fn(Ask<'_>) -> bool,
    /// Computes one value, or [`None`] to leave the placeholder alone.
    pub resolve: fn(&str, Option<&Viewer>, Ask<'_>) -> Option<String>,
}

/// Every adapter, in the order `/pie` lists them.
pub static ADAPTERS: &[Adapter] = &[pvault::ADAPTER, simplelang::ADAPTER];

/// The adapter that answers `id`, and the name to ask it for, lowercased.
#[must_use]
pub fn find(id: &str) -> Option<(&'static Adapter, String)> {
    let lowered = id.to_ascii_lowercase();
    let (namespace, name) = lowered.split_once('_')?;
    let adapter = ADAPTERS
        .iter()
        .find(|adapter| adapter.namespace == namespace)?;
    adapter
        .names
        .iter()
        .any(|(known, _)| *known == name)
        .then(|| (adapter, name.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pumpkin_pie::protocol::{check_name, check_namespace};

    #[test]
    fn an_id_finds_its_adapter_whatever_its_case() {
        let (adapter, name) = find("PVault_Balance").unwrap();
        assert_eq!(adapter.plugin, "PVault");
        assert_eq!(name, "balance");
    }

    #[test]
    fn a_name_the_adapter_does_not_list_is_nobodys() {
        assert!(find("pvault_nonsense").is_none());
        assert!(find("pvault").is_none());
        assert!(find("nobody_balance").is_none());
    }

    #[test]
    fn every_adapter_would_pass_as_an_expansion() {
        // An adapter stands in for the expansion its plugin has not registered,
        // so its namespace and names must be ones `register_expansion` accepts.
        // Otherwise the plugin could never take the namespace over.
        for adapter in ADAPTERS {
            assert_eq!(check_namespace(adapter.namespace), Ok(adapter.namespace));
            for (name, description) in adapter.names {
                assert!(check_name(name).is_ok(), "{name}");
                assert!(!description.is_empty(), "{name} needs a description");
            }
        }
        let mut namespaces: Vec<_> = ADAPTERS.iter().map(|adapter| adapter.namespace).collect();
        namespaces.sort_unstable();
        namespaces.dedup();
        assert_eq!(namespaces.len(), ADAPTERS.len());
    }
}
