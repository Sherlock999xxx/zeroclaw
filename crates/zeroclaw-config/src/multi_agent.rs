//! Multi-agent runtime types: alias newtypes, access-mode enum, and peer
//! external entries. Backs Issue #6272.
//!
//! These types are the schema-as-law primitives for the multi-agent
//! features landing in v0.8.0:
//!
//! - [`AgentAlias`], [`PeerGroupName`], [`PeerUsername`] are typed string
//!   newtypes that carry their meaning at the type level. They use the
//!   shared `define_provider_ref!` macro defined in [`crate::providers`]
//!   so the on-disk TOML shape stays plain-string while consumers see a
//!   typed value.
//! - [`AccessMode`] is the cross-agent filesystem grant. Read-only,
//!   write-only, or read-write. Default for cross-agent access maps is
//!   "key absent = no grant"; this enum encodes only the granted modes.
//! - [`PeerExternal`] is a single non-agent member of a peer group
//!   (humans, external bots) on the group's channel.
//!
//! Cross-agent semantics, peer-group resolution, and Hand permission
//! inheritance live in the runtime crate; this module only carries the
//! data shapes.

use serde::{Deserialize, Serialize};

crate::define_provider_ref!(AgentAlias, "agents");
crate::define_provider_ref!(PeerGroupName, "peer_groups");
crate::define_provider_ref!(PeerUsername, "channels.peers");

/// Cross-agent filesystem grant.
///
/// Used as the value type in `[agents.<alias>.workspace.access]` maps.
/// A missing entry means no cross-agent access at all (jailed). The enum
/// only encodes the granted modes; absence is the safe default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum AccessMode {
    /// Read access only. Cross-agent `file_read` is permitted; writes are not.
    Read,
    /// Write access only. Cross-agent `file_write` is permitted; reads are not.
    Write,
    /// Both read and write. The agent can `file_read` and `file_write` against
    /// the target's workspace.
    ReadWrite,
}

impl AccessMode {
    /// Whether this mode includes read access.
    #[must_use]
    pub const fn allows_read(self) -> bool {
        matches!(self, Self::Read | Self::ReadWrite)
    }

    /// Whether this mode includes write access.
    #[must_use]
    pub const fn allows_write(self) -> bool {
        matches!(self, Self::Write | Self::ReadWrite)
    }
}

/// Single non-agent member of a peer group: a human or an external bot reachable
/// at `username` on the group's `channel`. The channel ref lives on the group,
/// so the entry only carries the username.
///
/// Lifted into `[[peer_groups.<name>.external_peers]]` and
/// `[[peer_groups.<name>.ignore]]` arrays.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
pub struct PeerExternal {
    /// The on-channel username, formatted as the channel kind expects (e.g.
    /// `@beta_bot` for Telegram, `Audacity#0001` for Discord). Validation lives
    /// in `Config::validate()` once the channel kind is known.
    pub username: PeerUsername,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_alias_round_trips_through_serde() {
        // TOML's root must be a table; in real usage AgentAlias lives inside
        // structs. Round-tripping through JSON exercises the same serde path
        // as serialization inside a struct.
        let alias = AgentAlias::new("researcher");
        let json = serde_json::to_string(&alias).unwrap();
        assert_eq!(json, "\"researcher\"");
        let back: AgentAlias = serde_json::from_str(&json).unwrap();
        assert_eq!(alias, back);
    }

    #[test]
    fn access_mode_serializes_snake_case() {
        let cases = [
            (AccessMode::Read, "\"read\""),
            (AccessMode::Write, "\"write\""),
            (AccessMode::ReadWrite, "\"read_write\""),
        ];
        for (mode, expected) in cases {
            let json = serde_json::to_string(&mode).unwrap();
            assert_eq!(json, expected, "mode={mode:?}");
            let back: AccessMode = serde_json::from_str(&json).unwrap();
            assert_eq!(back, mode);
        }
    }

    #[test]
    fn access_mode_capability_predicates() {
        assert!(AccessMode::Read.allows_read());
        assert!(!AccessMode::Read.allows_write());
        assert!(!AccessMode::Write.allows_read());
        assert!(AccessMode::Write.allows_write());
        assert!(AccessMode::ReadWrite.allows_read());
        assert!(AccessMode::ReadWrite.allows_write());
    }

    #[test]
    fn peer_external_round_trips() {
        let entry = PeerExternal {
            username: PeerUsername::new("@beta_bot"),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let back: PeerExternal = serde_json::from_str(&json).unwrap();
        assert_eq!(entry, back);
    }

    #[test]
    fn peer_external_round_trips_through_toml_array() {
        // Real-world shape: peer_groups.<name>.external_peers is an array of
        // tables. Validate the typed shape parses cleanly from that form.
        let toml_input = r#"
[[external_peers]]
username = "@user_1"

[[external_peers]]
username = "@user_2"
"#;
        #[derive(Deserialize)]
        struct Wrapper {
            external_peers: Vec<PeerExternal>,
        }
        let parsed: Wrapper = toml::from_str(toml_input).unwrap();
        assert_eq!(parsed.external_peers.len(), 2);
        assert_eq!(parsed.external_peers[0].username, "@user_1");
        assert_eq!(parsed.external_peers[1].username, "@user_2");
    }

    #[test]
    fn alias_newtypes_are_distinct_at_type_level() {
        // Compile-time: AgentAlias and PeerGroupName don't accidentally
        // assign to each other. The cast through `String` is the only path.
        let agent = AgentAlias::new("alpha");
        let group: PeerGroupName = PeerGroupName::new(agent.as_str());
        assert_eq!(agent.as_str(), group.as_str());
    }
}
