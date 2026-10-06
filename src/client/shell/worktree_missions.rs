//! Mission choice in the native worktree form; all identities remain client captures.
use super::*;

#[derive(Clone, Debug)]
pub(super) struct MissionWorktreeIntent {
    endpoint_id: ClientEndpointId,
    boot_id: String,
    generation: Option<u64>,
    pub(super) mission: crate::organization::Mission,
}

impl ClientShellState {
    pub(super) fn worktree_mission_notice(
        &mut self,
        message: &str,
        outcome: &mut ClientShellInput,
    ) {
        outcome.repaint |= self.push_endpoint_notice(
            ClientEndpointNoticeKind::Unavailable,
            "worktree.create_in_mission",
            "Target unavailable",
            message,
        );
    }
    pub(super) fn mission_worktree_intent_valid(&self, intent: &MissionWorktreeIntent) -> bool {
        self.worktree_missions_available()
            && intent.endpoint_id == self.active_endpoint_id
            && self.endpoints.iter().any(|e| {
                e.endpoint_id == intent.endpoint_id
                    && e.snapshot_generation == intent.generation
                    && e.snapshot
                        .as_ref()
                        .is_some_and(|s| s.boot_id == intent.boot_id)
                    && e.organization.as_ref().is_some_and(|c| {
                        c.organization
                            .missions
                            .iter()
                            .any(|m| m.id == intent.mission.id)
                    })
            })
    }

    pub(super) fn begin_mission_worktree(
        &mut self,
        mission_id: crate::organization::MissionId,
        member_workspace: Option<String>,
        outcome: &mut ClientShellInput,
    ) {
        if !self.worktree_missions_available() {
            outcome.repaint |= self.push_endpoint_notice(
                ClientEndpointNoticeKind::Unsupported,
                "worktree.create_in_mission",
                "Action unavailable",
                "This server does not support creating worktrees in missions.",
            );
            return;
        }
        let Some(endpoint) = self
            .endpoints
            .iter()
            .find(|e| e.endpoint_id == self.active_endpoint_id)
        else {
            return;
        };
        let Some(snapshot) = endpoint.snapshot.as_deref() else {
            return;
        };
        let Some(mission) = endpoint
            .organization
            .as_ref()
            .and_then(|c| c.organization.missions.iter().find(|m| m.id == mission_id))
            .cloned()
        else {
            self.worktree_mission_notice("The selected mission no longer exists.", outcome);
            return;
        };
        let intent = MissionWorktreeIntent {
            endpoint_id: endpoint.endpoint_id.clone(),
            boot_id: snapshot.boot_id.clone(),
            generation: endpoint.snapshot_generation,
            mission,
        };
        if let Some(member) = member_workspace {
            let parent = snapshot
                .workspaces
                .iter()
                .find(|w| w.workspace_id == member)
                .and_then(|w| match &w.worktree {
                    Some(worktree) if worktree.is_linked_worktree => {
                        snapshot.workspaces.iter().find(|candidate| {
                            candidate
                                .worktree
                                .as_ref()
                                .is_some_and(|c| c.key == worktree.key && !c.is_linked_worktree)
                        })
                    }
                    _ => Some(w),
                });
            let source =
                parent.and_then(|w| self.navigation_target(&intent.endpoint_id, &w.workspace_id));
            if let Some(source) = source {
                self.prepare_mission_worktree(source, intent, outcome);
            } else {
                self.worktree_mission_notice(
                    "The selected member's repository parent is no longer open.",
                    outcome,
                );
            }
            outcome.repaint = true;
            return;
        }
        let sources = snapshot
            .workspaces
            .iter()
            .filter(|w| {
                (w.branch.is_some() || w.worktree.is_some())
                    && !w.worktree.as_ref().is_some_and(|t| t.is_linked_worktree)
            })
            .filter_map(|w| {
                self.navigation_target(&intent.endpoint_id, &w.workspace_id)
                    .map(|target| {
                        (
                            target,
                            format!("{} ({}) · {}", w.label, w.workspace_id, w.new_workspace_cwd),
                        )
                    })
            })
            .collect::<Vec<_>>();
        if sources.is_empty() {
            self.worktree_mission_notice(
                "Open a repository parent workspace before creating a mission worktree.",
                outcome,
            );
            return;
        }
        self.overlay = Some(ClientShellOverlay::ContextMenu(ClientContextMenuOverlay {
            target: ClientContextMenuTarget::MissionWorktreeSources { intent, sources },
            x: self.hits.workspace_body.x,
            y: self.hits.workspace_body.y,
            highlighted: 0,
        }));
        outcome.repaint = true;
    }

    pub(super) fn prepare_mission_worktree(
        &mut self,
        source: WorkspaceNavigationTarget,
        intent: MissionWorktreeIntent,
        outcome: &mut ClientShellInput,
    ) {
        if !self.mission_worktree_intent_valid(&intent) || !self.navigation_target_valid(&source) {
            self.worktree_mission_notice(
                "The selected source or mission connection is no longer available.",
                outcome,
            );
            outcome.repaint = true;
            return;
        }
        self.push_endpoint_method_with_kind(
            crate::api::schema::Method::WorktreeList(crate::api::schema::WorktreeListParams {
                workspace_id: Some(source.workspace_id.clone()),
                cwd: None,
                trust_repository: false,
            }),
            PendingEndpointKind::PrepareMissionWorktreeCreate { source, intent },
            outcome,
        );
    }

    pub(super) fn worktree_missions_available(&self) -> bool {
        self.endpoints.iter().any(|e| {
            e.endpoint_id == self.active_endpoint_id
                && e.status == ClientEndpointStatus::Online
                && e.organization_supported
                && e.organization.is_some()
                && e.methods
                    .as_ref()
                    .is_some_and(|m| m.contains("worktree.create_in_mission"))
        })
    }

    pub(super) fn open_worktree_mission_picker(&mut self, outcome: &mut ClientShellInput) {
        if !self.worktree_missions_available() {
            return;
        }
        let Some(ClientShellOverlay::WorktreeCreate(create)) = self.overlay.take() else {
            return;
        };
        if create.creating {
            self.overlay = Some(ClientShellOverlay::WorktreeCreate(create));
            return;
        }
        let mut missions = self
            .endpoints
            .iter()
            .find(|e| e.endpoint_id == self.active_endpoint_id)
            .and_then(|e| e.organization.as_ref())
            .map(|c| c.organization.missions.clone())
            .unwrap_or_default();
        missions.sort_by_key(|m| m.order);
        self.overlay = Some(ClientShellOverlay::ContextMenu(ClientContextMenuOverlay {
            target: ClientContextMenuTarget::WorktreeMissionPicker {
                create: Box::new(create),
                missions,
            },
            x: self.hits.overlay_clear.x,
            y: self.hits.overlay_clear.y,
            highlighted: 0,
        }));
        outcome.repaint = true;
    }

    pub(super) fn cancel_worktree_mission_picker(&mut self) {
        self.overlay = match self.overlay.take() {
            Some(ClientShellOverlay::ContextMenu(ClientContextMenuOverlay {
                target: ClientContextMenuTarget::WorktreeMissionPicker { create, .. },
                ..
            })) => Some(ClientShellOverlay::WorktreeCreate(*create)),
            _ => None,
        };
    }

    pub(super) fn validate_worktree_create_intent(
        &mut self,
        outcome: &mut ClientShellInput,
    ) -> bool {
        let Some(ClientShellOverlay::WorktreeCreate(create)) = self.overlay.as_ref() else {
            return false;
        };
        let source_valid = create.source_capture.as_ref().is_none_or(|target| {
            target.endpoint_id == self.active_endpoint_id && self.navigation_target_valid(target)
        });
        let mission_valid = create.mission.as_ref().is_none_or(|mission| {
            self.worktree_missions_available()
                && self
                    .endpoints
                    .iter()
                    .find(|e| e.endpoint_id == self.active_endpoint_id)
                    .and_then(|e| e.organization.as_ref())
                    .is_some_and(|c| c.organization.missions.iter().any(|m| m.id == mission.id))
        });
        if source_valid && mission_valid {
            return true;
        }
        outcome.repaint |= self.push_endpoint_notice(
            ClientEndpointNoticeKind::Rejected,
            "worktree.create_in_mission",
            "Target unavailable",
            "The selected source or mission is no longer available. Choose a current target.",
        );
        false
    }
}
