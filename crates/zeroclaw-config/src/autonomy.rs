use serde::{Deserialize, Serialize};

/// How much autonomy the agent has.
///
/// Variants are ordered from least to most autonomous so that
/// [`Ord`] / [`PartialOrd`] compare a child's level against a
/// parent's during SubAgent escalation checks (`child <= parent`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub enum AutonomyLevel {
    /// Read-only: can observe but not act
    ReadOnly,
    /// Supervised: acts but requires approval for risky operations
    #[default]
    Supervised,
    /// Full: autonomous execution within policy bounds
    Full,
}

/// Whether a risk profile may delegate work to other agents, and to which.
///
/// `Forbidden` is the default: a profile that does not declare `delegation`
/// cannot delegate at all. `Allow { agents }` names the agent aliases this
/// profile may delegate to; the dispatcher is never included in its own list.
/// Delegation is gated on the caller and target sharing a risk profile, so the
/// allow-list authorizes which same-profile agents are reachable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "schema-export", derive(schemars::JsonSchema))]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum DelegationPolicy {
    #[default]
    Forbidden,
    Allow { agents: Vec<String> },
}

impl DelegationPolicy {
    /// Whether this profile may delegate to `target_alias`.
    pub fn permits(&self, target_alias: &str) -> bool {
        match self {
            Self::Forbidden => false,
            Self::Allow { agents } => agents.iter().any(|a| a == target_alias),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delegation_default_is_forbidden() {
        assert!(matches!(
            DelegationPolicy::default(),
            DelegationPolicy::Forbidden
        ));
        assert!(!DelegationPolicy::default().permits("clamps"));
    }

    #[test]
    fn delegation_allow_gates_on_membership() {
        let p = DelegationPolicy::Allow {
            agents: vec!["clamps".to_string(), "glados".to_string()],
        };
        assert!(p.permits("clamps"));
        assert!(p.permits("glados"));
        assert!(!p.permits("lineation"));
    }
}
