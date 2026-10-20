//! Workspace value types: type enum + name slug rule, mirroring DB CHECKs.

/// `workspaces.workspace_type` values (CHECK `workspaces_workspace_type_check`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorkspaceType {
    #[default]
    Retail,
    Manufacturing,
}

impl WorkspaceType {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "retail" => Some(Self::Retail),
            "manufacturing" => Some(Self::Manufacturing),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Retail => "retail",
            Self::Manufacturing => "manufacturing",
        }
    }
}

/// `a-z0-9` groups joined by single dashes. Same rule as CHECK `workspaces_name_slug`.
pub fn is_slug(value: &str) -> bool {
    value.split('-').all(|part| {
        !part.is_empty()
            && part
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_type_round_trips() {
        for ty in [WorkspaceType::Retail, WorkspaceType::Manufacturing] {
            assert_eq!(WorkspaceType::parse(ty.as_str()), Some(ty));
        }
        assert_eq!(WorkspaceType::default(), WorkspaceType::Retail);
    }

    #[test]
    fn workspace_type_rejects_unknown_and_case_variants() {
        for bad in ["bakery", "Retail", "RETAIL", ""] {
            assert_eq!(WorkspaceType::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn slug_rule_matches_db_check() {
        for ok in ["acme", "acme-shop-2", "a", "123"] {
            assert!(is_slug(ok), "rejected {ok}");
        }
        for bad in [
            "",
            "Acme",
            "acme shop",
            "-acme",
            "acme-",
            "acme--shop",
            "acme_shop",
            "café",
        ] {
            assert!(!is_slug(bad), "accepted {bad:?}");
        }
    }
}
