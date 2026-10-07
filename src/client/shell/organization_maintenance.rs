//! Native metadata maintenance. Captures stay scoped to one endpoint incarnation.
use super::*;
use crate::organization::{CollectionId, MissionId};

#[derive(Clone, Debug)]
pub(super) enum OrganizationSubject {
    Collection(CollectionId),
    Mission(MissionId),
    Tab {
        tab_id: String,
        mission_id: MissionId,
    },
}

#[derive(Clone, Debug)]
pub(super) struct OrganizationCapture {
    pub(super) endpoint_id: ClientEndpointId,
    pub(super) boot_id: String,
    pub(super) generation: Option<u64>,
    pub(super) subject: OrganizationSubject,
}

impl ClientShellState {
    pub(super) fn organization_capture_valid(&self, capture: &OrganizationCapture) -> bool {
        capture.endpoint_id == self.active_endpoint_id
            && self
                .endpoints
                .iter()
                .find(|e| e.endpoint_id == capture.endpoint_id)
                .is_some_and(|e| {
                    e.status == ClientEndpointStatus::Online
                        && e.snapshot_generation == capture.generation
                        && e.snapshot
                            .as_ref()
                            .is_some_and(|s| s.boot_id == capture.boot_id)
                        && e.organization
                            .as_ref()
                            .is_some_and(|c| match &capture.subject {
                                OrganizationSubject::Collection(id) => {
                                    c.organization.collections.iter().any(|c| &c.id == id)
                                }
                                OrganizationSubject::Mission(id) => {
                                    c.organization.missions.iter().any(|m| &m.id == id)
                                }
                                OrganizationSubject::Tab { tab_id, mission_id } => {
                                    e.snapshot
                                        .as_ref()
                                        .is_some_and(|s| s.tabs.iter().any(|t| &t.tab_id == tab_id))
                                        && c.organization.mission_for(
                                            &crate::organization::MissionTarget::Tab {
                                                tab_id: tab_id.clone(),
                                            },
                                        ) == Some(mission_id)
                                }
                            })
                })
    }
    pub(super) fn maintenance_stale(&mut self, outcome: &mut ClientShellInput) {
        outcome.repaint |= self.push_endpoint_notice(
            ClientEndpointNoticeKind::Rejected,
            "organization.stale",
            "Action unavailable",
            "The selected organization or connection changed. Select it again.",
        );
    }
    pub(super) fn open_collection_rename(
        &mut self,
        capture: OrganizationCapture,
        outcome: &mut ClientShellInput,
    ) {
        if !self.ensure_maintenance_method(&capture, "collection.rename", outcome) {
            return;
        }
        let OrganizationSubject::Collection(collection_id) = &capture.subject else {
            return;
        };
        let Some(name) = self
            .endpoints
            .iter()
            .find(|e| e.endpoint_id == capture.endpoint_id)
            .and_then(|e| e.organization.as_ref())
            .and_then(|c| {
                c.organization
                    .collections
                    .iter()
                    .find(|c| &c.id == collection_id)
            })
            .map(|c| c.name.as_str().to_owned())
        else {
            return;
        };
        self.overlay = Some(ClientShellOverlay::Rename(ClientRenameOverlay {
            title: "rename collection",
            input: TextEditor::new(&name, true),
            target: ClientRenameTarget::Collection(capture),
        }));
        outcome.repaint = true;
    }
}

impl ClientShellState {
    pub(super) fn move_organization_collection(
        &mut self,
        capture: OrganizationCapture,
        later: bool,
        outcome: &mut ClientShellInput,
    ) {
        if !self.ensure_maintenance_method(&capture, "collection.move", outcome) {
            return;
        }
        let OrganizationSubject::Collection(collection_id) = &capture.subject else {
            return;
        };
        let Some(catalog) = self
            .endpoints
            .iter()
            .find(|e| e.endpoint_id == capture.endpoint_id)
            .and_then(|e| e.organization.as_ref())
        else {
            return;
        };
        let Some(collection) = catalog
            .organization
            .collections
            .iter()
            .find(|c| &c.id == collection_id)
        else {
            return;
        };
        let to = if later {
            collection.order.checked_add(1)
        } else {
            collection.order.checked_sub(1)
        };
        let Some(to_index) = to.filter(|i| *i < catalog.organization.collections.len() as u64)
        else {
            outcome.repaint |= self.push_endpoint_notice(
                ClientEndpointNoticeKind::Rejected,
                "organization.order",
                "Order unchanged",
                "This collection is already at the requested end.",
            );
            return;
        };
        self.push_endpoint_method(
            crate::api::schema::Method::CollectionMove(crate::api::schema::CollectionMoveParams {
                collection_id: collection_id.clone(),
                to_index,
            }),
            outcome,
        );
    }
}

impl ClientShellState {
    pub(super) fn confirm_collection_delete(
        &mut self,
        capture: OrganizationCapture,
        outcome: &mut ClientShellInput,
    ) {
        if !self.ensure_maintenance_method(&capture, "collection.delete", outcome) {
            return;
        }
        let OrganizationSubject::Collection(id) = &capture.subject else {
            return;
        };
        self.overlay = Some(ClientShellOverlay::ConfirmClose(
            ClientConfirmCloseOverlay {
                workspace_id: String::new(),
                close_group: false,
                tab_target: None,
                title: format!("Delete collection {}?", id.0),
                detail: "Workspaces stay open; families move to Uncollected.".into(),
                organization: Some(capture),
            },
        ));
        outcome.repaint = true;
    }
}

impl ClientShellState {
    pub(super) fn open_mission_definition_menu(
        &mut self,
        capture: OrganizationCapture,
        x: u16,
        y: u16,
        outcome: &mut ClientShellInput,
    ) {
        if !self.organization_capture_valid(&capture) {
            self.maintenance_stale(outcome);
            return;
        }
        self.overlay = Some(ClientShellOverlay::ContextMenu(ClientContextMenuOverlay {
            target: ClientContextMenuTarget::MissionMaintenance { capture },
            x,
            y,
            highlighted: 0,
        }));
        outcome.repaint = true;
    }
    pub(super) fn open_mission_rename(
        &mut self,
        capture: OrganizationCapture,
        outcome: &mut ClientShellInput,
    ) {
        if !self.ensure_maintenance_method(&capture, "mission.rename", outcome) {
            return;
        }
        let OrganizationSubject::Mission(mission_id) = &capture.subject else {
            return;
        };
        let Some(name) = self
            .endpoints
            .iter()
            .find(|e| e.endpoint_id == capture.endpoint_id)
            .and_then(|e| e.organization.as_ref())
            .and_then(|c| c.organization.missions.iter().find(|m| &m.id == mission_id))
            .map(|m| m.name.as_str().to_owned())
        else {
            return;
        };
        self.overlay = Some(ClientShellOverlay::Rename(ClientRenameOverlay {
            title: "rename mission",
            input: TextEditor::new(&name, true),
            target: ClientRenameTarget::Mission(capture),
        }));
        outcome.repaint = true;
    }
}

impl ClientShellState {
    pub(super) fn open_mission_objective(
        &mut self,
        capture: OrganizationCapture,
        outcome: &mut ClientShellInput,
    ) {
        if !self.ensure_maintenance_method(&capture, "mission.set_objective", outcome) {
            return;
        }
        let OrganizationSubject::Mission(mission_id) = &capture.subject else {
            return;
        };
        let Some(objective) = self
            .endpoints
            .iter()
            .find(|e| e.endpoint_id == capture.endpoint_id)
            .and_then(|e| e.organization.as_ref())
            .and_then(|c| c.organization.missions.iter().find(|m| &m.id == mission_id))
            .map(|m| m.objective.clone().unwrap_or_default())
        else {
            return;
        };
        self.overlay = Some(ClientShellOverlay::Rename(ClientRenameOverlay {
            title: "edit objective (empty clears)",
            input: TextEditor::new(&objective, true),
            target: ClientRenameTarget::MissionObjective(capture),
        }));
        outcome.repaint = true;
    }
}

impl ClientShellState {
    pub(super) fn move_organization_mission(
        &mut self,
        capture: OrganizationCapture,
        later: bool,
        outcome: &mut ClientShellInput,
    ) {
        if !self.ensure_maintenance_method(&capture, "mission.move", outcome) {
            return;
        }
        let OrganizationSubject::Mission(mission_id) = &capture.subject else {
            return;
        };
        let Some(catalog) = self
            .endpoints
            .iter()
            .find(|e| e.endpoint_id == capture.endpoint_id)
            .and_then(|e| e.organization.as_ref())
        else {
            return;
        };
        let Some(mission) = catalog
            .organization
            .missions
            .iter()
            .find(|m| &m.id == mission_id)
        else {
            return;
        };
        let to = if later {
            mission.order.checked_add(1)
        } else {
            mission.order.checked_sub(1)
        };
        let Some(to_index) = to.filter(|i| *i < catalog.organization.missions.len() as u64) else {
            outcome.repaint |= self.push_endpoint_notice(
                ClientEndpointNoticeKind::Rejected,
                "organization.order",
                "Order unchanged",
                "This mission is already at the requested end.",
            );
            return;
        };
        self.push_endpoint_method(
            crate::api::schema::Method::MissionMove(crate::api::schema::MissionMoveParams {
                mission_id: mission_id.clone(),
                to_index,
            }),
            outcome,
        );
    }
    pub(super) fn confirm_mission_delete(
        &mut self,
        capture: OrganizationCapture,
        outcome: &mut ClientShellInput,
    ) {
        if !self.ensure_maintenance_method(&capture, "mission.delete", outcome) {
            return;
        }
        let OrganizationSubject::Mission(id) = &capture.subject else {
            return;
        };
        self.overlay = Some(ClientShellOverlay::ConfirmClose(
            ClientConfirmCloseOverlay {
                workspace_id: String::new(),
                close_group: false,
                tab_target: None,
                title: format!("Delete mission {}?", id.0),
                detail: "Assignments cleared; terminals stay open.".into(),
                organization: Some(capture),
            },
        ));
        outcome.repaint = true;
    }
    pub(super) fn submit_organization_delete(
        &mut self,
        capture: OrganizationCapture,
        outcome: &mut ClientShellInput,
    ) {
        if !self.organization_capture_valid(&capture) {
            self.maintenance_stale(outcome);
            return;
        }
        let method = match capture.subject {
            OrganizationSubject::Collection(collection_id) => {
                crate::api::schema::Method::CollectionDelete(
                    crate::api::schema::CollectionDeleteParams { collection_id },
                )
            }
            OrganizationSubject::Mission(mission_id) => {
                crate::api::schema::Method::MissionDelete(crate::api::schema::MissionDeleteParams {
                    mission_id,
                })
            }
            OrganizationSubject::Tab { tab_id, mission_id } => {
                crate::api::schema::Method::MissionUnassign(
                    crate::api::schema::MissionUnassignParams {
                        target: crate::organization::MissionTarget::Tab { tab_id },
                        mission_id,
                    },
                )
            }
        };
        self.push_endpoint_method(method, outcome);
    }
}

impl ClientShellState {
    pub(super) fn confirm_tab_mission_removal(
        &mut self,
        capture: OrganizationCapture,
        outcome: &mut ClientShellInput,
    ) {
        if !self.ensure_maintenance_method(&capture, "mission.unassign", outcome) {
            return;
        }
        let OrganizationSubject::Tab { tab_id, .. } = &capture.subject else {
            return;
        };
        self.overlay = Some(ClientShellOverlay::ConfirmClose(
            ClientConfirmCloseOverlay {
                workspace_id: String::new(),
                close_group: false,
                tab_target: None,
                title: format!("Remove mission from tab {tab_id}?"),
                detail: "All inherited panes removed; overrides stay. Terminals stay open.".into(),
                organization: Some(capture),
            },
        ));
        outcome.repaint = true;
    }
}

impl ClientShellState {
    pub(super) fn capture_organization_subject(
        &self,
        subject: OrganizationSubject,
    ) -> Option<OrganizationCapture> {
        let endpoint = self
            .endpoints
            .iter()
            .find(|e| e.endpoint_id == self.active_endpoint_id)?;
        Some(OrganizationCapture {
            endpoint_id: endpoint.endpoint_id.clone(),
            boot_id: endpoint.snapshot.as_ref()?.boot_id.clone(),
            generation: endpoint.snapshot_generation,
            subject,
        })
    }
}

impl ClientShellState {
    fn ensure_maintenance_method(
        &mut self,
        capture: &OrganizationCapture,
        method: &str,
        outcome: &mut ClientShellInput,
    ) -> bool {
        if !self.organization_capture_valid(capture) {
            self.maintenance_stale(outcome);
            return false;
        }
        let available = self
            .endpoints
            .iter()
            .find(|e| e.endpoint_id == capture.endpoint_id)
            .is_some_and(|e| {
                e.organization_supported && e.methods.as_ref().is_some_and(|m| m.contains(method))
            });
        if !available {
            outcome.repaint|=self.push_endpoint_notice(ClientEndpointNoticeKind::Unsupported,method,"Action unavailable",format!("This server does not support {method}. Other terminal actions remain available."));
        }
        available
    }
}

impl ClientShellState {
    pub(super) fn organization_information(
        &mut self,
        collection: bool,
        outcome: &mut ClientShellInput,
    ) {
        let body = if collection {
            "Managed repository families recover their collection on reopen. Closed standalone references are removed. Empty collections persist; assign an existing family from its workspace menu."
        } else {
            "Membership uses public IDs. Restore removes unavailable targets and creates no replacement terminals. Empty missions persist. Assign existing tabs or panes from their menus."
        };
        outcome.repaint |= self.push_endpoint_notice(
            ClientEndpointNoticeKind::Rejected,
            "organization.information",
            "Membership and restore",
            body,
        );
    }
}
