use super::AppState;
use crate::organization::MissionTarget;

impl AppState {
    /// Transfer an existing complete tab. All fallible work precedes detachment.
    pub(crate) fn transfer_tab(
        &mut self,
        tab_id: &str,
        workspace_id: &str,
        insert_index: usize,
    ) -> Result<String, &'static str> {
        let (source, tab_index) = self
            .workspaces
            .iter()
            .enumerate()
            .find_map(|(index, ws)| {
                ws.tabs
                    .iter()
                    .position(|tab| {
                        crate::workspace::public_tab_id_for_number(&ws.id, tab.number) == tab_id
                    })
                    .map(|tab| (index, tab))
            })
            .ok_or("tab_not_found")?;
        let destination = self
            .workspaces
            .iter()
            .position(|ws| ws.id == workspace_id)
            .ok_or("workspace_not_found")?;
        if destination == source || insert_index > self.workspaces[destination].tabs.len() {
            return Err("tab_transfer_failed");
        }
        let next_tab = self.workspaces[destination].next_public_tab_number;
        next_tab.checked_add(1).ok_or("public_id_exhausted")?;
        let pane_ids = self.workspaces[source].tabs[tab_index].layout.pane_ids();
        self.workspaces[destination]
            .next_public_pane_number
            .checked_add(pane_ids.len())
            .ok_or("public_id_exhausted")?;
        let new_id = crate::workspace::public_tab_id_for_number(workspace_id, next_tab);
        let mut organization = self.organization.clone();
        organization.transfer_mission_target(
            &MissionTarget::Tab {
                tab_id: tab_id.to_owned(),
            },
            MissionTarget::Tab {
                tab_id: new_id.clone(),
            },
        )?;
        let pane_identities = pane_ids
            .iter()
            .enumerate()
            .filter_map(|(offset, pane)| {
                let old = self.workspaces[source].public_pane_number(*pane)?;
                Some((
                    crate::workspace::public_pane_id_for_number(&self.workspaces[source].id, old),
                    crate::workspace::public_pane_id_for_number(
                        workspace_id,
                        self.workspaces[destination].next_public_pane_number + offset,
                    ),
                ))
            })
            .collect::<Vec<_>>();
        organization.relocate_pane_missions(&pane_identities)?;
        // Preflight source cleanup too, so revision exhaustion never leaves a partial move.
        if self.workspaces[source].tabs.len() == 1 {
            let live = self
                .workspaces
                .iter()
                .enumerate()
                .filter(|(idx, _)| *idx != source)
                .map(|(_, ws)| Self::family_id(ws))
                .collect::<Vec<_>>();
            organization.retain_live_standalone_families(&live)?;
        }
        let focused = self.current_pane_focus_target();
        let selected_id = self.workspaces.get(self.selected).map(|ws| ws.id.clone());
        let old_pane_ids = pane_ids
            .iter()
            .filter_map(|pane| {
                self.workspaces[source]
                    .public_pane_number(*pane)
                    .map(|number| {
                        (
                            crate::workspace::public_pane_id_for_number(
                                &self.workspaces[source].id,
                                number,
                            ),
                            *pane,
                        )
                    })
            })
            .collect::<Vec<_>>();
        let tab = self.workspaces[source].take_tab_for_transfer(tab_index);
        self.workspaces[destination].insert_transferred_tab(insert_index, tab);
        self.organization = organization;
        for (id, pane) in old_pane_ids {
            self.public_pane_id_aliases.insert(id, pane);
        }
        if let Some(previous) = &mut self.previous_pane_focus {
            if pane_ids.contains(&previous.pane_id) {
                previous.workspace_id = workspace_id.to_owned();
            }
        }
        if let Some(target) = self.toast.as_mut().and_then(|toast| toast.target.as_mut()) {
            if pane_ids.contains(&target.pane_id) {
                target.workspace_id = workspace_id.to_owned();
            }
        }
        for notification in self.pending_agent_notifications.values_mut() {
            if pane_ids.contains(&notification.pane_id) {
                notification.workspace_id = workspace_id.to_owned();
            }
        }
        if self.workspaces[source].tabs.is_empty() {
            self.close_workspaces(vec![source]);
        }
        if let Some(focused) = focused {
            let ws_id = if pane_ids.contains(&focused.pane_id) {
                workspace_id
            } else {
                &focused.workspace_id
            };
            if let Some(index) = self.workspaces.iter().position(|ws| ws.id == ws_id) {
                self.active = Some(index);
                if let Some(tab) = self.workspaces[index].find_tab_index_for_pane(focused.pane_id) {
                    self.workspaces[index].active_tab = tab;
                }
            }
        }
        if let Some(index) =
            selected_id.and_then(|id| self.workspaces.iter().position(|ws| ws.id == id))
        {
            self.selected = index;
        }
        self.mark_session_dirty();
        Ok(new_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mc_s3_transfer_final_tab_preserves_inactive_focus_and_rejects_exhaustion() {
        let mut state = AppState::test_with_adversarial_identity_state();
        state
            .workspaces
            .push(crate::workspace::Workspace::test_new("Source"));
        state.ensure_test_terminals();
        let id = crate::workspace::public_tab_id_for_number(&state.workspaces[1].id, 1);
        let destination = state.workspaces[0].id.clone();
        let focused = state.current_pane_focus_target();
        let mission = state.create_mission("Empty source".into(), None).unwrap();
        state
            .assign_mission(
                MissionTarget::Tab { tab_id: id.clone() },
                mission.id.clone(),
            )
            .unwrap();
        state.organization.revision = u64::MAX - 1;
        let catalog = state.organization.clone();
        assert_eq!(
            state.transfer_tab(&id, &destination, 0),
            Err("organization_revision_exhausted")
        );
        assert_eq!(state.organization, catalog);
        assert_eq!(state.workspaces.len(), 2);
        state.assert_invariants_for_test();
        state.organization.revision = 2;
        let moved = state.transfer_tab(&id, &destination, 0).unwrap();
        assert_eq!(state.workspaces.len(), 1);
        assert_eq!(state.current_pane_focus_target(), focused);
        assert_eq!(
            state
                .organization
                .mission_for(&MissionTarget::Tab { tab_id: moved }),
            Some(&mission.id)
        );
        state.assert_invariants_for_test();
    }
}
