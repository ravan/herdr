use super::*;

pub(super) fn pending_label(method: &str) -> Option<&'static str> {
    match method {
        "collection.set_hibernating" => Some(" updating Hibernate…"),
        "mission.create" => Some(" creating mission…"),
        "mission.assign_pane" => Some(" assigning pane mission…"),
        "mission.clear_pane_override" => Some(" clearing pane override…"),
        "mission.assign" => Some(" assigning mission…"),
        "collection.create" => Some(" creating collection…"),
        "collection.assign_family" => Some(" moving family…"),
        _ => None,
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct PaneMissionMembership {
    pub(super) mission_id: Option<crate::organization::MissionId>,
    pub(super) label: Option<String>,
    pub(super) order: u64,
    pub(super) explicit: bool,
    pub(super) inherited_label: Option<String>,
    pub(super) parked: bool,
}

#[derive(Clone, Debug)]
pub(super) enum CollectionRow {
    Hibernate,
    Collection(usize),
    Uncollected,
    Workspace(WorkspaceEntry),
}

pub(super) fn collection_workspace_entries(
    endpoint: &ClientShellEndpoint,
) -> Option<Vec<WorkspaceEntry>> {
    (endpoint.organization_supported && endpoint.organization.is_some()).then(|| {
        endpoint
            .organization_rows
            .iter()
            .filter_map(|row| match row {
                CollectionRow::Workspace(entry) => Some(*entry),
                _ => None,
            })
            .collect()
    })
}

impl ClientShellState {
    pub(super) fn open_mission_picker(
        &mut self,
        tab_id: String,
        workspace: WorkspaceNavigationTarget,
        outcome: &mut ClientShellInput,
    ) {
        if !self.missions_available() {
            outcome.repaint |= self.push_endpoint_notice(
                ClientEndpointNoticeKind::Unsupported,
                "missions",
                "Action unavailable",
                "This server does not support missions.",
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
        missions.sort_by_key(|mission| mission.order);
        self.overlay = Some(ClientShellOverlay::ContextMenu(ClientContextMenuOverlay {
            target: ClientContextMenuTarget::MissionPicker {
                workspace,
                tab_id,
                missions,
            },
            x: self.hits.workspace_body.x,
            y: self.hits.workspace_body.y,
            highlighted: 0,
        }));
    }
    pub(super) fn missions_available(&self) -> bool {
        self.endpoints
            .iter()
            .find(|e| e.endpoint_id == self.active_endpoint_id)
            .is_some_and(|e| {
                e.organization_supported
                    && e.methods.as_ref().is_some_and(|methods| {
                        ["organization.get", "mission.create", "mission.assign"]
                            .iter()
                            .all(|method| methods.contains(*method))
                    })
            })
    }
    pub(super) fn open_missions(&mut self, outcome: &mut ClientShellInput) {
        if !self.missions_available() {
            outcome.repaint |= self.push_endpoint_notice(
                ClientEndpointNoticeKind::Unsupported,
                "missions",
                "Action unavailable",
                "This server does not support missions.",
            );
            return;
        }
        let missions = self
            .endpoints
            .iter()
            .find(|e| e.endpoint_id == self.active_endpoint_id)
            .and_then(|e| e.organization.as_ref())
            .map(|catalog| {
                let mut missions = catalog
                    .organization
                    .missions
                    .iter()
                    .map(|mission| {
                        (
                            mission.clone(),
                            catalog
                                .organization
                                .mission_assignments
                                .iter()
                                .filter(|a| a.mission_id == mission.id)
                                .count(),
                        )
                    })
                    .collect::<Vec<_>>();
                missions.sort_by_key(|(mission, _)| mission.order);
                missions
            })
            .unwrap_or_default();
        self.overlay = Some(ClientShellOverlay::ContextMenu(ClientContextMenuOverlay {
            target: ClientContextMenuTarget::Missions { missions },
            x: self.hits.workspace_body.x,
            y: self.hits.workspace_body.y,
            highlighted: 0,
        }));
    }
    pub(super) fn open_new_mission(&mut self, outcome: &mut ClientShellInput) {
        if !self.missions_available() {
            outcome.repaint |= self.push_endpoint_notice(
                ClientEndpointNoticeKind::Unsupported,
                "missions",
                "Action unavailable",
                "This server does not support missions.",
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
        let Some(snapshot) = endpoint.snapshot.as_ref() else {
            return;
        };
        self.overlay = Some(ClientShellOverlay::Rename(ClientRenameOverlay {
            title: "new mission",
            input: TextEditor::default(),
            target: ClientRenameTarget::NewMission {
                endpoint_id: endpoint.endpoint_id.clone(),
                boot_id: snapshot.boot_id.clone(),
                generation: endpoint.snapshot_generation,
            },
        }));
    }
    pub(super) fn restore_organization_notice(&mut self) {
        let boot = self
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.boot_id.as_str());
        if self
            .organization_notices
            .get(&self.active_endpoint_id)
            .is_some_and(|notice| {
                // A failed overview jump is client history, not a server fact.
                notice.key.code != "organization.mission_control.target"
                    && Some(notice.key.boot_id.as_str()) != boot
            })
        {
            self.organization_notices.remove(&self.active_endpoint_id);
        }
        if self.visible_endpoint_notice.is_none() {
            self.visible_endpoint_notice = self
                .organization_notices
                .get(&self.active_endpoint_id)
                .cloned();
        }
    }
    pub(super) fn open_collection_picker(
        &mut self,
        workspace_id: String,
        outcome: &mut ClientShellInput,
    ) {
        if !self.organization_available() {
            outcome.repaint |= self.push_endpoint_notice(
                ClientEndpointNoticeKind::Unsupported,
                "organization",
                "Action unavailable",
                "This server does not support collections.",
            );
            return;
        }
        let Some(workspace) = self.navigation_target(&self.active_endpoint_id, &workspace_id)
        else {
            return;
        };
        let Some(target) = self.snapshot.as_ref().and_then(|snapshot| {
            snapshot
                .workspaces
                .iter()
                .find(|target| target.workspace_id == workspace_id)
        }) else {
            return;
        };
        let family_id = match target.worktree.as_ref() {
            Some(worktree) => crate::organization::FamilyId::Managed {
                key: worktree.key.clone(),
            },
            None => crate::organization::FamilyId::Standalone { workspace_id },
        };
        let collections = self
            .endpoints
            .iter()
            .find(|endpoint| endpoint.endpoint_id == self.active_endpoint_id)
            .and_then(|endpoint| endpoint.organization.as_ref())
            .map(|catalog| catalog.organization.collections.clone())
            .unwrap_or_default();
        if collections.is_empty() {
            outcome.repaint |= self.push_endpoint_notice(
                ClientEndpointNoticeKind::Rejected,
                "no_collections",
                "Create a collection first",
                "Use New collection in the menu to create a destination.",
            );
            return;
        }
        self.overlay = Some(ClientShellOverlay::ContextMenu(ClientContextMenuOverlay {
            target: ClientContextMenuTarget::CollectionPicker {
                workspace,
                family_id,
                collections,
            },
            x: self.hits.workspace_body.x,
            y: self.hits.workspace_body.y,
            highlighted: 0,
        }));
    }
    pub(crate) fn set_endpoint_organization_supported(
        &mut self,
        endpoint_id: &ClientEndpointId,
        supported: bool,
    ) {
        if let Some(endpoint) = self
            .endpoints
            .iter_mut()
            .find(|endpoint| &endpoint.endpoint_id == endpoint_id)
        {
            endpoint.organization_supported = supported;
            if !supported {
                endpoint.organization = None;
                endpoint.organization_generation = None;
                endpoint.pending_organization = None;
                endpoint.organization_rows.clear();
                endpoint.mission_labels.clear();
                endpoint.pane_missions.clear();
            }
        }
    }

    pub(super) fn organization_available(&self) -> bool {
        self.endpoints
            .iter()
            .find(|endpoint| endpoint.endpoint_id == self.active_endpoint_id)
            .is_some_and(|endpoint| {
                endpoint.organization_supported
                    && endpoint.methods.as_ref().is_some_and(|methods| {
                        [
                            "organization.get",
                            "collection.create",
                            "collection.assign_family",
                        ]
                        .iter()
                        .all(|method| methods.contains(*method))
                    })
            })
    }

    pub(super) fn open_new_collection(&mut self, outcome: &mut ClientShellInput) {
        if !self.organization_available() {
            outcome.repaint |= self.push_endpoint_notice(
                ClientEndpointNoticeKind::Unsupported,
                "organization",
                "Action unavailable",
                "This server does not support collections.",
            );
            return;
        }
        self.overlay = Some(ClientShellOverlay::Rename(ClientRenameOverlay {
            title: "new collection",
            input: TextEditor::default(),
            target: ClientRenameTarget::NewCollection,
        }));
    }

    pub(crate) fn set_endpoint_organization_for_generation(
        &mut self,
        endpoint_id: &ClientEndpointId,
        generation: u64,
        catalog: crate::protocol::endpoint::EndpointOrganizationCatalog,
    ) -> bool {
        let Some(endpoint) = self
            .endpoints
            .iter_mut()
            .find(|endpoint| &endpoint.endpoint_id == endpoint_id)
        else {
            return false;
        };
        if !endpoint.organization_supported {
            return false;
        }
        if endpoint
            .snapshot_generation
            .is_some_and(|current| generation < current)
        {
            return false;
        }
        if endpoint.snapshot_generation != Some(generation) || endpoint.snapshot.is_none() {
            if endpoint.pending_organization.as_ref().is_some_and(
                |(pending_generation, pending)| {
                    *pending_generation > generation
                        || (*pending_generation == generation
                            && pending.boot_id == catalog.boot_id
                            && pending.organization.revision >= catalog.organization.revision)
                },
            ) {
                return false;
            }
            endpoint.pending_organization = Some((generation, catalog));
            return false;
        }
        if endpoint
            .snapshot
            .as_ref()
            .is_none_or(|snapshot| snapshot.boot_id != catalog.boot_id)
        {
            return false;
        }
        if endpoint
            .organization
            .as_ref()
            .is_some_and(|current| current.organization.revision >= catalog.organization.revision)
        {
            return false;
        }
        endpoint.organization_generation = Some(generation);
        endpoint.organization = Some(catalog);
        self.rebuild_organization_rows(endpoint_id);
        true
    }

    pub(super) fn reconcile_organization(
        &mut self,
        endpoint_id: &ClientEndpointId,
        mut projection_changed: bool,
    ) {
        let Some(endpoint) = self
            .endpoints
            .iter_mut()
            .find(|endpoint| &endpoint.endpoint_id == endpoint_id)
        else {
            return;
        };
        if let Some((generation, catalog)) =
            endpoint
                .pending_organization
                .take()
                .filter(|(generation, catalog)| {
                    Some(*generation) == endpoint.snapshot_generation
                        && endpoint
                            .snapshot
                            .as_ref()
                            .is_some_and(|snapshot| snapshot.boot_id == catalog.boot_id)
                })
        {
            endpoint.organization_generation = Some(generation);
            endpoint.organization = Some(catalog);
            projection_changed = true;
        }
        if endpoint.organization_generation != endpoint.snapshot_generation
            || endpoint.organization.as_ref().is_some_and(|catalog| {
                endpoint
                    .snapshot
                    .as_ref()
                    .is_none_or(|snapshot| snapshot.boot_id != catalog.boot_id)
            })
        {
            endpoint.organization = None;
            endpoint.organization_rows.clear();
            endpoint.mission_labels.clear();
            endpoint.pane_missions.clear();
        }
        if projection_changed {
            self.rebuild_organization_rows(endpoint_id);
        }
    }

    pub(super) fn rebuild_organization_rows(&mut self, endpoint_id: &ClientEndpointId) {
        let Some(index) = self
            .endpoints
            .iter()
            .position(|endpoint| &endpoint.endpoint_id == endpoint_id)
        else {
            return;
        };
        let endpoint = &self.endpoints[index];
        let (Some(snapshot), Some(catalog)) =
            (endpoint.snapshot.as_deref(), endpoint.organization.as_ref())
        else {
            return;
        };
        let mission_labels = catalog
            .organization
            .mission_assignments
            .iter()
            .filter_map(|assignment| {
                let crate::organization::MissionTarget::Tab { tab_id } = &assignment.target;
                let mission = catalog
                    .organization
                    .missions
                    .iter()
                    .find(|mission| mission.id == assignment.mission_id)?;
                Some((tab_id.clone(), mission.name.as_str().to_owned()))
            })
            .collect();
        let pane_missions = snapshot
            .panes
            .iter()
            .map(|pane| {
                let organization = &catalog.organization;
                let mission_id = organization
                    .effective_pane_mission(&pane.pane_id, &pane.tab_id)
                    .cloned();
                let mission = mission_id
                    .as_ref()
                    .and_then(|id| organization.missions.iter().find(|m| &m.id == id));
                let inherited = organization
                    .mission_for(&crate::organization::MissionTarget::Tab {
                        tab_id: pane.tab_id.clone(),
                    })
                    .and_then(|id| organization.missions.iter().find(|m| &m.id == id));
                let parked = snapshot
                    .workspaces
                    .iter()
                    .find(|ws| ws.workspace_id == pane.workspace_id)
                    .and_then(|ws| {
                        let family = match &ws.worktree {
                            Some(worktree) => crate::organization::FamilyId::Managed {
                                key: worktree.key.clone(),
                            },
                            None => crate::organization::FamilyId::Standalone {
                                workspace_id: ws.workspace_id.clone(),
                            },
                        };
                        let id = organization.collection_for(&family)?;
                        organization.collections.iter().find(|c| &c.id == id)
                    })
                    .is_some_and(|c| c.hibernating);
                (
                    pane.pane_id.clone(),
                    PaneMissionMembership {
                        mission_id,
                        label: mission.map(|m| m.name.as_str().to_owned()),
                        order: mission.map_or(u64::MAX, |m| m.order),
                        explicit: organization.pane_override(&pane.pane_id).is_some(),
                        inherited_label: inherited.map(|m| m.name.as_str().to_owned()),
                        parked,
                    },
                )
            })
            .collect();
        let empty = HashSet::new();
        let groups = self
            .collapsed_groups_for_endpoint(endpoint_id)
            .unwrap_or(&empty);
        let base = sidebar::workspace_entries(snapshot, groups);
        let assignments = base
            .iter()
            .map(|entry| {
                let workspace = &snapshot.workspaces[entry.index];
                catalog
                    .organization
                    .family_assignments
                    .iter()
                    .find(
                        |assignment| match (&assignment.family_id, &workspace.worktree) {
                            (crate::organization::FamilyId::Managed { key }, Some(worktree)) => {
                                key == &worktree.key
                            }
                            (crate::organization::FamilyId::Standalone { workspace_id }, None) => {
                                workspace_id == &workspace.workspace_id
                            }
                            _ => false,
                        },
                    )
                    .map(|assignment| &assignment.collection_id)
            })
            .collect::<Vec<_>>();
        let mut ordered = (0..catalog.organization.collections.len()).collect::<Vec<_>>();
        ordered.sort_by_key(|index| catalog.organization.collections[*index].order);
        let mut rows = Vec::new();
        let append_collection = |rows: &mut Vec<CollectionRow>, index: usize| {
            rows.push(CollectionRow::Collection(index));
            let id = &catalog.organization.collections[index].id;
            if self
                .collapsed_collections
                .get(endpoint_id)
                .is_some_and(|ids| ids.contains(id))
            {
                return;
            }
            rows.extend(
                base.iter()
                    .zip(&assignments)
                    .filter(|(_, assignment)| **assignment == Some(id))
                    .map(|(entry, _)| CollectionRow::Workspace(*entry)),
            );
        };
        for &index in &ordered {
            if !catalog.organization.collections[index].hibernating {
                append_collection(&mut rows, index);
            }
        }
        rows.push(CollectionRow::Uncollected);
        rows.extend(
            base.iter()
                .zip(&assignments)
                .filter(|(_, assignment)| assignment.is_none())
                .map(|(entry, _)| CollectionRow::Workspace(*entry)),
        );
        rows.push(CollectionRow::Hibernate);
        if self.expanded_hibernate.contains(endpoint_id) {
            for index in ordered {
                if catalog.organization.collections[index].hibernating {
                    append_collection(&mut rows, index);
                }
            }
        }
        self.endpoints[index].pane_missions = pane_missions;
        self.endpoints[index].mission_labels = mission_labels;
        self.endpoints[index].organization_rows = rows;
    }

    /// Explicit navigation reveals parked locations; catalog/focus updates never do this.
    pub(super) fn reveal_organization_target(
        &mut self,
        endpoint_id: &ClientEndpointId,
        target: &ClientEndpointFocusTarget,
        outcome: &mut ClientShellInput,
    ) {
        let location = self
            .endpoints
            .iter()
            .find(|endpoint| &endpoint.endpoint_id == endpoint_id)
            .and_then(|endpoint| {
                let snapshot = endpoint.snapshot.as_ref()?;
                let workspace_id = match target {
                    ClientEndpointFocusTarget::Workspace(id) => id.as_str(),
                    ClientEndpointFocusTarget::Tab(id) => snapshot
                        .tabs
                        .iter()
                        .find(|tab| &tab.tab_id == id)?
                        .workspace_id
                        .as_str(),
                    ClientEndpointFocusTarget::Pane(id) => snapshot
                        .panes
                        .iter()
                        .find(|pane| &pane.pane_id == id)?
                        .workspace_id
                        .as_str(),
                    #[cfg(windows)]
                    ClientEndpointFocusTarget::Notification { pane_id, .. } => snapshot
                        .panes
                        .iter()
                        .find(|pane| &pane.pane_id == pane_id)?
                        .workspace_id
                        .as_str(),
                };
                let workspace = snapshot
                    .workspaces
                    .iter()
                    .find(|workspace| workspace.workspace_id == workspace_id)?;
                let catalog = &endpoint.organization.as_ref()?.organization;
                let family = match &workspace.worktree {
                    Some(worktree) => crate::organization::FamilyId::Managed {
                        key: worktree.key.clone(),
                    },
                    None => crate::organization::FamilyId::Standalone {
                        workspace_id: workspace_id.to_owned(),
                    },
                };
                let id = catalog.collection_for(&family)?;
                let collection = catalog
                    .collections
                    .iter()
                    .find(|collection| &collection.id == id)?;
                Some((
                    collection.id.clone(),
                    collection.hibernating,
                    workspace
                        .worktree
                        .as_ref()
                        .map(|worktree| worktree.key.clone()),
                ))
            });
        let Some((id, hibernating, family_key)) = location else {
            return;
        };
        let mut changed = hibernating && self.expanded_hibernate.insert(endpoint_id.clone());
        if let Some(ids) = self.collapsed_collections.get_mut(endpoint_id) {
            changed |= ids.remove(&id);
        }
        if let Some(key) = family_key {
            let groups = if endpoint_id.is_local() {
                Some(&mut self.collapsed_groups)
            } else {
                self.remote_collapsed_groups.get_mut(endpoint_id)
            };
            if let Some(groups) = groups {
                changed |= groups.remove(&key);
            }
        }
        if changed {
            self.rebuild_organization_rows(endpoint_id);
            self.reveal_focused_workspace = true;
            self.reveal_navigation_workspace = true;
            self.persist_chrome_preferences(outcome);
            outcome.repaint = true;
        }
    }

    pub(super) fn toggle_hibernate(
        &mut self,
        endpoint: &ClientEndpointId,
        outcome: &mut ClientShellInput,
    ) {
        if !self.expanded_hibernate.remove(endpoint) {
            self.expanded_hibernate.insert(endpoint.clone());
        }
        self.rebuild_organization_rows(endpoint);
        self.persist_chrome_preferences(outcome);
        outcome.repaint = true;
    }

    pub(super) fn toggle_collection(
        &mut self,
        endpoint_id: &ClientEndpointId,
        id: crate::organization::CollectionId,
        outcome: &mut ClientShellInput,
    ) {
        let collapsed = self
            .collapsed_collections
            .entry(endpoint_id.clone())
            .or_default();
        if !collapsed.remove(&id) {
            collapsed.insert(id);
        }
        self.rebuild_organization_rows(endpoint_id);
        self.persist_chrome_preferences(outcome);
        outcome.repaint = true;
    }
}
