use super::*;

impl ClientContextMenuOverlay {
    pub(super) fn items(&self) -> Vec<ClientContextMenuItem> {
        use ClientContextMenuAction as Action;

        let item = |label: &str, action| ClientContextMenuItem {
            label: label.to_owned(),
            action,
        };
        let mut items = match &self.target {
            ClientContextMenuTarget::PaneMissionPicker { missions, .. }
            | ClientContextMenuTarget::MissionPicker { missions, .. } => missions
                .iter()
                .enumerate()
                .map(|(index, mission)| ClientContextMenuItem {
                    label: format!("{} ({})", mission.name.as_str(), mission.id.0),
                    action: Action::AssignMission(index),
                })
                .collect(),
            ClientContextMenuTarget::Missions { missions } => {
                let mut rows = vec![item("New mission…", Action::NewMission)];
                rows.extend(
                    missions
                        .iter()
                        .enumerate()
                        .map(|(index, (mission, count))| ClientContextMenuItem {
                            label: format!(
                                "{} ({}) · {} tabs{}",
                                mission.name.as_str(),
                                mission.id.0,
                                count,
                                mission
                                    .objective
                                    .as_ref()
                                    .map(|objective| format!(" · {objective}"))
                                    .unwrap_or_default()
                            ),
                            action: Action::MissionDefinition(index),
                        }),
                );
                rows
            }

            ClientContextMenuTarget::Collection { hibernating, .. } => vec![item(
                if *hibernating {
                    "Hibernate collection"
                } else {
                    "Bring collection back"
                },
                Action::SetHibernating,
            )],
            ClientContextMenuTarget::CollectionPicker { collections, .. } => collections
                .iter()
                .enumerate()
                .map(|(index, collection)| {
                    let duplicate = collections
                        .iter()
                        .filter(|other| other.name == collection.name)
                        .count()
                        > 1;
                    ClientContextMenuItem {
                        label: if duplicate {
                            format!("{} ({})", collection.name.as_str(), collection.id.0)
                        } else {
                            collection.name.as_str().to_owned()
                        },
                        action: Action::AssignCollection(index),
                    }
                })
                .collect(),
            ClientContextMenuTarget::Workspace { is_git: false, .. } => {
                vec![item("Rename", Action::Rename), item("Close", Action::Close)]
            }
            ClientContextMenuTarget::Workspace {
                is_linked_worktree: false,
                has_worktree_children: false,
                ..
            } => vec![
                item("Rename", Action::Rename),
                item("Close", Action::Close),
                item("New worktree", Action::NewWorktree),
                item("Open worktree...", Action::OpenWorktree),
            ],
            ClientContextMenuTarget::Workspace {
                is_linked_worktree: true,
                ..
            } => vec![
                item("Rename", Action::Rename),
                item("Close", Action::Close),
                item("Delete worktree checkout...", Action::RemoveWorktree),
            ],
            ClientContextMenuTarget::Workspace {
                has_worktree_children: true,
                close_group,
                collapsed,
                ..
            } => vec![
                item("Rename", Action::Rename),
                item(
                    if *close_group { "Close group" } else { "Close" },
                    Action::Close,
                ),
                item("New worktree", Action::NewWorktree),
                item("Open worktree...", Action::OpenWorktree),
                item(
                    if *collapsed { "Expand" } else { "Collapse" },
                    Action::ToggleGroup,
                ),
            ],
            ClientContextMenuTarget::Tab { .. } => vec![
                item("New tab", Action::NewTab),
                item("Rename", Action::Rename),
                item("Close", Action::Close),
                item("Add to mission…", Action::AddToMission),
            ],
            ClientContextMenuTarget::Pane {
                source_pane_id,
                has_manual_label,
                right_click_passthrough,
                mission_context,
                ..
            } => {
                let mut items = vec![item("Rename pane", Action::RenamePane)];
                if *has_manual_label {
                    items.push(item("Clear pane name", Action::ClearPaneName));
                }
                if source_pane_id.is_some() {
                    items.push(item("Swap with focused pane", Action::SwapWithFocusedPane));
                }
                if let Some(context) = mission_context {
                    items.push(item(
                        &format!(
                            "Mission: {} ({})",
                            context.membership,
                            if context.explicit {
                                "pane override"
                            } else {
                                "inherits tab"
                            }
                        ),
                        Action::PaneMissionInfo,
                    ));
                    items.push(item("Assign mission…", Action::AssignPaneMission));
                    if context.explicit {
                        items.push(item(
                            &format!("Clear override (inherit {})", context.inherited),
                            Action::ClearPaneMissionOverride,
                        ));
                    }
                }
                items.extend([
                    item("Split right", Action::SplitRight),
                    item("Split down", Action::SplitDown),
                    item("Zoom", Action::Zoom),
                    item(
                        if *right_click_passthrough {
                            "Use Herdr right-click menu"
                        } else {
                            "Send right-clicks to pane"
                        },
                        Action::ToggleRightClickPassthrough,
                    ),
                    item("Close pane", Action::ClosePane),
                ]);
                items
            }
        };
        if matches!(self.target, ClientContextMenuTarget::Workspace { .. }) {
            items.push(item(
                "Move family to collection...",
                Action::MoveFamilyToCollection,
            ));
        }
        items
    }
}

impl ClientShellState {
    pub(super) fn open_collection_context_menu(
        &mut self,
        endpoint_id: ClientEndpointId,
        collection_id: crate::organization::CollectionId,
        x: u16,
        y: u16,
    ) {
        let Some(endpoint) = self
            .endpoints
            .iter()
            .find(|endpoint| endpoint.endpoint_id == endpoint_id)
        else {
            return;
        };
        let (Some(snapshot), Some(catalog)) =
            (endpoint.snapshot.as_ref(), endpoint.organization.as_ref())
        else {
            return;
        };
        let Some(collection) = catalog
            .organization
            .collections
            .iter()
            .find(|collection| collection.id == collection_id)
        else {
            return;
        };
        self.overlay = Some(ClientShellOverlay::ContextMenu(ClientContextMenuOverlay {
            target: ClientContextMenuTarget::Collection {
                endpoint_id,
                boot_id: snapshot.boot_id.clone(),
                generation: endpoint.snapshot_generation,
                collection_id,
                hibernating: !collection.hibernating,
            },
            x,
            y,
            highlighted: 0,
        }));
    }

    pub(super) fn open_workspace_context_menu(&mut self, workspace_id: String, x: u16, y: u16) {
        let Some(snapshot) = self.snapshot.as_deref() else {
            return;
        };
        let Some(workspace) = snapshot
            .workspaces
            .iter()
            .find(|workspace| workspace.workspace_id == workspace_id)
        else {
            return;
        };
        let worktree = workspace.worktree.as_ref();
        let has_worktree_children = worktree.is_some_and(|worktree| {
            !worktree.is_linked_worktree
                && snapshot.workspaces.iter().any(|candidate| {
                    candidate.worktree.as_ref().is_some_and(|candidate| {
                        candidate.key == worktree.key && candidate.is_linked_worktree
                    })
                })
        });
        let close_group = super::sidebar::workspace_close_is_group(snapshot, workspace);
        let collapsed = worktree.is_some_and(|worktree| {
            self.group_is_collapsed(&self.active_endpoint_id, &worktree.key)
        });
        self.overlay = Some(ClientShellOverlay::ContextMenu(ClientContextMenuOverlay {
            target: ClientContextMenuTarget::Workspace {
                workspace_id,
                is_git: worktree.is_some() || workspace.branch.is_some(),
                is_linked_worktree: worktree.is_some_and(|worktree| worktree.is_linked_worktree),
                has_worktree_children,
                close_group,
                collapsed,
            },
            x,
            y,
            highlighted: 0,
        }));
    }

    pub(super) fn open_tab_context_menu(&mut self, tab_id: String, x: u16, y: u16) {
        let Some(tab) = self
            .snapshot
            .as_deref()
            .and_then(|snapshot| snapshot.tabs.iter().find(|tab| tab.tab_id == tab_id))
        else {
            return;
        };
        self.overlay = Some(ClientShellOverlay::ContextMenu(ClientContextMenuOverlay {
            target: ClientContextMenuTarget::Tab {
                tab_id,
                workspace_id: tab.workspace_id.clone(),
                mission_context: self
                    .navigation_target(&self.active_endpoint_id, &tab.workspace_id),
            },
            x,
            y,
            highlighted: 0,
        }));
    }

    pub(super) fn open_pane_context_menu(&mut self, pane_id: String, x: u16, y: u16) {
        let Some(snapshot) = self.snapshot.as_deref() else {
            return;
        };
        let Some(pane) = snapshot.panes.iter().find(|pane| pane.pane_id == pane_id) else {
            return;
        };
        let source_pane_id = snapshot
            .focused_pane_id
            .clone()
            .filter(|focused| focused != &pane_id);
        self.overlay = Some(ClientShellOverlay::ContextMenu(ClientContextMenuOverlay {
            target: ClientContextMenuTarget::Pane {
                mission_context: self.pane_mission_context(&pane_id),
                pane_id,
                workspace_id: pane.workspace_id.clone(),
                source_pane_id,
                has_manual_label: pane.label.is_some(),
                right_click_passthrough: pane.right_click_passthrough,
            },
            x,
            y,
            highlighted: 0,
        }));
    }

    pub(super) fn move_context_menu_selection(&mut self, delta: isize) {
        let Some(ClientShellOverlay::ContextMenu(menu)) = self.overlay.as_mut() else {
            return;
        };
        let item_count = menu.items().len();
        if item_count == 0 {
            return;
        }
        menu.highlighted = (menu.highlighted as isize + delta)
            .clamp(0, item_count.saturating_sub(1) as isize) as usize;
    }

    pub(super) fn activate_context_menu_item(
        &mut self,
        index: usize,
        outcome: &mut ClientShellInput,
    ) {
        let Some(ClientShellOverlay::ContextMenu(menu)) = self.overlay.take() else {
            return;
        };
        let Some(action) = menu.items().get(index).map(|item| item.action) else {
            outcome.repaint = true;
            return;
        };
        match menu.target {
            ClientContextMenuTarget::PaneMissionPicker { context, missions } => {
                if let ClientContextMenuAction::AssignMission(index) = action {
                    if let Some(mission) = missions.get(index) {
                        self.submit_pane_mission(context, Some(mission.id.clone()), outcome);
                    }
                }
            }
            ClientContextMenuTarget::MissionPicker {
                workspace,
                tab_id,
                missions,
            } => {
                if let ClientContextMenuAction::AssignMission(index) = action {
                    if let Some(mission) = missions.get(index) {
                        let valid = self.navigation_target_valid(&workspace)
                            && workspace.endpoint_id == self.active_endpoint_id
                            && self.snapshot.as_ref().is_some_and(|snapshot| {
                                snapshot.tabs.iter().any(|tab| {
                                    tab.tab_id == tab_id
                                        && tab.workspace_id == workspace.workspace_id
                                })
                            })
                            && self
                                .endpoints
                                .iter()
                                .find(|e| e.endpoint_id == workspace.endpoint_id)
                                .and_then(|e| e.organization.as_ref())
                                .is_some_and(|catalog| {
                                    catalog
                                        .organization
                                        .missions
                                        .iter()
                                        .any(|m| m.id == mission.id)
                                });
                        if valid {
                            self.push_endpoint_method(
                                crate::api::schema::Method::MissionAssign(
                                    crate::api::schema::MissionAssignParams {
                                        target: crate::organization::MissionTarget::Tab { tab_id },
                                        mission_id: mission.id.clone(),
                                    },
                                ),
                                outcome,
                            );
                        } else {
                            outcome.repaint |= self.push_endpoint_notice(
                                ClientEndpointNoticeKind::Rejected,
                                "mission.stale",
                                "Mission target unavailable",
                                "The selected tab or mission is no longer available.",
                            );
                        }
                    }
                }
            }

            ClientContextMenuTarget::Missions { missions } => {
                match action {
                    ClientContextMenuAction::NewMission => self.open_new_mission(outcome),
                    ClientContextMenuAction::MissionDefinition(index) => {
                        if let Some((mission, count)) = missions.get(index) {
                            // Definitions are inspectable before member navigation is introduced.
                            self.overlay =
                                Some(ClientShellOverlay::ContextMenu(ClientContextMenuOverlay {
                                    target: ClientContextMenuTarget::Missions {
                                        missions: vec![(mission.clone(), *count)],
                                    },
                                    x: menu.x,
                                    y: menu.y,
                                    highlighted: 1,
                                }));
                        }
                    }
                    _ => {}
                }
            }

            ClientContextMenuTarget::Collection {
                endpoint_id,
                boot_id,
                generation,
                collection_id,
                hibernating,
            } => {
                let valid = endpoint_id == self.active_endpoint_id
                    && self
                        .endpoints
                        .iter()
                        .find(|endpoint| endpoint.endpoint_id == endpoint_id)
                        .is_some_and(|endpoint| {
                            endpoint.snapshot_generation == generation
                                && endpoint
                                    .snapshot
                                    .as_ref()
                                    .is_some_and(|snapshot| snapshot.boot_id == boot_id)
                                && endpoint.organization.as_ref().is_some_and(|catalog| {
                                    catalog
                                        .organization
                                        .collections
                                        .iter()
                                        .any(|collection| collection.id == collection_id)
                                })
                        });
                if valid {
                    self.push_endpoint_method(
                        crate::api::schema::Method::CollectionSetHibernating(
                            crate::api::schema::CollectionSetHibernatingParams {
                                collection_id,
                                hibernating,
                            },
                        ),
                        outcome,
                    );
                } else {
                    outcome.repaint |= self.push_endpoint_notice(
                        ClientEndpointNoticeKind::Rejected,
                        "collection.stale",
                        "Collection unavailable",
                        "The selected collection is no longer available.",
                    );
                }
            }

            ClientContextMenuTarget::CollectionPicker {
                workspace,
                family_id,
                collections,
            } => {
                if self.navigation_target_valid(&workspace)
                    && workspace.endpoint_id == self.active_endpoint_id
                {
                    if let ClientContextMenuAction::AssignCollection(index) = action {
                        if let Some(collection) = collections.get(index) {
                            self.push_endpoint_method(
                                crate::api::schema::Method::CollectionAssignFamily(
                                    crate::api::schema::CollectionAssignFamilyParams {
                                        family_id,
                                        collection_id: collection.id.clone(),
                                    },
                                ),
                                outcome,
                            );
                        }
                    }
                } else {
                    outcome.repaint |= self.push_endpoint_notice(
                        ClientEndpointNoticeKind::Rejected,
                        "stale_family",
                        "Family unavailable",
                        "The selected workspace is no longer available.",
                    );
                }
            }
            ClientContextMenuTarget::Workspace {
                workspace_id,
                close_group,
                ..
            } => self.activate_workspace_context_action(workspace_id, close_group, action, outcome),
            ClientContextMenuTarget::Tab {
                tab_id,
                workspace_id,
                mission_context,
            } => {
                if action == ClientContextMenuAction::AddToMission {
                    if let Some(workspace) = mission_context.filter(|context| {
                        self.navigation_target_valid(context)
                            && context.endpoint_id == self.active_endpoint_id
                    }) {
                        self.open_mission_picker(tab_id, workspace, outcome);
                    } else {
                        outcome.repaint |= self.push_endpoint_notice(
                            ClientEndpointNoticeKind::Rejected,
                            "mission.stale",
                            "Mission target unavailable",
                            "The selected tab is no longer available.",
                        );
                    }
                } else {
                    self.activate_tab_context_action(tab_id, workspace_id, action, outcome);
                }
            }
            ClientContextMenuTarget::Pane {
                pane_id,
                workspace_id,
                source_pane_id,
                right_click_passthrough,
                mission_context,
                ..
            } => match (action, mission_context) {
                (ClientContextMenuAction::AssignPaneMission, Some(context)) => {
                    self.open_pane_mission_picker(context, outcome)
                }
                (ClientContextMenuAction::ClearPaneMissionOverride, Some(context)) => {
                    self.submit_pane_mission(context, None, outcome)
                }
                (ClientContextMenuAction::PaneMissionInfo, Some(context)) => {
                    outcome.repaint |= self.push_endpoint_notice(
                        ClientEndpointNoticeKind::Rejected,
                        "mission.membership",
                        "Pane mission",
                        format!(
                            "{}; clearing an override inherits the tab's mission: {}.",
                            context.membership, context.inherited
                        ),
                    );
                }
                _ => self.activate_pane_context_action(
                    pane_id,
                    workspace_id,
                    source_pane_id,
                    right_click_passthrough,
                    action,
                    outcome,
                ),
            },
        }
        outcome.repaint = true;
    }

    fn activate_workspace_context_action(
        &mut self,
        workspace_id: String,
        close_group: bool,
        action: ClientContextMenuAction,
        outcome: &mut ClientShellInput,
    ) {
        use crate::input::KeybindAction;

        match action {
            ClientContextMenuAction::MoveFamilyToCollection => {
                self.open_collection_picker(workspace_id, outcome)
            }
            ClientContextMenuAction::Rename => {
                let label = self
                    .snapshot
                    .as_deref()
                    .and_then(|snapshot| {
                        snapshot
                            .workspaces
                            .iter()
                            .find(|workspace| workspace.workspace_id == workspace_id)
                    })
                    .map(|workspace| workspace.label.clone());
                if let Some(label) = label {
                    self.overlay = Some(ClientShellOverlay::Rename(ClientRenameOverlay {
                        title: "rename workspace",
                        input: TextEditor::new(&label, false),
                        target: ClientRenameTarget::Workspace { workspace_id },
                    }));
                }
            }
            ClientContextMenuAction::Close => {
                self.request_workspace_close(workspace_id, Some(close_group), outcome);
            }
            ClientContextMenuAction::NewWorktree => {
                self.begin_worktree_action_for(KeybindAction::NewWorktree, workspace_id, outcome)
            }
            ClientContextMenuAction::OpenWorktree => {
                self.begin_worktree_action_for(KeybindAction::OpenWorktree, workspace_id, outcome)
            }
            ClientContextMenuAction::RemoveWorktree => {
                self.begin_worktree_action_for(KeybindAction::RemoveWorktree, workspace_id, outcome)
            }
            ClientContextMenuAction::ToggleGroup => {
                let key = self.snapshot.as_deref().and_then(|snapshot| {
                    snapshot
                        .workspaces
                        .iter()
                        .find(|workspace| workspace.workspace_id == workspace_id)
                        .and_then(|workspace| workspace.worktree.as_ref())
                        .map(|worktree| worktree.key.clone())
                });
                if let Some(key) = key {
                    let endpoint_id = self.active_endpoint_id.clone();
                    self.toggle_collapsed_group(&endpoint_id, key);
                    self.persist_chrome_preferences(outcome);
                }
            }
            _ => {}
        }
    }

    fn activate_tab_context_action(
        &mut self,
        tab_id: String,
        workspace_id: String,
        action: ClientContextMenuAction,
        outcome: &mut ClientShellInput,
    ) {
        use crate::api::schema::{Method, TabTarget};

        self.push_endpoint_method(
            Method::TabFocus(TabTarget {
                tab_id: tab_id.clone(),
            }),
            outcome,
        );
        match action {
            ClientContextMenuAction::NewTab => {
                if self.config.prompt_new_tab_name {
                    let default_name = (self
                        .snapshot
                        .as_deref()
                        .map(|snapshot| {
                            snapshot
                                .tabs
                                .iter()
                                .filter(|tab| tab.workspace_id == workspace_id)
                                .count()
                        })
                        .unwrap_or(0)
                        + 1)
                    .to_string();
                    self.overlay = Some(ClientShellOverlay::Rename(ClientRenameOverlay {
                        title: "new tab",
                        input: TextEditor::new(&default_name, true),
                        target: ClientRenameTarget::NewTab {
                            workspace_id,
                            default_name,
                        },
                    }));
                } else {
                    self.push_endpoint_method(
                        Method::TabCreate(crate::api::schema::TabCreateParams {
                            workspace_id: Some(workspace_id),
                            cwd: None,
                            focus: true,
                            label: None,
                            env: Default::default(),
                        }),
                        outcome,
                    );
                }
            }
            ClientContextMenuAction::Rename => {
                let tab = self
                    .snapshot
                    .as_deref()
                    .and_then(|snapshot| snapshot.tabs.iter().find(|tab| tab.tab_id == tab_id));
                if let Some(tab) = tab {
                    self.overlay = Some(ClientShellOverlay::Rename(ClientRenameOverlay {
                        title: "rename tab",
                        input: TextEditor::new(&tab.label, false),
                        target: ClientRenameTarget::Tab {
                            tab_id,
                            auto_name: !tab.custom_label,
                            original_name: tab.label.clone(),
                        },
                    }));
                }
            }
            ClientContextMenuAction::Close => {
                self.request_tab_close(tab_id, outcome);
            }
            _ => {}
        }
    }

    fn activate_pane_context_action(
        &mut self,
        pane_id: String,
        workspace_id: String,
        source_pane_id: Option<String>,
        right_click_passthrough: bool,
        action: ClientContextMenuAction,
        outcome: &mut ClientShellInput,
    ) {
        use crate::api::schema::{
            Method, PaneInputSetParams, PaneRenameParams, PaneRightClickTarget, PaneSplitParams,
            PaneSwapParams, PaneTarget, PaneZoomMode, PaneZoomParams, SplitDirection,
        };

        match action {
            ClientContextMenuAction::RenamePane => {
                let label = self.snapshot.as_deref().and_then(|snapshot| {
                    snapshot
                        .panes
                        .iter()
                        .find(|pane| pane.pane_id == pane_id)
                        .and_then(|pane| pane.label.clone())
                });
                self.overlay = Some(ClientShellOverlay::Rename(ClientRenameOverlay {
                    title: "rename pane",
                    input: TextEditor::new(label.as_deref().unwrap_or_default(), label.is_none()),
                    target: ClientRenameTarget::Pane { pane_id },
                }));
            }
            ClientContextMenuAction::ClearPaneName => self.push_endpoint_method(
                Method::PaneRename(PaneRenameParams {
                    pane_id,
                    label: None,
                }),
                outcome,
            ),
            ClientContextMenuAction::SwapWithFocusedPane => {
                if let Some(source_pane_id) = source_pane_id {
                    self.push_endpoint_method(
                        Method::PaneSwap(PaneSwapParams {
                            pane_id: None,
                            direction: None,
                            source_pane_id: Some(source_pane_id.clone()),
                            target_pane_id: Some(pane_id),
                        }),
                        outcome,
                    );
                    self.push_endpoint_method(
                        Method::PaneFocus(PaneTarget {
                            pane_id: source_pane_id,
                        }),
                        outcome,
                    );
                }
            }
            ClientContextMenuAction::SplitRight | ClientContextMenuAction::SplitDown => {
                self.push_endpoint_method(
                    Method::PaneSplit(PaneSplitParams {
                        workspace_id: Some(workspace_id),
                        target_pane_id: Some(pane_id),
                        direction: if action == ClientContextMenuAction::SplitRight {
                            SplitDirection::Right
                        } else {
                            SplitDirection::Down
                        },
                        ratio: None,
                        cwd: None,
                        focus: true,
                        right_click: Default::default(),
                        env: Default::default(),
                    }),
                    outcome,
                );
            }
            ClientContextMenuAction::Zoom => self.push_endpoint_method(
                Method::PaneZoom(PaneZoomParams {
                    pane_id: Some(pane_id),
                    mode: PaneZoomMode::Toggle,
                }),
                outcome,
            ),
            ClientContextMenuAction::ToggleRightClickPassthrough => self.push_endpoint_method(
                Method::PaneInputSet(PaneInputSetParams {
                    pane_id,
                    right_click: if right_click_passthrough {
                        PaneRightClickTarget::Herdr
                    } else {
                        PaneRightClickTarget::Pane
                    },
                }),
                outcome,
            ),
            ClientContextMenuAction::ClosePane => {
                self.push_endpoint_method(Method::PaneClose(PaneTarget { pane_id }), outcome)
            }
            _ => {}
        }
    }
}
