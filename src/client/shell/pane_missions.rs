//! Captured pane assignment actions; terminal identity and confirmed catalog stay authoritative.
use super::*;

#[derive(Clone, Debug)]
pub(super) struct PaneMissionMenuContext {
    pub(super) workspace: WorkspaceNavigationTarget,
    pub(super) pane_id: String,
    pub(super) membership: String,
    pub(super) inherited: String,
    pub(super) explicit: bool,
}

impl ClientShellState {
    pub(super) fn pane_mission_context(&self, pane_id: &str) -> Option<PaneMissionMenuContext> {
        let endpoint = self
            .endpoints
            .iter()
            .find(|e| e.endpoint_id == self.active_endpoint_id)?;
        let pane = endpoint
            .snapshot
            .as_ref()?
            .panes
            .iter()
            .find(|p| p.pane_id == pane_id)?;
        let membership = endpoint.pane_missions.get(pane_id);
        Some(PaneMissionMenuContext {
            workspace: self.navigation_target(&endpoint.endpoint_id, &pane.workspace_id)?,
            pane_id: pane_id.to_owned(),
            membership: membership
                .and_then(|m| m.label.clone())
                .unwrap_or_else(|| "Unassigned".into()),
            inherited: membership
                .and_then(|m| m.inherited_label.clone())
                .unwrap_or_else(|| "Unassigned".into()),
            explicit: membership.is_some_and(|m| m.explicit),
        })
    }

    fn pane_mission_context_valid(&self, context: &PaneMissionMenuContext) -> bool {
        context.workspace.endpoint_id == self.active_endpoint_id
            && self.navigation_target_valid(&context.workspace)
            && self.snapshot.as_ref().is_some_and(|s| {
                s.panes.iter().any(|p| {
                    p.pane_id == context.pane_id && p.workspace_id == context.workspace.workspace_id
                })
            })
    }

    fn pane_mission_action_available(&self, method: &str) -> bool {
        self.endpoints
            .iter()
            .find(|e| e.endpoint_id == self.active_endpoint_id)
            .is_some_and(|e| {
                e.organization_supported
                    && e.methods
                        .as_ref()
                        .is_some_and(|m| m.contains(method) && m.contains("organization.get"))
            })
    }

    pub(super) fn open_pane_mission_picker(
        &mut self,
        context: PaneMissionMenuContext,
        outcome: &mut ClientShellInput,
    ) {
        if !self.pane_mission_context_valid(&context) {
            self.stale_pane_mission(outcome);
            return;
        }
        if !self.pane_mission_action_available("mission.assign_pane") {
            outcome.repaint |= self.push_endpoint_notice(
                ClientEndpointNoticeKind::Unsupported,
                "mission.assign_pane",
                "Action unavailable",
                "This server does not support pane mission assignment.",
            );
            return;
        }
        let mut missions = self
            .endpoints
            .iter()
            .find(|e| e.endpoint_id == self.active_endpoint_id)
            .and_then(|e| e.organization.as_ref())
            .map(|c| c.organization.missions.clone())
            .unwrap_or_default();
        if missions.is_empty() {
            outcome.repaint |= self.push_endpoint_notice(
                ClientEndpointNoticeKind::Rejected,
                "mission.no_missions",
                "Create a mission first",
                "Use Missions… in the menu to create a destination.",
            );
            return;
        }
        missions.sort_by_key(|m| m.order);
        self.overlay = Some(ClientShellOverlay::ContextMenu(ClientContextMenuOverlay {
            target: ClientContextMenuTarget::PaneMissionPicker { context, missions },
            x: self.hits.agent_body.x,
            y: self.hits.agent_body.y,
            highlighted: 0,
        }));
    }

    pub(super) fn submit_pane_mission(
        &mut self,
        context: PaneMissionMenuContext,
        mission: Option<crate::organization::MissionId>,
        outcome: &mut ClientShellInput,
    ) {
        if !self.pane_mission_context_valid(&context) {
            self.stale_pane_mission(outcome);
            return;
        }
        if let Some(id) = &mission {
            if !self
                .endpoints
                .iter()
                .find(|e| e.endpoint_id == self.active_endpoint_id)
                .and_then(|e| e.organization.as_ref())
                .is_some_and(|c| c.organization.missions.iter().any(|m| &m.id == id))
            {
                self.stale_pane_mission(outcome);
                return;
            }
        }
        let method = match mission {
            Some(mission_id) => crate::api::schema::Method::MissionAssignPane(
                crate::api::schema::MissionAssignPaneParams {
                    pane_id: context.pane_id,
                    mission_id,
                },
            ),
            None => crate::api::schema::Method::MissionClearPaneOverride(
                crate::api::schema::MissionClearPaneOverrideParams {
                    pane_id: context.pane_id,
                },
            ),
        };
        self.push_endpoint_method(method, outcome);
    }

    fn stale_pane_mission(&mut self, outcome: &mut ClientShellInput) {
        outcome.repaint |= self.push_endpoint_notice(
            ClientEndpointNoticeKind::Rejected,
            "mission.stale",
            "Mission target unavailable",
            "The selected pane, mission or connection is no longer available.",
        );
    }
}
