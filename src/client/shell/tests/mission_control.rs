use super::*;

fn mission_creation_client() -> ClientShellState {
    let mut state = mission_members_client();
    let mut snap = state.snapshot.as_deref().unwrap().clone();
    let mut parent = snap.workspaces[0].clone();
    parent.workspace_id = "ws_parent".into();
    parent.label = "Repository parent".into();
    parent.worktree.as_mut().unwrap().is_linked_worktree = false;
    snap.workspaces.push(parent);
    snap.revision += 1;
    state.cache_endpoint_snapshot_for_generation(&ClientEndpointId::Local, 1, Box::new(snap));
    state.activate_endpoint_projection(&ClientEndpointId::Local);
    let mut projected = surface();
    projected.projection_revision = state.snapshot.as_ref().unwrap().revision;
    projected.surface_revision = 3;
    state.set_pane_surface(projected);
    state.set_endpoint_methods(Some(vec![
        "organization.get".into(),
        "worktree.list".into(),
        "worktree.create_in_mission".into(),
    ]));
    state
}

#[test]
fn mc_s7_contextual_preparation_revalidates_mission_source_and_independent_capability() {
    for change in ["mission", "source", "capability"] {
        let mut state = mission_creation_client();
        open(&mut state, 140, 40);
        state.handle_input_bytes(b"\tempty\x0e");
        let choose = state.handle_input_bytes(b"\r");
        let [ClientShellAction::Endpoint { request, .. }] = &choose.actions[..] else {
            panic!("source preparation");
        };
        match change {
            "mission" => {
                let mut catalog = state.endpoints[0].organization.clone().unwrap();
                catalog.organization.missions.retain(|m| m.id.0 != "empty");
                catalog.organization.revision += 1;
                state.set_endpoint_organization_for_generation(
                    &ClientEndpointId::Local,
                    1,
                    catalog,
                );
            }
            "source" => {
                let mut snapshot = state.snapshot.as_deref().unwrap().clone();
                snapshot
                    .workspaces
                    .retain(|w| w.workspace_id != "ws_parent");
                snapshot.revision += 1;
                state.cache_endpoint_snapshot_for_generation(
                    &ClientEndpointId::Local,
                    1,
                    Box::new(snapshot),
                );
                state.activate_endpoint_projection(&ClientEndpointId::Local);
            }
            _ => state.set_endpoint_methods(Some(vec![
                "organization.get".into(),
                "worktree.list".into(),
                "worktree.create".into(),
            ])),
        }
        state.handle_endpoint_result("boot-1", &request.id, Ok(worktree_list_result(None)));
        assert!(
            !matches!(state.overlay, Some(ClientShellOverlay::WorktreeCreate(_))),
            "{change} cannot revive captured creation intent"
        );
        assert!(state.visible_endpoint_notice.is_some(), "{change}");
    }
    let mut legacy = mission_creation_client();
    legacy.set_endpoint_methods(Some(vec![
        "organization.get".into(),
        "worktree.list".into(),
        "worktree.create".into(),
    ]));
    open(&mut legacy, 140, 40);
    legacy.handle_input_bytes(b"\tempty");
    assert!(frame_rows(&legacy.compose(140, 40).unwrap())
        .join("\n")
        .contains("new worktree unavailable"));
    let action = legacy.handle_input_bytes(b"\x0e");
    assert!(action.actions.is_empty());
    assert!(matches!(
        legacy.overlay,
        Some(ClientShellOverlay::MissionControl(_))
    ));
}

#[test]
fn mc_s7_heading_capture_does_not_adopt_restored_id_after_endpoint_reconnect() {
    let mut state = mission_creation_client();
    open(&mut state, 140, 40);
    state.handle_input_bytes(b"\tempty");
    state.compose(140, 40).unwrap();
    let mut snapshot = state.snapshot.as_deref().unwrap().clone();
    snapshot.boot_id = "boot-2".into();
    snapshot.revision += 1;
    let mut catalog = state.endpoints[0].organization.clone().unwrap();
    catalog.boot_id = "boot-2".into();
    state.cache_endpoint_snapshot_for_generation(&ClientEndpointId::Local, 2, Box::new(snapshot));
    state.activate_endpoint_projection(&ClientEndpointId::Local);
    state.set_endpoint_organization_for_generation(&ClientEndpointId::Local, 2, catalog);
    let mut frame = surface();
    frame.boot_id = "boot-2".into();
    frame.projection_revision = state.snapshot.as_ref().unwrap().revision;
    frame.surface_revision = 4;
    state.set_pane_surface(frame);
    let action = state.handle_input_bytes(b"\x0e");
    assert!(action.actions.is_empty());
    assert!(
        matches!(state.overlay, Some(ClientShellOverlay::MissionControl(_))),
        "reconnect does not open source picker using old heading capture"
    );
    assert!(state.visible_endpoint_notice.is_some());
}

#[test]
fn mc_s7_mouse_context_action_reuses_compact_visible_geometry() {
    let mut state = mission_creation_client();
    open(&mut state, 140, 40);
    state.handle_input_bytes(b"\tempty");
    let frame = state.compose(50, 14).unwrap();
    let rows = frame_rows(&frame);
    let y = rows
        .iter()
        .position(|row| row.contains("new worktree"))
        .expect("visible mouse action") as u16;
    let x = rows[y as usize].find("new worktree").unwrap() as u16;
    let action = state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::empty(),
    })]);
    assert!(action.actions.is_empty());
    let text = frame_rows(&state.compose(50, 14).unwrap()).join("\n");
    assert!(
        text.contains("ws_parent"),
        "mouse opens source picker for selected mission"
    );
}

#[test]
fn mc_s7_empty_mission_creation_asks_for_exact_parent_then_reuses_native_form() {
    let mut state = mission_creation_client();
    open(&mut state, 140, 40);
    state.handle_input_bytes(b"\t");
    state.handle_input_bytes(b"empty");
    state.compose(140, 40).unwrap();
    let action = state.handle_input_bytes(b"\x0e"); // Ctrl+n: new worktree in selected mission.
    assert!(
        action.actions.is_empty(),
        "explicit source choice precedes discovery"
    );
    let frame = state.compose(140, 40).unwrap();
    let text = frame_rows(&frame).join("\n");
    assert!(
        text.contains("Repository parent") && text.contains("ws_parent"),
        "source picker displays exact eligible parent"
    );
    let choose = state.handle_input_bytes(b"\r");
    let [ClientShellAction::Endpoint { request, .. }] = &choose.actions[..] else {
        panic!("source discovery");
    };
    assert!(
        matches!(&request.method, crate::api::schema::Method::WorktreeList(p) if p.workspace_id.as_deref() == Some("ws_parent"))
    );
    state.handle_endpoint_result("boot-1", &request.id, Ok(worktree_list_result(None)));
    assert!(
        matches!(&state.overlay, Some(ClientShellOverlay::WorktreeCreate(create)) if create.source_workspace_id == "ws_parent" && create.mission.as_ref().is_some_and(|m| m.id.0 == "empty"))
    );
}

#[test]
fn mc_s6_search_selects_matching_members_from_headings_and_updates_compact_hits_in_one_batch() {
    let mut state = mission_members_client();
    open(&mut state, 140, 40);
    state.handle_input_bytes(b"\t");
    let unavailable = state.handle_input_bytes(b"\r");
    assert!(
        unavailable.actions.is_empty() && unavailable.requests.is_empty(),
        "a Spaces workspace capture cannot focus while absent from Missions"
    );
    // The first arrow explicitly selects the first member from a view capture;
    // the second selects its mission heading before searching.
    state.handle_input_bytes(b"\x1b[A\x1b[A");
    state.compose(50, 14).unwrap();
    state.handle_input_bytes(b"Editor sibling");
    let frame = state.compose(50, 14).unwrap();
    assert!(frame_rows(&frame)
        .iter()
        .any(|row| row.contains("pane_2") && row.contains("Hibernate")));
    let input = state.handle_input_bytes(b"\r");
    let [ClientShellAction::Endpoint { request, .. }] = &input.actions[..] else {
        panic!("search chooses matching current pane rather than heading/hidden target");
    };
    assert_eq!(
        serde_json::to_value(&request.method).unwrap(),
        serde_json::json!({"method":"pane.focus","params":{"pane_id":"pane_2"}})
    );

    let mut state = mission_members_client();
    open(&mut state, 140, 40);
    state.handle_input_bytes(b"\tworker");
    state.compose(50, 14).unwrap();
    // Edit the query and click the newly filtered pane row in one input batch.
    let frame = state.compose(50, 14).unwrap();
    let y = frame_rows(&frame)
        .iter()
        .position(|row| row.contains("pane_1"))
        .unwrap() as u16;
    let rect = state
        .hits
        .mission_control_rows
        .iter()
        .find(|(rect, _)| rect.y == y)
        .unwrap()
        .0;
    let input = state.handle_raw_events(vec![
        RawInputEvent::Text(crate::input::TextCommit::new(" runtime")),
        RawInputEvent::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: rect.x,
            row: rect.y,
            modifiers: KeyModifiers::empty(),
        }),
    ]);
    let [ClientShellAction::Endpoint { request, .. }] = &input.actions[..] else {
        panic!("mouse uses current filtered geometry");
    };
    assert_eq!(
        serde_json::to_value(&request.method).unwrap(),
        serde_json::json!({"method":"pane.focus","params":{"pane_id":"pane_2"}})
    );
}

#[test]
fn mc_s6_member_assignment_reuses_exact_dialogs_with_independent_methods_and_confirmed_state() {
    for (target, method) in [
        ("tab_1", "mission.assign"),
        ("pane_1", "mission.assign_pane"),
    ] {
        let mut state = mission_members_client();
        state.set_endpoint_methods(Some(vec![
            "organization.get".into(),
            method.into(),
            "pane.focus".into(),
            "tab.focus".into(),
        ]));
        let before = state.endpoints[0].organization.clone();
        open(&mut state, 140, 40);
        state.handle_input_bytes(b"\t");
        let frame = state.compose(140, 40).unwrap();
        let y = frame_rows(&frame)
            .iter()
            .position(|row| row.contains(target))
            .unwrap() as u16;
        let rect = state
            .hits
            .mission_control_rows
            .iter()
            .find(|(rect, _)| rect.y == y)
            .unwrap()
            .0;
        let input = state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Right),
            column: rect.x,
            row: rect.y,
            modifiers: KeyModifiers::empty(),
        })]);
        assert!(input.actions.is_empty() && input.requests.is_empty());
        assert!(
            matches!(state.overlay, Some(ClientShellOverlay::ContextMenu(_))),
            "supported assignment opens existing picker without requiring other mission methods"
        );
        let input = state.handle_input_bytes(b"\r");
        let [ClientShellAction::Endpoint { request, .. }] = &input.actions[..] else {
            panic!("assignment travels through public method");
        };
        let request = serde_json::to_value(&request.method).unwrap();
        assert_eq!(request["method"], method);
        if target == "tab_1" {
            assert_eq!(request["params"]["target"]["tab_id"], target);
        } else {
            assert_eq!(request["params"]["pane_id"], target);
        }
        assert_eq!(
            state.endpoints[0].organization, before,
            "assignment waits for confirmed catalog"
        );
    }
}

#[test]
fn mc_s6_removed_membership_and_blocked_exit_keep_stale_actions_across_search_and_views() {
    for needs_you in [false, true] {
        let mut state = mission_members_client();
        if needs_you {
            let mut snap = state.snapshot.as_deref().unwrap().clone();
            snap.agents[0].agent_status = AgentStatus::Blocked;
            snap.revision += 1;
            state.cache_endpoint_snapshot_for_generation(
                &ClientEndpointId::Local,
                1,
                Box::new(snap),
            );
            state.activate_endpoint_projection(&ClientEndpointId::Local);
            let mut frame = surface();
            frame.projection_revision = 3;
            frame.surface_revision = 3;
            state.set_pane_surface(frame);
        }
        open(&mut state, 140, 40);
        state.handle_input_bytes(if needs_you { b"\t\t" } else { b"\t" });
        state.handle_input_bytes(b"worker");
        if needs_you {
            let mut snap = state.snapshot.as_deref().unwrap().clone();
            snap.agents[0].agent_status = AgentStatus::Working;
            snap.revision += 1;
            state.cache_endpoint_snapshot_for_generation(
                &ClientEndpointId::Local,
                1,
                Box::new(snap),
            );
            state.activate_endpoint_projection(&ClientEndpointId::Local);
        } else {
            let mut catalog = state.endpoints[0].organization.clone().unwrap();
            catalog.organization.pane_mission_assignments.clear();
            catalog.organization.revision += 1;
            state.set_endpoint_organization_for_generation(&ClientEndpointId::Local, 1, catalog);
        }
        for bytes in [b"\r".as_slice(), b"\x7f\r", b"\t\r", b"\t\r", b"\t\r"] {
            let input = state.handle_input_bytes(bytes);
            assert!(
                input.actions.is_empty() && input.requests.is_empty(),
                "state transition or view switch must not revive stale capture"
            );
            assert!(matches!(
                state.overlay,
                Some(ClientShellOverlay::MissionControl(_))
            ));
            assert!(state.visible_endpoint_notice.is_some());
        }
        // Reselecting a current row is an explicit choice and can navigate.
        state.handle_input_bytes(if needs_you { b"\t\t" } else { b"\x1b[Z" });
        state.handle_input_bytes(b"\x1b[B");
        let input = state.handle_input_bytes(b"\r");
        assert!(!input.actions.is_empty());
    }
}

#[test]
fn mc_s6_needs_you_uses_authoritative_blocked_agents_including_parked_outside_custom_view() {
    let mut state = mission_members_client();
    let mut snap = state.snapshot.as_deref().unwrap().clone();
    snap.agents[0].agent_status = AgentStatus::Blocked;
    snap.agents[0].display_agent = Some("Approval worker".into());
    snap.agents[0].state_labels = vec![("blocked".into(), "Awaiting approval".into())];
    snap.agent_view_label = Some("Other agents".into());
    snap.agent_order.clear();
    snap.revision += 1;
    state.cache_endpoint_snapshot_for_generation(&ClientEndpointId::Local, 1, Box::new(snap));
    state.activate_endpoint_projection(&ClientEndpointId::Local);
    let mut frame = surface();
    frame.projection_revision = 3;
    frame.surface_revision = 3;
    state.set_pane_surface(frame);
    open(&mut state, 140, 40);
    state.handle_input_bytes(b"\t\t");
    let frame = state.compose(140, 40).unwrap();
    let text = frame_rows(&frame).join("\n");
    assert!(text.contains("Approval worker") && text.contains("Awaiting approval"));
    assert!(text.contains("Hibernate") && text.contains("pane_1"));
    assert!(!text.contains("Editor sibling") && !text.contains("pane_2"));
    state.handle_input_bytes(b"\x1b[B");
    let input = state.handle_input_bytes(b"\r");
    let [ClientShellAction::Endpoint { request, .. }] = &input.actions[..] else {
        panic!("exact blocked jump");
    };
    assert_eq!(
        serde_json::to_value(&request.method).unwrap(),
        serde_json::json!({"method":"pane.focus","params":{"pane_id":"pane_1"}})
    );
    assert!(
        state.endpoints[0]
            .organization
            .as_ref()
            .unwrap()
            .organization
            .collections[1]
            .hibernating
    );
}

fn mission_members_client() -> ClientShellState {
    let mut state = parked_client();
    let mut snap = state.snapshot.as_deref().unwrap().clone();
    let mut pane = snap.panes[0].clone();
    pane.pane_id = "pane_2".into();
    pane.label = Some("Editor sibling".into());
    pane.focused = false;
    snap.panes.push(pane);
    snap.revision += 1;
    state.cache_endpoint_snapshot_for_generation(&ClientEndpointId::Local, 1, Box::new(snap));
    state.activate_endpoint_projection(&ClientEndpointId::Local);
    let mut frame = surface();
    frame.projection_revision = state.snapshot.as_ref().unwrap().revision;
    frame.surface_revision = 2;
    state.set_pane_surface(frame);
    let mut catalog = state.endpoints[0].organization.clone().unwrap();
    catalog
        .organization
        .missions
        .push(crate::organization::Mission {
            id: crate::organization::MissionId("runtime".into()),
            name: "Agent runtime".to_owned().try_into().unwrap(),
            objective: Some("Keep runtime healthy".into()),
            order: 1,
        });
    catalog
        .organization
        .missions
        .push(crate::organization::Mission {
            id: crate::organization::MissionId("empty".into()),
            name: "Empty objective".to_owned().try_into().unwrap(),
            objective: None,
            order: 2,
        });
    catalog.organization.mission_assignments[0].mission_id =
        crate::organization::MissionId("runtime".into());
    catalog.organization.pane_mission_assignments.push(
        crate::organization::PaneMissionAssignment {
            pane_id: "pane_1".into(),
            mission_id: crate::organization::MissionId("platform".into()),
        },
    );
    catalog.organization.revision += 1;
    state.set_endpoint_organization_for_generation(&ClientEndpointId::Local, 1, catalog);
    state
}

#[test]
fn mc_s6_missions_show_empty_definitions_explicit_tabs_and_effective_panes_once() {
    let mut state = mission_members_client();
    open(&mut state, 140, 40);
    state.handle_input_bytes(b"\t");
    let frame = state.compose(140, 40).unwrap();
    let text = frame_rows(&frame).join("\n");
    assert!(
        text.contains("Tako platform")
            && text.contains("Agent runtime")
            && text.contains("Empty objective")
    );
    assert!(text.contains("Keep runtime healthy"));
    assert_eq!(text.matches("pane_1").count(), 1);
    assert_eq!(text.matches("pane_2").count(), 1);
    assert!(text.contains("tab_1") && text.contains("explicit") && text.contains("inherited"));
    assert!(text.contains("Hibernate") && text.contains("/repo/worktrees/feature"));
    state.handle_input_bytes(b"worker Tako");
    state.handle_input_bytes(b"\x1b[B");
    let input = state.handle_input_bytes(b"\r");
    let [ClientShellAction::Endpoint { request, .. }] = &input.actions[..] else {
        panic!("exact member jump");
    };
    assert_eq!(
        serde_json::to_value(&request.method).unwrap(),
        serde_json::json!({"method":"pane.focus","params":{"pane_id":"pane_1"}})
    );
}

#[test]
fn mc_s6_view_switches_own_input_and_mouse_without_changing_terminal_context() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(snapshot()));
    state.set_pane_surface(surface());
    let before = serde_json::to_value(state.snapshot.as_ref()).unwrap();
    state.compose(50, 14).unwrap();
    state.handle_input_bytes(b"\x02m");
    let input = state.handle_input_bytes(b"\t");
    assert!(input.actions.is_empty() && input.requests.is_empty() && !input.resize);
    let frame = state.compose(50, 14).unwrap();
    let rows = frame_rows(&frame);
    assert!(rows.iter().any(|r| r.contains("Missions")));
    assert!(rows.iter().any(|r| r.contains("Needs you")));
    assert!(rows.iter().any(|r| r.contains("No missions")));
    let y = rows.iter().position(|r| r.contains("Needs you")).unwrap() as u16;
    let x = rows[y as usize].find("Needs you").unwrap() as u16;
    let result = click(&mut state, Rect::new(x, y, 1, 1));
    assert!(result.actions.is_empty() && result.requests.is_empty());
    let frame = state.compose(50, 14).unwrap();
    assert!(frame_rows(&frame)
        .iter()
        .any(|r| r.contains("No agents need you")));
    state.handle_input_bytes(b"\x1b");
    assert_eq!(
        serde_json::to_value(state.snapshot.as_ref()).unwrap(),
        before
    );
}

fn click(state: &mut ClientShellState, rect: Rect) -> ClientShellInput {
    state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: rect.x,
        row: rect.y,
        modifiers: KeyModifiers::empty(),
    })])
}

fn open(state: &mut ClientShellState, cols: u16, rows: u16) {
    state.compose(cols, rows).unwrap();
    click(state, state.hits.global_launcher);
    let frame = state.compose(cols, rows).unwrap();
    let y = frame_rows(&frame)
        .iter()
        .position(|row| row.contains("Mission control"))
        .expect("Mission control is available in the existing menu") as u16;
    let rect = state
        .hits
        .global_menu_rows
        .iter()
        .find(|(rect, _)| rect.y == y)
        .unwrap()
        .0;
    let input = click(state, rect);
    assert!(input.actions.is_empty() && input.requests.is_empty() && !input.resize);
}

#[test]
fn mc_s5_open_search_cancel_preserves_terminal_context_without_forwarding_input() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    let mut snap = snapshot();
    snap.tabs[0].zoomed = true;
    state.set_snapshot(Box::new(snap));
    let mut scrolled = surface();
    scrolled.panes[0].scroll = Some(crate::protocol::PaneSurfaceScrollMetrics {
        offset_from_bottom: 12,
        max_offset_from_bottom: 40,
        viewport_rows: 2,
    });
    state.set_pane_surface(scrolled);
    let before = serde_json::to_value(state.snapshot.as_ref()).unwrap();
    let before_surface = state.pane_surface.clone();
    let layout = state.layout(106, 30);
    open(&mut state, 106, 30);
    let input = state.handle_input_bytes(b"find a tab");
    assert!(input.actions.is_empty() && input.requests.is_empty());
    let frame = state.compose(106, 30).unwrap();
    assert!(frame_rows(&frame)
        .iter()
        .any(|row| row.contains("find a tab")));
    assert!(frame_rows(&frame).iter().any(|row| row.contains("Spaces")));
    let cancel = state.handle_input_bytes(b"\x1b");
    assert!(cancel.actions.is_empty() && cancel.requests.is_empty() && !cancel.resize);
    assert!(state.overlay.is_none());
    assert_eq!(
        serde_json::to_value(state.snapshot.as_ref()).unwrap(),
        before
    );
    assert_eq!(state.pane_surface, before_surface);
    assert_eq!(state.layout(106, 30), layout);
}

#[test]
fn mc_s5_launch_binding_uses_configured_direct_and_default_prefix_keys() {
    for (config, bytes) in [
        (Config::default(), b"\x02m".as_slice()),
        (
            toml::from_str::<Config>("[keys]\nmission_control = 'ctrl+g'").unwrap(),
            b"\x07".as_slice(),
        ),
    ] {
        let mut state = ClientShellState::new(ClientShellConfig::from_config(&config));
        state.set_snapshot(Box::new(snapshot()));
        state.set_pane_surface(surface());
        let input = state.handle_input_bytes(bytes);
        assert!(input.requests.is_empty() && input.actions.is_empty() && !input.resize);
        let frame = state.compose(106, 30).unwrap();
        assert!(frame_rows(&frame)
            .iter()
            .any(|row| row.contains("Mission control")));
        state.handle_input_bytes(b"\x1b");
        assert!(state.overlay.is_none());
    }
}

#[test]
fn mc_s5_spaces_searches_tab_location_branch_and_keeps_duplicate_targets_exact() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    let mut snap = snapshot();
    snap.workspaces[0].branch = Some("release-candidate".into());
    for id in ["tab_2", "tab_3"] {
        let mut tab = snap.tabs[0].clone();
        tab.tab_id = id.into();
        tab.label = "Review".into();
        tab.focused = false;
        snap.tabs.push(tab);
    }
    state.set_snapshot(Box::new(snap));
    state.set_pane_surface(surface());
    state.set_endpoint_methods(Some(vec![
        "workspace.focus".into(),
        "tab.focus".into(),
        "pane.focus".into(),
    ]));
    open(&mut state, 106, 30);
    state.handle_input_bytes(b"review release-candidate /repo");
    let frame = state.compose(106, 30).unwrap();
    assert!(frame_rows(&frame).iter().any(|row| row.contains("tab_2")));
    assert!(frame_rows(&frame).iter().any(|row| row.contains("tab_3")));
    state.handle_input_bytes(b"\x1b[B");
    let input = state.handle_input_bytes(b"\r");
    let [ClientShellAction::Endpoint {
        endpoint_id,
        boot_id,
        request,
    }] = &input.actions[..]
    else {
        panic!("exact tab focus travels through the existing endpoint command");
    };
    assert_eq!(endpoint_id, &ClientEndpointId::Local);
    assert_eq!(boot_id, "boot-1");
    assert_eq!(
        serde_json::to_value(&request.method).unwrap(),
        serde_json::json!({"method":"tab.focus","params":{"tab_id":"tab_3"}})
    );
    assert!(state.overlay.is_none());
}

fn parked_client() -> ClientShellState {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    let mut snap = snapshot();
    snap.workspaces[0].new_workspace_cwd = "/repo/worktrees/feature".into();
    snap.workspaces[0].worktree = Some(ClientShellWorktree {
        key: "repo-key".into(),
        label: "Repository family".into(),
        is_linked_worktree: true,
    });
    snap.panes[0].foreground_cwd = Some("/actual/worker".into());
    snap.agents.push(ClientShellAgent {
        pane_id: "pane_1".into(),
        workspace_id: "ws_1".into(),
        tab_id: "tab_1".into(),
        name: Some("worker".into()),
        display_agent: None,
        agent: Some("codex".into()),
        title: None,
        terminal_title: None,
        terminal_title_stripped: None,
        agent_status: AgentStatus::Idle,
        state_change_seq: 1,
        state_labels: vec![],
        tokens: vec![("role".into(), "build-label".into())],
        focused: true,
    });
    state.cache_endpoint_snapshot_for_generation(&ClientEndpointId::Local, 1, Box::new(snap));
    state.activate_endpoint_projection(&ClientEndpointId::Local);
    state.set_pane_surface(surface());
    state.set_endpoint_methods(Some(vec![
        "organization.get".into(),
        "workspace.focus".into(),
        "tab.focus".into(),
        "pane.focus".into(),
    ]));
    state.set_endpoint_organization_supported(&ClientEndpointId::Local, true);
    state.set_endpoint_organization_for_generation(&ClientEndpointId::Local,1,serde_json::from_value(serde_json::json!({"boot_id":"boot-1","organization":{
        "revision":2,"collections":[{"id":"active","name":"Infrastructure","order":0,"hibernating":false},{"id":"parked","name":"Side quests","order":1,"hibernating":true}],
        "family_assignments":[{"family_id":{"kind":"managed","key":"repo-key"},"collection_id":"parked"}],
        "missions":[{"id":"platform","name":"Tako platform","order":0}],
        "mission_assignments":[{"target":{"kind":"tab","tab_id":"tab_1"},"mission_id":"platform"}]
    }})).unwrap());
    state
}

#[test]
fn mc_s5_spaces_search_includes_parked_agent_location_and_assigned_labels_without_unparking() {
    let mut state = parked_client();
    let before = state.endpoints[0].organization.clone();
    open(&mut state, 106, 30);
    let frame = state.compose(106, 30).unwrap();
    let text = frame_rows(&frame).join("\n");
    assert!(
        text.contains("Infrastructure")
            && text.contains("Uncollected")
            && text.contains("Hibernate")
    );
    assert!(
        !text.contains("Side quests"),
        "Hibernate starts collapsed in the overview"
    );
    state.handle_input_bytes(b"Tako platform build-label /actual/worker");
    let frame = state.compose(106, 30).unwrap();
    let text = frame_rows(&frame).join("\n");
    assert!(text.contains("worker") && text.contains("Hibernate") && text.contains("pane_1"));
    let input = state.handle_input_bytes(b"\r");
    assert!(
        matches!(&input.actions[..],[ClientShellAction::Endpoint {request,..}] if matches!(&request.method,crate::api::schema::Method::PaneFocus(p) if p.pane_id=="pane_1"))
    );
    assert!(state.overlay.is_none());
    assert_eq!(state.endpoints[0].organization, before);
    assert!(state.expanded_hibernate.contains(&ClientEndpointId::Local));
}

#[test]
fn mc_s5_closed_or_reconnected_selection_never_redirects_to_a_replacement() {
    for change in ["closed", "boot", "generation", "method"] {
        let mut state = parked_client();
        open(&mut state, 106, 30);
        state.handle_input_bytes(b"worker /actual");
        state.compose(106, 30).unwrap();
        let mut changed = state.snapshot.as_deref().unwrap().clone();
        changed.revision += 1;
        let generation = if change == "generation" { 2 } else { 1 };
        match change {
            "closed" => {
                changed.panes[0].pane_id = "pane_2".into();
                changed.agents[0].pane_id = "pane_2".into();
            }
            "boot" => changed.boot_id = "boot-2".into(),
            "method" => state.set_endpoint_methods(Some(vec!["organization.get".into()])),
            _ => {}
        }
        state.cache_endpoint_snapshot_for_generation(
            &ClientEndpointId::Local,
            generation,
            Box::new(changed.clone()),
        );
        state.activate_endpoint_projection(&ClientEndpointId::Local);
        let mut updated_surface = surface();
        updated_surface.boot_id = changed.boot_id;
        updated_surface.projection_revision = changed.revision;
        state.set_pane_surface(updated_surface);
        let input = state.handle_input_bytes(b"\r");
        assert!(
            input.actions.is_empty() && input.requests.is_empty(),
            "{change} selection must fail closed"
        );
        state.handle_input_bytes(b"\x1b");
        state.tick_notifications(std::time::Instant::now() + std::time::Duration::from_secs(60));
        state.handle_input_bytes(b"typed after notice");
        let text = frame_rows(&state.compose(106, 30).unwrap()).join("\n");
        assert!(text.contains("Target unavailable"), "{change}: {text}");
    }
}

#[test]
fn mc_s5_overlay_owns_popup_text_paste_mouse_and_held_repeats_but_keeps_owed_release() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(snapshot()));
    state.set_pane_surface(surface());
    let key = crate::input::TerminalKey::new(KeyCode::Char('x'), KeyModifiers::empty());
    assert!(!state
        .handle_raw_events(vec![RawInputEvent::Key(key.clone())])
        .requests
        .is_empty());
    open(&mut state, 106, 30);
    let repeat = state.handle_raw_events(vec![RawInputEvent::Key(
        key.clone()
            .with_kind(crossterm::event::KeyEventKind::Repeat),
    )]);
    assert!(
        repeat.actions.is_empty() && repeat.requests.is_empty(),
        "held pane keys cannot repeat into the shell while open"
    );
    let release = state.handle_raw_events(vec![RawInputEvent::Key(
        key.with_kind(crossterm::event::KeyEventKind::Release),
    )]);
    assert!(
        matches!(&release.requests[..],[ClientMessage::ClientShellPaneInput {pane_id,events}] if pane_id=="pane_1" && matches!(&events[..],[ClientPaneInputEvent::Key {kind:crate::protocol::ClientKeyKind::Release,..}]))
    );
    state.set_pane_surface(surface_with_popup());
    for event in [
        RawInputEvent::Text(crate::input::TextCommit::new("IME")),
        RawInputEvent::Paste(" pasted".into()),
    ] {
        let input = state.handle_raw_events(vec![event]);
        assert!(input.requests.is_empty() && input.actions.is_empty());
    }
    let frame = state.compose(106, 30).unwrap();
    assert!(frame_rows(&frame).join("\n").contains("IME pasted"));
    for kind in [
        MouseEventKind::Down(MouseButton::Right),
        MouseEventKind::ScrollUp,
        MouseEventKind::Moved,
    ] {
        let input = state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
            kind,
            column: 100,
            row: 29,
            modifiers: KeyModifiers::empty(),
        })]);
        assert!(input.requests.is_empty() && input.actions.is_empty());
    }
    assert_eq!(state.clipboard_image_target(), None);
    assert!(state.modal_paste_target_active());
    let input = state.handle_input_bytes(b"\x1b");
    assert!(input.requests.is_empty() && input.actions.is_empty());
    assert!(state.overlay.is_none());
}

fn many_tabs_client() -> ClientShellState {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    let mut snap = snapshot();
    for index in 0..30 {
        let mut tab = snap.tabs[0].clone();
        tab.tab_id = format!("tab_{}", index + 2);
        tab.label = format!("Target {index:02}");
        tab.focused = false;
        snap.tabs.push(tab);
    }
    state.set_snapshot(Box::new(snap));
    state.set_pane_surface(surface());
    state.set_endpoint_methods(Some(vec!["tab.focus".into()]));
    state
}

#[test]
fn mc_s5_keyboard_mouse_agree_after_search_page_scroll_and_compact_resize() {
    let mut keyboard = many_tabs_client();
    let mut mouse = many_tabs_client();
    for state in [&mut keyboard, &mut mouse] {
        open(state, 106, 30);
        state.handle_input_bytes(b"Target");
        for _ in 0..40 {
            state.handle_input_bytes(b"\x1b[6~");
        }
        let frame = state.compose(50, 12).unwrap();
        assert!(
            frame_rows(&frame).join("\n").contains("Target 29"),
            "keyboard selection is revealed after compact resize"
        );
    }
    let keyed = keyboard.handle_input_bytes(b"\r");
    let frame = mouse.compose(50, 12).unwrap();
    let y = frame_rows(&frame)
        .iter()
        .position(|row| row.contains("Target 29"))
        .unwrap() as u16;
    let clicked = click(&mut mouse, Rect::new(10, y, 1, 1));
    for input in [keyed, clicked] {
        assert!(
            matches!(&input.actions[..],[ClientShellAction::Endpoint {request,..}] if matches!(&request.method,crate::api::schema::Method::TabFocus(t) if t.tab_id=="tab_31"))
        );
    }
}

fn click_label(
    state: &mut ClientShellState,
    label: &str,
    cols: u16,
    rows: u16,
) -> ClientShellInput {
    let frame = state.compose(cols, rows).unwrap();
    let y = frame_rows(&frame)
        .iter()
        .position(|row| row.contains(label))
        .unwrap_or_else(|| panic!("missing {label}",)) as u16;
    click(
        state,
        Rect::new(
            if cols < 60 {
                3
            } else {
                state.hits.overlay_cancel.x.saturating_sub(70)
            },
            y,
            1,
            1,
        ),
    )
}

#[test]
fn mc_s5_hibernate_collection_family_collapse_and_close_are_client_owned() {
    let mut state = parked_client();
    let mut snap = state.snapshot.as_deref().unwrap().clone();
    snap.revision += 1;
    let mut parent = snap.workspaces[0].clone();
    parent.workspace_id = "ws_2".into();
    parent.label = "Family parent".into();
    parent.worktree.as_mut().unwrap().is_linked_worktree = false;
    snap.workspaces.push(parent);
    state.cache_endpoint_snapshot_for_generation(&ClientEndpointId::Local, 1, Box::new(snap));
    state.activate_endpoint_projection(&ClientEndpointId::Local);
    let mut surf = surface();
    surf.projection_revision = 2;
    state.set_pane_surface(surf);
    let before = state.endpoints[0].organization.clone();
    open(&mut state, 106, 40);
    let expand = click_label(&mut state, "Hibernate", 106, 40);
    assert!(expand.actions.is_empty() && expand.requests.is_empty());
    assert!(frame_rows(&state.compose(106, 40).unwrap())
        .join("\n")
        .contains("Side quests"));
    let fold = click_label(&mut state, "Repository family", 106, 40);
    assert!(fold.actions.is_empty() && fold.requests.is_empty());
    assert!(!frame_rows(&state.compose(106, 40).unwrap())
        .join("\n")
        .contains("Family parent"));
    click_label(&mut state, "Repository family", 106, 40);
    assert!(frame_rows(&state.compose(106, 40).unwrap())
        .join("\n")
        .contains("Family parent"));
    click_label(&mut state, "Side quests", 106, 40);
    assert!(!frame_rows(&state.compose(106, 40).unwrap())
        .join("\n")
        .contains("Family parent"));
    let close = state.hits.overlay_cancel;
    let cancel = click(&mut state, close);
    assert!(cancel.actions.is_empty() && cancel.requests.is_empty());
    assert!(state.overlay.is_none());
    assert_eq!(state.endpoints[0].organization, before);
    assert!(!state.expanded_hibernate.contains(&ClientEndpointId::Local));
}

#[test]
fn mc_s5_search_finds_all_assigned_labels_and_refreshes_metadata_without_replacing_identity() {
    let mut state = parked_client();
    open(&mut state, 106, 30);
    state.handle_input_bytes(b"worker /actual");
    let mut changed = state.snapshot.as_deref().unwrap().clone();
    changed.revision += 1;
    changed.panes[0].label = Some("deploy-prod".into());
    state.cache_endpoint_snapshot_for_generation(&ClientEndpointId::Local, 1, Box::new(changed));
    state.activate_endpoint_projection(&ClientEndpointId::Local);
    let mut surf = surface();
    surf.projection_revision = 2;
    state.set_pane_surface(surf);
    state.handle_input_bytes(b"\x01\x0bdeploy-prod");
    let frame = state.compose(106, 30).unwrap();
    assert!(frame_rows(&frame).join("\n").contains("worker"));
    let input = state.handle_input_bytes(b"\r");
    assert!(
        matches!(&input.actions[..],[ClientShellAction::Endpoint {request,..}] if matches!(&request.method,crate::api::schema::Method::PaneFocus(p) if p.pane_id=="pane_1"))
    );
}

#[test]
fn mc_s5_navigation_rejection_notice_survives_typing_and_endpoint_reboot() {
    let mut state = parked_client();
    open(&mut state, 106, 30);
    state.handle_input_bytes(b"worker /actual");
    let input = state.handle_input_bytes(b"\r");
    let [ClientShellAction::Endpoint { request, .. }] = &input.actions[..] else {
        panic!("pane focus request");
    };
    let (repaint, actions) = state.handle_endpoint_result(
        "boot-1",
        &request.id,
        Err(ClientShellEndpointError {
            code: Some("stale_target".into()),
            message: "That pane closed before focus.".into(),
        }),
    );
    assert!(repaint && actions.is_empty());
    let mut changed = state.snapshot.as_deref().unwrap().clone();
    changed.boot_id = "boot-2".into();
    changed.revision = 1;
    state.cache_endpoint_snapshot_for_generation(&ClientEndpointId::Local, 2, Box::new(changed));
    state.activate_endpoint_projection(&ClientEndpointId::Local);
    let mut surf = surface();
    surf.boot_id = "boot-2".into();
    state.set_pane_surface(surf);
    state.tick_notifications(std::time::Instant::now() + std::time::Duration::from_secs(60));
    state.handle_input_bytes(b"typed");
    assert!(frame_rows(&state.compose(106, 30).unwrap())
        .join("\n")
        .contains("Target unavailable"));
}

#[test]
fn mc_s5_query_and_wheel_update_mouse_targets_before_the_next_frame() {
    let mut state = many_tabs_client();
    open(&mut state, 106, 12);
    state.compose(106, 12).unwrap();
    let y = state.hits.mission_control_rows[0].0.y;
    let input = state.handle_raw_events(vec![
        RawInputEvent::Text(crate::input::TextCommit::new("Target 29")),
        RawInputEvent::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 10,
            row: y,
            modifiers: KeyModifiers::empty(),
        }),
    ]);
    assert!(
        matches!(&input.actions[..],[ClientShellAction::Endpoint {request,..}] if matches!(&request.method,crate::api::schema::Method::TabFocus(t) if t.tab_id=="tab_31"))
    );

    let mut state = many_tabs_client();
    open(&mut state, 106, 12);
    state.handle_input_bytes(b"Target");
    state.compose(106, 12).unwrap();
    let y = state.hits.mission_control_rows[0].0.y;
    let input = state.handle_raw_events(vec![
        RawInputEvent::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 10,
            row: y,
            modifiers: KeyModifiers::empty(),
        }),
        RawInputEvent::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 10,
            row: y,
            modifiers: KeyModifiers::empty(),
        }),
    ]);
    assert!(
        matches!(&input.actions[..],[ClientShellAction::Endpoint {request,..}] if matches!(&request.method,crate::api::schema::Method::TabFocus(t) if t.tab_id=="tab_5"))
    );
}

#[test]
fn mc_s5_open_stops_selection_drag_autoscroll_without_moving_the_viewport() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(snapshot()));
    let mut surf = surface();
    surf.panes[0].scroll = Some(crate::protocol::PaneSurfaceScrollMetrics {
        offset_from_bottom: 0,
        max_offset_from_bottom: 20,
        viewport_rows: 2,
    });
    state.set_pane_surface(surf);
    state.compose(106, 20).unwrap();
    let pane = state.hits.panes[0].clone();
    state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: pane.inner_rect.x,
        row: pane.inner_rect.y + 1,
        modifiers: KeyModifiers::empty(),
    })]);
    let dragged = state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        column: pane.inner_rect.x,
        row: pane.inner_rect.y.saturating_sub(1),
        modifiers: KeyModifiers::empty(),
    })]);
    let [ClientShellAction::Endpoint { request, .. }] = &dragged.actions[..] else {
        panic!("edge drag scroll");
    };
    state.handle_endpoint_result("boot-1", &request.id, Ok(pane_scroll_result(3, 20, 2)));
    let before = state.pane_surface.clone();
    let mut launch = ClientShellInput::default();
    state.record_binding(
        crate::input::KeybindMatch::Action(crate::input::KeybindAction::OpenMissionControl),
        &mut launch,
    );
    assert!(launch.requests.is_empty() && launch.actions.is_empty());
    let tick = state
        .tick_selection_autoscroll(std::time::Instant::now() + std::time::Duration::from_secs(1));
    assert!(
        tick.requests.is_empty() && tick.actions.is_empty(),
        "overview owns input; an old drag must not scroll its underlying pane"
    );
    state.handle_input_bytes(b"\x1b");
    assert_eq!(state.pane_surface, before);
}

#[test]
fn mc_s5_compact_results_keep_parked_marker_and_public_id_visible_for_long_labels() {
    let mut state = parked_client();
    let mut snap = state.snapshot.as_deref().unwrap().clone();
    snap.revision += 1;
    snap.agents[0].name = Some("A very long assigned name that fills this narrow screen".into());
    state.cache_endpoint_snapshot_for_generation(&ClientEndpointId::Local, 1, Box::new(snap));
    state.activate_endpoint_projection(&ClientEndpointId::Local);
    let mut surf = surface();
    surf.projection_revision = 2;
    state.set_pane_surface(surf);
    open(&mut state, 106, 30);
    state.handle_input_bytes(b"build-label");
    let frame = state.compose(50, 12).unwrap();
    let text = frame_rows(&frame).join("\n");
    assert!(
        text.contains("[Hibernate]") && text.contains("pane_1"),
        "compact results identify both parked state and exact target: {text}"
    );
}

#[test]
fn mc_s5_editing_search_after_target_closure_keeps_the_stale_capture_until_explicit_selection() {
    let mut state = parked_client();
    open(&mut state, 106, 30);
    state.handle_input_bytes(b"worker /actual");
    state.compose(106, 30).unwrap();
    let mut changed = state.snapshot.as_deref().unwrap().clone();
    changed.revision += 1;
    changed.panes[0].pane_id = "pane_2".into();
    changed.agents[0].pane_id = "pane_2".into();
    state.cache_endpoint_snapshot_for_generation(&ClientEndpointId::Local, 1, Box::new(changed));
    state.activate_endpoint_projection(&ClientEndpointId::Local);
    let mut surf = surface();
    surf.projection_revision = 2;
    state.set_pane_surface(surf);
    state.handle_input_bytes(b"\x7f");
    let stale = state.handle_input_bytes(b"\r");
    assert!(
        stale.requests.is_empty() && stale.actions.is_empty(),
        "editing search cannot silently adopt a closed target's replacement"
    );
    state.handle_input_bytes(b"\x1b[B");
    let selected = state.handle_input_bytes(b"\r");
    assert!(
        matches!(&selected.actions[..],[ClientShellAction::Endpoint {request,..}] if matches!(&request.method,crate::api::schema::Method::PaneFocus(p) if p.pane_id=="pane_2"))
    );
}

#[test]
fn mc_s5_search_from_a_heading_selects_a_matching_target_when_every_collection_is_parked() {
    let mut state = parked_client();
    let mut catalog = state.endpoints[0].organization.clone().unwrap();
    catalog.organization.revision += 1;
    catalog.organization.collections.retain(|c| c.hibernating);
    state.set_endpoint_organization_for_generation(&ClientEndpointId::Local, 1, catalog);
    open(&mut state, 106, 30);
    state.handle_input_bytes(b"worker /actual");
    let input = state.handle_input_bytes(b"\r");
    assert!(
        matches!(&input.actions[..],[ClientShellAction::Endpoint {request,..}] if matches!(&request.method,crate::api::schema::Method::PaneFocus(p) if p.pane_id=="pane_1"))
    );
}
