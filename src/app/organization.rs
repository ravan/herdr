use super::AppState;
use crate::organization::{Collection, CollectionId, CollectionName, FamilyId};

impl AppState {
    pub(crate) fn set_workspace_worktree_membership(
        &mut self,
        workspace_id: &str,
        membership: crate::workspace::WorktreeSpaceMembership,
    ) -> Result<bool, &'static str> {
        let index = self
            .workspaces
            .iter()
            .position(|workspace| workspace.id == workspace_id)
            .ok_or("family_not_found")?;
        if self.workspaces[index].worktree_space.as_ref() == Some(&membership) {
            return Ok(false);
        }
        let old = Self::family_id(&self.workspaces[index]);
        if matches!(old, FamilyId::Standalone { .. }) {
            self.organization.transfer_family(
                &old,
                FamilyId::Managed {
                    key: membership.key.clone(),
                },
            )?;
        }
        self.workspaces[index].worktree_space = Some(membership);
        self.mark_session_dirty();
        Ok(true)
    }
    pub(crate) fn reconcile_organization_families(
        &mut self,
    ) -> Result<Vec<FamilyId>, &'static str> {
        let live = self
            .workspaces
            .iter()
            .map(Self::family_id)
            .collect::<Vec<_>>();
        let removed = self.organization.retain_live_standalone_families(&live)?;
        if !removed.is_empty() {
            self.mark_session_dirty();
        }
        Ok(removed)
    }
    pub(crate) fn create_collection(&mut self, name: String) -> Result<Collection, &'static str> {
        let name = CollectionName::try_from(name)?;
        let revision = self
            .organization
            .revision
            .checked_add(1)
            .ok_or("organization_revision_exhausted")?;
        let collection = self
            .organization
            .create_collection(CollectionId(format!("collection_{revision}")), name)?;
        self.mark_session_dirty();
        Ok(collection)
    }

    pub(crate) fn set_collection_hibernating(
        &mut self,
        id: CollectionId,
        hibernating: bool,
    ) -> Result<(), &'static str> {
        if self.organization.set_hibernating(&id, hibernating)? {
            self.mark_session_dirty();
        }
        Ok(())
    }

    pub(crate) fn family_id(workspace: &crate::workspace::Workspace) -> FamilyId {
        match workspace.worktree_space.as_ref() {
            Some(membership) => FamilyId::Managed {
                key: membership.key.clone(),
            },
            None => FamilyId::Standalone {
                workspace_id: workspace.id.clone(),
            },
        }
    }

    pub(crate) fn assign_family_to_collection(
        &mut self,
        family: FamilyId,
        collection: CollectionId,
    ) -> Result<(), &'static str> {
        let live = self
            .workspaces
            .iter()
            .map(Self::family_id)
            .collect::<Vec<_>>();
        self.organization.assign_family(family, collection, &live)?;
        self.mark_session_dirty();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mc_s1_closing_the_last_standalone_removes_its_reference_but_keeps_the_collection() {
        let mut state = AppState::test_with_adversarial_identity_state();
        let family = AppState::family_id(&state.workspaces[0]);
        let collection = state.create_collection("Agent workshop".into()).unwrap();
        state
            .assign_family_to_collection(family, collection.id.clone())
            .unwrap();
        state.close_workspaces(vec![0]);
        state.assert_invariants_for_test();
        assert!(state.organization.family_assignments.is_empty());
        assert_eq!(state.organization.collections, vec![collection]);
        assert_eq!(state.organization.revision, 3);
        assert!(state.session_dirty);
    }

    #[test]
    fn mc_s1_terminal_exit_cleans_membership_even_at_the_reserved_revision_boundary() {
        let mut state = AppState::test_with_adversarial_identity_state();
        let family = AppState::family_id(&state.workspaces[0]);
        let collection = state.create_collection("Agent workshop".into()).unwrap();
        state
            .assign_family_to_collection(family, collection.id.clone())
            .unwrap();
        state.organization.revision = u64::MAX - 1;
        let panes = state.workspaces[0]
            .tabs
            .iter()
            .flat_map(|tab| tab.layout.pane_ids())
            .collect::<Vec<_>>();
        for pane_id in panes {
            state.handle_app_event(crate::events::AppEvent::PaneDied {
                pane_id,
                exit_reason: crate::platform::ChildExitReason::Exited,
            });
            state.assert_invariants_for_test();
        }
        assert!(state.workspaces.is_empty());
        assert!(state.organization.family_assignments.is_empty());
        assert_eq!(state.organization.revision, u64::MAX);
        assert_eq!(state.organization.collections, vec![collection]);
    }

    #[test]
    fn mc_s1_standalone_membership_transfers_atomically_and_managed_membership_survives_closure() {
        let mut state = AppState::test_with_adversarial_identity_state();
        let workspace_id = state.workspaces[0].id.clone();
        let standalone = AppState::family_id(&state.workspaces[0]);
        let collection = state.create_collection("Agent workshop".into()).unwrap();
        state
            .assign_family_to_collection(standalone.clone(), collection.id.clone())
            .unwrap();
        let membership = crate::workspace::WorktreeSpaceMembership {
            key: "repo-key".into(),
            label: "repo".into(),
            repo_root: "/repo".into(),
            checkout_path: "/repo".into(),
            is_linked_worktree: false,
        };
        let capture_tabs = |state: &AppState| {
            serde_json::to_value(crate::persist::capture(
                &state.workspaces,
                &state.terminals,
                &crate::terminal::TerminalRuntimeRegistry::new(),
                state.active,
                state.selected,
            ))
            .unwrap()["workspaces"][0]["tabs"]
                .clone()
        };
        let before = capture_tabs(&state);
        state
            .set_workspace_worktree_membership(&workspace_id, membership.clone())
            .unwrap();
        assert_eq!(
            state.organization.collection_for(&FamilyId::Managed {
                key: "repo-key".into()
            }),
            Some(&collection.id)
        );
        assert_eq!(state.organization.collection_for(&standalone), None);
        assert_eq!(state.organization.revision, 3);
        assert_eq!(capture_tabs(&state), before);
        state.assert_invariants_for_test();
        state.close_workspaces(vec![0]);
        state.assert_invariants_for_test();
        assert_eq!(
            state.organization.collection_for(&FamilyId::Managed {
                key: "repo-key".into()
            }),
            Some(&collection.id)
        );
        let mut reopened = crate::workspace::Workspace::test_new("reopened");
        reopened.worktree_space = Some(membership);
        state.workspaces.push(reopened);
        state.active = Some(0);
        state.ensure_test_terminals();
        assert_eq!(
            state
                .organization
                .collection_for(&AppState::family_id(&state.workspaces[0])),
            Some(&collection.id)
        );
        state.assert_invariants_for_test();
    }
}

#[cfg(test)]
mod hibernate_tests {
    use super::*;
    #[test]
    fn mc_s2_hibernate_preserves_family_focus_and_terminal_identity() {
        let mut state = AppState::test_with_adversarial_identity_state();
        let collection = state.create_collection("Infrastructure".into()).unwrap();
        let family = AppState::family_id(&state.workspaces[0]);
        state
            .assign_family_to_collection(family.clone(), collection.id.clone())
            .unwrap();
        let before = serde_json::to_value(crate::persist::capture(
            &state.workspaces,
            &state.terminals,
            &crate::terminal::TerminalRuntimeRegistry::new(),
            state.active,
            state.selected,
        ))
        .unwrap();
        for (hibernating, revision) in [(true, 3), (false, 4)] {
            state.session_dirty = false;
            state
                .set_collection_hibernating(collection.id.clone(), hibernating)
                .unwrap();
            assert_eq!(state.organization.collections[0].hibernating, hibernating);
            assert_eq!(state.organization.revision, revision);
            assert!(state.session_dirty);
            assert_eq!(
                state.organization.collection_for(&family),
                Some(&collection.id)
            );
            assert_eq!(
                serde_json::to_value(crate::persist::capture(
                    &state.workspaces,
                    &state.terminals,
                    &crate::terminal::TerminalRuntimeRegistry::new(),
                    state.active,
                    state.selected
                ))
                .unwrap(),
                before
            );
            state.assert_invariants_for_test();
        }
    }
}
