//! What one member may do in one workspace: owner flag + union of their live teams' grants.

use std::collections::BTreeSet;

use crate::modules::permissions::catalog::{FRONTEND_ONLY, WORKSPACE_ADMIN, WORKSPACE_OWNER};

/// Effective permissions of a member in a workspace (Laravel `User::workspacePermissions`).
#[derive(Debug, Default)]
pub struct Granted {
    owner: bool,
    names: BTreeSet<String>,
}

impl Granted {
    /// `names`: team grants, any order, duplicates allowed.
    pub fn new(owner: bool, names: impl IntoIterator<Item = String>) -> Self {
        Self {
            owner,
            names: names.into_iter().collect(),
        }
    }

    /// Laravel `hasWorkspacePermission` + MainPolicy frontend-only bypass.
    pub fn allows(&self, name: &str) -> bool {
        self.owner
            || self.names.contains(WORKSPACE_ADMIN)
            || FRONTEND_ONLY.contains(&name)
            || self.names.contains(name)
    }

    /// True for the workspace creator.
    pub fn is_owner(&self) -> bool {
        self.owner
    }

    /// Owner or holder of `workspace_admin`.
    pub fn is_admin(&self) -> bool {
        self.owner || self.names.contains(WORKSPACE_ADMIN)
    }

    /// /me list: grants sorted, then `workspace_owner` (Laravel appends it last).
    pub fn to_list(&self) -> Vec<String> {
        let mut list: Vec<String> = self.names.iter().cloned().collect();
        if self.owner {
            list.push(WORKSPACE_OWNER.to_owned());
        }
        list
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn granted(owner: bool, names: &[&str]) -> Granted {
        Granted::new(owner, names.iter().map(|n| n.to_string()))
    }

    #[test]
    fn exact_name_needed_without_superuser() {
        let g = granted(false, &["relation.list"]);

        assert!(g.allows("relation.list"));
        assert!(!g.allows("relation.create"));
        assert!(!g.allows("relation"));
    }

    #[test]
    fn owner_and_workspace_admin_pass_everything() {
        assert!(granted(true, &[]).allows("invoice.delete"));
        assert!(granted(false, &[WORKSPACE_ADMIN]).allows("invoice.delete"));
    }

    #[test]
    fn admin_means_owner_or_workspace_admin_grant() {
        assert!(granted(true, &[]).is_admin());
        assert!(granted(false, &[WORKSPACE_ADMIN]).is_admin());
        assert!(!granted(false, &["team.invite"]).is_admin());
    }

    #[test]
    fn frontend_only_always_passes() {
        assert!(granted(false, &[]).allows("product.view_margin"));
    }

    #[test]
    fn list_dedups_sorts_and_appends_owner_last() {
        let g = granted(true, &["team.list", "relation.list", "team.list"]);

        assert_eq!(
            g.to_list(),
            ["relation.list", "team.list", "workspace_owner"]
        );
        assert!(granted(false, &[]).to_list().is_empty());
    }
}
