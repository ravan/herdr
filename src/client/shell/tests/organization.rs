use super::*;

fn click(state: &mut ClientShellState, rect: Rect) -> ClientShellInput {
    state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: rect.x,
        row: rect.y,
        modifiers: KeyModifiers::empty(),
    })])
}

fn organization_catalog(
    boot: &str,
    revision: u64,
    name: &str,
) -> crate::protocol::endpoint::EndpointOrganizationCatalog {
    serde_json::from_value(serde_json::json!({"boot_id":boot,"organization":{"revision":revision,
        "collections":[{"id":"collection_1","name":name,"order":0,"hibernating":false}],"family_assignments":[]}})).unwrap()
}

#[test]
fn mc_s1_client_create_uses_the_menu_and_keeps_form_text_out_of_the_terminal() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(snapshot()));
    state.set_pane_surface(surface());
    state.set_endpoint_methods(Some(vec![
        "organization.get".into(),
        "collection.create".into(),
        "collection.assign_family".into(),
    ]));
    state.set_endpoint_organization_supported(&ClientEndpointId::Local, true);
    state.compose(106, 30).unwrap();
    let launcher = state.hits.global_launcher;
    click(&mut state, launcher);
    let frame = state.compose(106, 30).unwrap();
    assert!(frame_rows(&frame)
        .iter()
        .any(|row| row.contains("new collection")));
    let row = frame_rows(&frame)
        .iter()
        .position(|row| row.contains("new collection"))
        .unwrap() as u16;
    let item = state
        .hits
        .global_menu_rows
        .iter()
        .find(|(rect, _)| rect.y == row)
        .expect("New collection has a clickable menu row")
        .0;
    click(&mut state, item);
    assert!(state.handle_input_bytes(b"discard me").actions.is_empty());
    let cancel = state.handle_input_bytes(b"\x1b");
    assert!(cancel.actions.is_empty() && cancel.requests.is_empty());
    assert!(state.overlay.is_none());
    click(&mut state, launcher);
    state.compose(106, 30).unwrap();
    click(&mut state, item);
    let typing = state.handle_input_bytes(b"Agent workshop");
    assert!(typing.actions.is_empty() && typing.requests.is_empty());
    let create = state.handle_input_bytes(b"\r");
    let [ClientShellAction::Endpoint { request, .. }] = &create.actions[..] else {
        panic!("collection creation uses the public endpoint command lane");
    };
    assert_eq!(
        serde_json::to_value(&request.method).unwrap(),
        serde_json::json!({
            "method": "collection.create", "params": {"name": "Agent workshop"}
        })
    );
}

#[test]
fn mc_s1_confirmed_empty_collection_is_visible_without_a_new_core_snapshot() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.cache_endpoint_snapshot_for_generation(&ClientEndpointId::Local, 1, Box::new(snapshot()));
    assert!(state.activate_endpoint_projection(&ClientEndpointId::Local));
    state.set_pane_surface(surface());
    state.set_endpoint_methods(Some(vec![
        "organization.get".into(),
        "collection.create".into(),
        "collection.assign_family".into(),
    ]));
    state.set_endpoint_organization_supported(&ClientEndpointId::Local, true);
    let control = crate::client::endpoint::decode_endpoint_control(
        "endpoint.organization.v1",
        r#"{
        "boot_id":"boot-1", "organization":{"revision":1,"collections":[
            {"id":"collection_1","name":"Agent workshop","order":0,"hibernating":false}
        ]}
    }"#,
    )
    .unwrap();
    let crate::client::endpoint::EndpointControlMessage::Organization(catalog) = control else {
        panic!("organization is an optional companion control");
    };
    assert!(state.set_endpoint_organization_for_generation(&ClientEndpointId::Local, 1, catalog));
    let frame = state.compose(106, 30).unwrap();
    let rows = frame_rows(&frame);
    assert!(rows.iter().any(|row| row.contains("Agent workshop")));
    assert!(rows.iter().any(|row| row.contains("Uncollected")));
    assert!(rows.iter().any(|row| row.contains("client-shell")));
    assert_eq!(state.snapshot.as_ref().unwrap().revision, 1);
}

#[test]
fn mc_s1_move_family_menu_uses_a_captured_collection_id_and_waits_for_confirmation() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.cache_endpoint_snapshot_for_generation(&ClientEndpointId::Local, 1, Box::new(snapshot()));
    state.activate_endpoint_projection(&ClientEndpointId::Local);
    state.set_pane_surface(surface());
    state.set_endpoint_methods(Some(vec![
        "organization.get".into(),
        "collection.create".into(),
        "collection.assign_family".into(),
    ]));
    state.set_endpoint_organization_supported(&ClientEndpointId::Local, true);
    let catalog = serde_json::from_value(serde_json::json!({"boot_id":"boot-1", "organization": {
        "revision":2,"collections":[
            {"id":"collection_1","name":"Agent workshop","order":0,"hibernating":false},
            {"id":"collection_2","name":"Agent workshop","order":1,"hibernating":false}
        ], "family_assignments":[]
    }}))
    .unwrap();
    state.set_endpoint_organization_for_generation(&ClientEndpointId::Local, 1, catalog);
    state.compose(106, 30).unwrap();
    let workspace = state.hits.workspaces[0].rect;
    state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column: workspace.x,
        row: workspace.y,
        modifiers: KeyModifiers::empty(),
    })]);
    let frame = state.compose(106, 30).unwrap();
    let move_row = frame_rows(&frame)
        .iter()
        .position(|row| row.contains("Move family to collection"))
        .expect("family move menu") as u16;
    let item = state
        .hits
        .context_menu_rows
        .iter()
        .find(|(rect, _)| rect.y == move_row)
        .unwrap()
        .0;
    click(&mut state, item);
    let frame = state.compose(106, 30).unwrap();
    assert!(
        frame_rows(&frame)
            .iter()
            .any(|row| row.contains("collection_2")),
        "duplicate names have distinct identities in the picker"
    );
    let chosen = state.hits.context_menu_rows[1].0;
    let outcome = click(&mut state, chosen);
    let [ClientShellAction::Endpoint { request, .. }] = &outcome.actions[..] else {
        panic!("family assignment is a JSON command");
    };
    assert_eq!(
        serde_json::to_value(&request.method).unwrap(),
        serde_json::json!({
            "method":"collection.assign_family", "params":{
                "family_id":{"kind":"standalone","workspace_id":"ws_1"},"collection_id":"collection_2"
            }
        })
    );
    assert!(state.overlay.is_none());
    let frame = state.compose(106, 30).unwrap();
    assert!(frame_rows(&frame)
        .iter()
        .any(|row| row.contains("Uncollected")));
}

#[test]
fn mc_s1_collection_collapse_hides_the_whole_family_only_for_that_client() {
    let mut projected = snapshot();
    projected.workspaces[0].worktree = Some(ClientShellWorktree {
        key: "repo-key".into(),
        label: "repo".into(),
        is_linked_worktree: false,
    });
    for (id, label) in [("ws_2", "first-child"), ("ws_3", "second-child")] {
        let mut workspace = projected.workspaces[0].clone();
        workspace.workspace_id = id.into();
        workspace.label = label.into();
        workspace.focused = false;
        workspace.worktree.as_mut().unwrap().is_linked_worktree = true;
        projected.workspaces.push(workspace);
    }
    let catalog: crate::protocol::endpoint::EndpointOrganizationCatalog = serde_json::from_value(serde_json::json!({"boot_id":"boot-1","organization":{
        "revision":2,"collections":[{"id":"collection_1","name":"Agent workshop","order":0,"hibernating":false}],
        "family_assignments":[{"family_id":{"kind":"managed","key":"repo-key"},"collection_id":"collection_1"}]
    }})).unwrap();
    let prepare = |config| {
        let mut state = ClientShellState::new(config);
        state.cache_endpoint_snapshot_for_generation(
            &ClientEndpointId::Local,
            1,
            Box::new(projected.clone()),
        );
        state.activate_endpoint_projection(&ClientEndpointId::Local);
        state.set_pane_surface(surface());
        state.set_endpoint_organization_supported(&ClientEndpointId::Local, true);
        state.set_endpoint_methods(Some(vec![
            "organization.get".into(),
            "collection.create".into(),
            "collection.assign_family".into(),
        ]));
        state.set_endpoint_organization_for_generation(
            &ClientEndpointId::Local,
            1,
            catalog.clone(),
        );
        state
    };
    let path =
        std::env::temp_dir().join(format!("herdr-mc-s1-collapse-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut config = ClientShellConfig::from_config(&Config::default());
    config.preferences_path = Some(path.clone());
    let mut first = prepare(config);
    let mut second = prepare(ClientShellConfig::from_config(&Config::default()));
    let frame = first.compose(106, 40).unwrap();
    let rows = frame_rows(&frame);
    let header = rows
        .iter()
        .position(|row| row.contains("Agent workshop"))
        .unwrap() as u16;
    let uncollected = rows
        .iter()
        .position(|row| row.contains("Uncollected"))
        .unwrap() as u16;
    assert!(
        first
            .hits
            .workspaces
            .iter()
            .all(|hit| hit.rect.y > header && hit.rect.y < uncollected),
        "parent and both children render in their collection"
    );
    assert_eq!(first.hits.workspaces.len(), 3);
    let toggle = Rect::new(first.hits.workspace_body.x + 1, header, 1, 1);
    let collapse = click(&mut first, toggle);
    assert!(collapse.actions.is_empty() && collapse.requests.is_empty());
    let frame = first.compose(106, 40).unwrap();
    assert!(frame_rows(&frame)
        .iter()
        .any(|row| row.contains("▸ Agent workshop")));
    assert!(first.hits.workspaces.is_empty());
    assert_eq!(
        first.snapshot.as_ref().unwrap().focused_pane_id.as_deref(),
        Some("pane_1")
    );
    second.compose(106, 40).unwrap();
    assert_eq!(second.hits.workspaces.len(), 3);
    let mut config = ClientShellConfig::from_config(&Config::default());
    config.preferences =
        super::super::preferences::load(&path).expect("collapse preferences saved");
    let mut restored = prepare(config);
    restored.compose(106, 40).unwrap();
    assert!(restored.hits.workspaces.is_empty());
    let expand = click(&mut first, toggle);
    assert!(expand.actions.is_empty() && expand.requests.is_empty());
    first.compose(106, 40).unwrap();
    assert_eq!(first.hits.workspaces.len(), 3);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn mc_s1_stale_organization_is_rejected_and_disconnect_retires_confirmed_metadata() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.cache_endpoint_snapshot_for_generation(&ClientEndpointId::Local, 1, Box::new(snapshot()));
    state.activate_endpoint_projection(&ClientEndpointId::Local);
    state.set_pane_surface(surface());
    state.set_endpoint_organization_supported(&ClientEndpointId::Local, true);
    assert!(state.set_endpoint_organization_for_generation(
        &ClientEndpointId::Local,
        1,
        organization_catalog("boot-1", 9, "Old boot")
    ));
    assert!(!state.set_endpoint_organization_for_generation(
        &ClientEndpointId::Local,
        2,
        organization_catalog("boot-2", 1, "New boot")
    ));
    let mut next = snapshot();
    next.boot_id = "boot-2".into();
    state.cache_endpoint_snapshot_for_generation(&ClientEndpointId::Local, 2, Box::new(next));
    state.activate_endpoint_projection(&ClientEndpointId::Local);
    let mut next_surface = surface();
    next_surface.boot_id = "boot-2".into();
    state.set_pane_surface(next_surface);
    assert!(!state.set_endpoint_organization_for_generation(
        &ClientEndpointId::Local,
        1,
        organization_catalog("boot-1", 99, "Retired generation")
    ));
    assert!(!state.set_endpoint_organization_for_generation(
        &ClientEndpointId::Local,
        2,
        organization_catalog("boot-1", 99, "Retired boot")
    ));
    assert!(state.set_endpoint_organization_for_generation(
        &ClientEndpointId::Local,
        2,
        organization_catalog("boot-2", 3, "Newest catalog")
    ));
    let frame = state.compose(106, 30).unwrap();
    let text = frame_rows(&frame).join("\n");
    assert!(
        text.contains("Newest catalog") && !text.contains("Retired") && !text.contains("Old boot")
    );
    state.mark_endpoint_disconnected(&ClientEndpointId::Local);
    state.set_endpoint_status(&ClientEndpointId::Local, ClientEndpointStatus::Online);
    let frame = state.compose(106, 30).unwrap();
    assert!(
        !frame_rows(&frame).join("\n").contains("Newest catalog"),
        "reconnect must obtain a fresh confirmed catalog"
    );
}

#[test]
fn mc_s1_legacy_endpoint_keeps_typing_and_its_notice_until_explicit_dismissal() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(snapshot()));
    state.set_pane_surface(surface());
    state.compose(106, 30).unwrap();
    let launcher = state.hits.global_launcher;
    click(&mut state, launcher);
    let frame = state.compose(106, 30).unwrap();
    let row = frame_rows(&frame)
        .iter()
        .position(|row| row.contains("new collection"))
        .unwrap() as u16;
    let entry = state
        .hits
        .global_menu_rows
        .iter()
        .find(|(rect, _)| rect.y == row)
        .unwrap()
        .0;
    let unavailable = click(&mut state, entry);
    assert!(unavailable.actions.is_empty() && unavailable.requests.is_empty());
    assert!(state.overlay.is_none());
    let typing = state.handle_input_bytes(b"echo ready\r");
    assert!(typing.requests.iter().any(|request|matches!(request,ClientMessage::ClientShellPaneInput {pane_id,..} if pane_id=="pane_1")));
    state.tick_notifications(std::time::Instant::now() + std::time::Duration::from_secs(60));
    let frame = state.compose(106, 30).unwrap();
    assert!(
        frame_rows(&frame).join("\n").contains("Action unavailable"),
        "typing and elapsed time do not dismiss an organization notice"
    );
    let remote = crate::client::endpoint::SavedSshEndpoint {
        id: crate::client::endpoint::ProfileId::parse("0123456789abcdef0123456789abcdef").unwrap(),
        label: "Build".into(),
        target: "dev@build.example".into(),
        session: "agents".into(),
        enabled: true,
    };
    let remote_id = ClientEndpointId::Ssh(remote.id.clone());
    state.set_endpoint_catalog(&[remote]);
    state.set_endpoint_status(&remote_id, ClientEndpointStatus::Online);
    let mut remote_snapshot = snapshot();
    remote_snapshot.boot_id = "remote-boot".into();
    state.cache_endpoint_snapshot_for_generation(&remote_id, 1, Box::new(remote_snapshot));
    assert!(state.activate_endpoint_projection(&remote_id));
    let mut remote_surface = surface();
    remote_surface.boot_id = "remote-boot".into();
    state.set_pane_surface(remote_surface);
    let frame = state.compose(106, 30).unwrap();
    assert!(
        !frame_rows(&frame).join("\n").contains("Action unavailable"),
        "notice belongs to its originating endpoint"
    );
    state.activate_endpoint_projection(&ClientEndpointId::Local);
    state.set_pane_surface(surface());
    let frame = state.compose(106, 30).unwrap();
    assert!(frame_rows(&frame).join("\n").contains("Action unavailable"));
    let notice = state.hits.notification_toast;
    click(&mut state, notice);
    let frame = state.compose(106, 30).unwrap();
    assert!(!frame_rows(&frame).join("\n").contains("Action unavailable"));
}

#[test]
fn mc_s1_keyboard_navigation_uses_the_same_visible_collection_rows_as_the_mouse() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    let mut projected = snapshot();
    for (id, label) in [("ws_2", "hidden-family"), ("ws_3", "visible-family")] {
        let mut workspace = projected.workspaces[0].clone();
        workspace.workspace_id = id.into();
        workspace.label = label.into();
        workspace.focused = false;
        projected.workspaces.push(workspace);
    }
    state.cache_endpoint_snapshot_for_generation(&ClientEndpointId::Local, 1, Box::new(projected));
    state.activate_endpoint_projection(&ClientEndpointId::Local);
    state.set_pane_surface(surface());
    state.set_endpoint_organization_supported(&ClientEndpointId::Local, true);
    state.set_endpoint_methods(Some(vec![
        "organization.get".into(),
        "collection.create".into(),
        "collection.assign_family".into(),
        "workspace.focus".into(),
    ]));
    let catalog=serde_json::from_value(serde_json::json!({"boot_id":"boot-1","organization":{"revision":3,
        "collections":[{"id":"collection_1","name":"Agent workshop","order":0,"hibernating":false}],
        "family_assignments":[
            {"family_id":{"kind":"standalone","workspace_id":"ws_1"},"collection_id":"collection_1"},
            {"family_id":{"kind":"standalone","workspace_id":"ws_2"},"collection_id":"collection_1"}
        ]}})).unwrap();
    state.set_endpoint_organization_for_generation(&ClientEndpointId::Local, 1, catalog);
    let frame = state.compose(106, 30).unwrap();
    let header = frame_rows(&frame)
        .iter()
        .position(|row| row.contains("Agent workshop"))
        .unwrap() as u16;
    let toggle = Rect::new(state.hits.workspace_body.x + 1, header, 1, 1);
    click(&mut state, toggle);
    state.compose(106, 30).unwrap();
    assert_eq!(
        state
            .hits
            .workspaces
            .iter()
            .map(|hit| hit.workspace_id.as_str())
            .collect::<Vec<_>>(),
        vec!["ws_3"]
    );
    state.handle_input_bytes(&[0x02]);
    state.handle_input_bytes(b"w");
    let preview = state.handle_input_bytes(b"\x1b[B");
    assert!(preview.requests.is_empty() && preview.actions.is_empty());
    let activate = state.handle_input_bytes(b"\r");
    let [ClientShellAction::Endpoint { request, .. }] = &activate.actions[..] else {
        panic!("visible workspace navigation uses the public API");
    };
    assert!(
        matches!(&request.method,crate::api::schema::Method::WorkspaceFocus(target) if target.workspace_id=="ws_3")
    );
}

#[test]
fn mc_s1_selected_endpoint_catalog_is_visible_beside_a_legacy_endpoint() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.cache_endpoint_snapshot_for_generation(&ClientEndpointId::Local, 1, Box::new(snapshot()));
    state.activate_endpoint_projection(&ClientEndpointId::Local);
    state.set_pane_surface(surface());
    state.set_endpoint_organization_supported(&ClientEndpointId::Local, true);
    let mut catalog = organization_catalog("boot-1", 2, "Agent workshop");
    catalog
        .organization
        .family_assignments
        .push(crate::organization::FamilyAssignment {
            family_id: crate::organization::FamilyId::Standalone {
                workspace_id: "ws_1".into(),
            },
            collection_id: crate::organization::CollectionId("collection_1".into()),
        });
    state.set_endpoint_organization_for_generation(&ClientEndpointId::Local, 1, catalog);
    let remote = crate::client::endpoint::SavedSshEndpoint {
        id: crate::client::endpoint::ProfileId::parse("0123456789abcdef0123456789abcdef").unwrap(),
        label: "Legacy".into(),
        target: "dev@legacy.example".into(),
        session: "agents".into(),
        enabled: true,
    };
    let remote_id = ClientEndpointId::Ssh(remote.id.clone());
    state.set_endpoint_catalog(&[remote]);
    let mut remote_snapshot = snapshot();
    remote_snapshot.boot_id = "remote-boot".into();
    state.set_endpoint_status(&remote_id, ClientEndpointStatus::Online);
    state.cache_endpoint_snapshot_for_generation(&remote_id, 1, Box::new(remote_snapshot));
    let frame = state.compose(106, 40).unwrap();
    let rows = frame_rows(&frame);
    let header = rows
        .iter()
        .position(|row| row.contains("Agent workshop"))
        .expect("active endpoint collection is visible") as u16;
    assert!(rows.iter().any(|row| row.contains("Legacy")));
    let rect = state
        .hits
        .collections
        .iter()
        .find(|(_, endpoint, _)| endpoint == &ClientEndpointId::Local)
        .unwrap()
        .0;
    assert_eq!(rect.y, header);
    click(&mut state, rect);
    state.compose(106, 40).unwrap();
    assert!(state
        .hits
        .workspaces
        .iter()
        .all(|hit| hit.endpoint_id == remote_id));
    assert_eq!(state.active_endpoint_id, ClientEndpointId::Local);
}

#[test]
fn mc_s1_create_shows_pending_state_until_the_server_response() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(snapshot()));
    state.set_pane_surface(surface());
    state.set_endpoint_organization_supported(&ClientEndpointId::Local, true);
    state.set_endpoint_methods(Some(vec![
        "organization.get".into(),
        "collection.create".into(),
        "collection.assign_family".into(),
    ]));
    state.compose(106, 30).unwrap();
    let launcher = state.hits.global_launcher;
    click(&mut state, launcher);
    let frame = state.compose(106, 30).unwrap();
    let row = frame_rows(&frame)
        .iter()
        .position(|row| row.contains("new collection"))
        .unwrap() as u16;
    let item = state
        .hits
        .global_menu_rows
        .iter()
        .find(|(rect, _)| rect.y == row)
        .unwrap()
        .0;
    click(&mut state, item);
    state.handle_input_bytes(b"Agent workshop");
    let create = state.handle_input_bytes(b"\r");
    let [ClientShellAction::Endpoint { request, .. }] = &create.actions[..] else {
        panic!("collection request");
    };
    let request_id = request.id.clone();
    let frame = state.compose(106, 30).unwrap();
    assert!(frame_rows(&frame)
        .join("\n")
        .contains("creating collection"));
    state.handle_endpoint_result(
        "boot-1",
        &request_id,
        Err(ClientShellEndpointError {
            code: Some("invalid_collection_name".into()),
            message: "Name required".into(),
        }),
    );
    let frame = state.compose(106, 30).unwrap();
    let text = frame_rows(&frame).join("\n");
    assert!(!text.contains("creating collection"));
    assert!(text.contains("Action rejected"));
}

#[test]
fn mc_s1_stale_response_does_not_cancel_the_current_boots_collection_request() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(snapshot()));
    state.set_pane_surface(surface());
    state.set_endpoint_methods(Some(vec!["collection.create".into()]));
    let mut outcome = ClientShellInput::default();
    state.push_endpoint_method(
        crate::api::schema::Method::CollectionCreate(crate::api::schema::CollectionCreateParams {
            name: "Agent workshop".into(),
        }),
        &mut outcome,
    );
    let [ClientShellAction::Endpoint { request, .. }] = &outcome.actions[..] else {
        panic!("collection request");
    };
    let request_id = request.id.clone();
    let stale = state.handle_endpoint_result(
        "earlier-boot",
        &request_id,
        Ok(crate::api::schema::ResponseResult::Ok {}),
    );
    assert!(!stale.0 && stale.1.is_empty());
    let frame = state.compose(106, 30).unwrap();
    assert!(frame_rows(&frame)
        .join("\n")
        .contains("creating collection"));
    state.handle_endpoint_result(
        "boot-1",
        &request_id,
        Err(ClientShellEndpointError {
            code: Some("collection_not_found".into()),
            message: "Rejected by current server".into(),
        }),
    );
    let frame = state.compose(106, 30).unwrap();
    assert!(frame_rows(&frame).join("\n").contains("Action rejected"));
}

#[test]
fn mc_s1_initial_catalog_keeps_the_newest_revision_until_its_snapshot_arrives() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_endpoint_organization_supported(&ClientEndpointId::Local, true);
    state.set_endpoint_organization_for_generation(
        &ClientEndpointId::Local,
        1,
        organization_catalog("boot-1", 5, "Newest pending"),
    );
    state.set_endpoint_organization_for_generation(
        &ClientEndpointId::Local,
        1,
        organization_catalog("boot-1", 3, "Older pending"),
    );
    state.cache_endpoint_snapshot_for_generation(&ClientEndpointId::Local, 1, Box::new(snapshot()));
    state.activate_endpoint_projection(&ClientEndpointId::Local);
    state.set_pane_surface(surface());
    let frame = state.compose(106, 30).unwrap();
    let text = frame_rows(&frame).join("\n");
    assert!(text.contains("Newest pending") && !text.contains("Older pending"));
}

#[test]
fn mc_s1_confirmed_create_repaints_a_silent_client_and_applies_the_confirmed_catalog() {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.cache_endpoint_snapshot_for_generation(&ClientEndpointId::Local, 1, Box::new(snapshot()));
    state.activate_endpoint_projection(&ClientEndpointId::Local);
    state.set_pane_surface(surface());
    state.set_endpoint_organization_supported(&ClientEndpointId::Local, true);
    state.set_endpoint_methods(Some(vec![
        "collection.create".into(),
        "organization.get".into(),
        "collection.assign_family".into(),
    ]));
    let mut outcome = ClientShellInput::default();
    state.push_endpoint_method(
        crate::api::schema::Method::CollectionCreate(crate::api::schema::CollectionCreateParams {
            name: "Agent workshop".into(),
        }),
        &mut outcome,
    );
    let [ClientShellAction::Endpoint { request, .. }] = &outcome.actions[..] else {
        panic!("collection request");
    };
    let catalog = organization_catalog("boot-1", 1, "Agent workshop");
    let response = state.handle_endpoint_result(
        "boot-1",
        &request.id,
        Ok(crate::api::schema::ResponseResult::CollectionCreated {
            collection: catalog.organization.collections[0].clone(),
            organization: catalog.organization,
        }),
    );
    assert!(
        response.0,
        "confirmed creation must repaint without waiting for terminal output"
    );
    assert!(response.1.is_empty());
    let frame = state.compose(106, 30).unwrap();
    let text = frame_rows(&frame).join("\n");
    assert!(text.contains("Agent workshop") && !text.contains("creating collection"));
}

fn hibernate_client(config: ClientShellConfig) -> ClientShellState {
    let mut state = ClientShellState::new(config);
    let mut projected = snapshot();
    projected.workspaces[0].worktree = Some(ClientShellWorktree {
        key: "repo-key".into(),
        label: "repo".into(),
        is_linked_worktree: false,
    });
    let mut child = projected.workspaces[0].clone();
    child.workspace_id = "ws_2".into();
    child.label = "parked-child".into();
    child.focused = false;
    child.worktree.as_mut().unwrap().is_linked_worktree = true;
    projected.workspaces.push(child);
    state.cache_endpoint_snapshot_for_generation(&ClientEndpointId::Local, 1, Box::new(projected));
    state.activate_endpoint_projection(&ClientEndpointId::Local);
    state.set_pane_surface(surface());
    state.set_endpoint_methods(Some(vec![
        "organization.get".into(),
        "collection.create".into(),
        "collection.assign_family".into(),
        "collection.set_hibernating".into(),
        "pane.focus".into(),
    ]));
    state.set_endpoint_organization_supported(&ClientEndpointId::Local, true);
    let catalog = serde_json::from_value(serde_json::json!({"boot_id":"boot-1", "organization":{
        "revision":3,"collections":[
            {"id":"collection_1","name":"Infrastructure","order":0,"hibernating":true},
            {"id":"collection_2","name":"Daily","order":1,"hibernating":false}],
        "family_assignments":[{"family_id":{"kind":"managed","key":"repo-key"},"collection_id":"collection_1"}]
    }})).unwrap();
    state.set_endpoint_organization_for_generation(&ClientEndpointId::Local, 1, catalog);
    state
}

#[test]
fn mc_s2_hibernate_is_collapsed_and_expands_the_existing_family_without_terminal_actions() {
    let mut state = hibernate_client(ClientShellConfig::from_config(&Config::default()));
    let frame = state.compose(106, 40).unwrap();
    let rows = frame_rows(&frame);
    assert!(rows.iter().any(|row| row.contains("▸ Hibernate")));
    assert!(!rows.iter().any(|row| row.contains("Infrastructure")));
    assert!(state.hits.workspaces.is_empty());
    assert_eq!(
        state.snapshot.as_ref().unwrap().focused_pane_id.as_deref(),
        Some("pane_1")
    );
    let y = rows
        .iter()
        .position(|row| row.contains("Hibernate"))
        .unwrap() as u16;
    let rect = Rect::new(state.hits.workspace_body.x + 1, y, 1, 1);
    let action = click(&mut state, rect);
    assert!(action.actions.is_empty() && action.requests.is_empty());
    let frame = state.compose(106, 40).unwrap();
    let rows = frame_rows(&frame);
    let daily = rows.iter().position(|row| row.contains("Daily")).unwrap();
    let group = rows
        .iter()
        .position(|row| row.contains("▾ Hibernate"))
        .unwrap();
    let parked = rows
        .iter()
        .position(|row| row.contains("Infrastructure"))
        .unwrap();
    assert!(daily < group && group < parked);
    assert_eq!(state.hits.workspaces.len(), 2);
    assert_eq!(
        state
            .hits
            .workspaces
            .iter()
            .map(|hit| hit.workspace_id.as_str())
            .collect::<Vec<_>>(),
        vec!["ws_1", "ws_2"]
    );
    assert!(state.handle_input_bytes(b"typing while parked").requests.iter().any(|request| matches!(request, ClientMessage::ClientShellPaneInput { pane_id, .. } if pane_id == "pane_1")));
}

#[test]
fn mc_s2_hibernate_expansion_is_client_local_and_survives_preference_reload() {
    let path =
        std::env::temp_dir().join(format!("herdr-mc-s2-expansion-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut config = ClientShellConfig::from_config(&Config::default());
    config.preferences_path = Some(path.clone());
    let mut first = hibernate_client(config);
    let mut second = hibernate_client(ClientShellConfig::from_config(&Config::default()));
    first.compose(106, 40).unwrap();
    let rect = first.hits.hibernate[0].0;
    let expand = click(&mut first, rect);
    assert!(expand.actions.is_empty() && expand.requests.is_empty());
    assert!(frame_rows(&first.compose(106, 40).unwrap())
        .join("\n")
        .contains("Infrastructure"));
    assert!(!frame_rows(&second.compose(106, 40).unwrap())
        .join("\n")
        .contains("Infrastructure"));
    let mut config = ClientShellConfig::from_config(&Config::default());
    config.preferences = super::super::preferences::load(&path).unwrap();
    let mut restored = hibernate_client(config);
    assert!(frame_rows(&restored.compose(106, 40).unwrap())
        .join("\n")
        .contains("Infrastructure"));
    click(&mut first, rect);
    let mut config = ClientShellConfig::from_config(&Config::default());
    config.preferences = super::super::preferences::load(&path).unwrap();
    let mut collapsed = hibernate_client(config);
    assert!(!frame_rows(&collapsed.compose(106, 40).unwrap())
        .join("\n")
        .contains("Infrastructure"));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn mc_s2_collection_menu_waits_for_confirmation_and_uses_captured_identity() {
    let mut state = hibernate_client(ClientShellConfig::from_config(&Config::default()));
    state.compose(106, 40).unwrap();
    let group = state.hits.hibernate[0].0;
    click(&mut state, group);
    state.compose(106, 40).unwrap();
    let rect = state
        .hits
        .collections
        .iter()
        .find(|(_, _, id)| id.0 == "collection_1")
        .unwrap()
        .0;
    let open = state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column: rect.x,
        row: rect.y,
        modifiers: KeyModifiers::empty(),
    })]);
    assert!(open.actions.is_empty() && open.requests.is_empty());
    assert!(frame_rows(&state.compose(106, 40).unwrap())
        .join("\n")
        .contains("Bring collection back"));
    let submit = state.handle_input_bytes(b"\r");
    let [ClientShellAction::Endpoint { request, .. }] = &submit.actions[..] else {
        panic!("Hibernate uses JSON command lane");
    };
    assert_eq!(
        serde_json::to_value(&request.method).unwrap(),
        serde_json::json!({
            "method":"collection.set_hibernating", "params":{"collection_id":"collection_1","hibernating":false}
        })
    );
    assert!(frame_rows(&state.compose(106, 40).unwrap())
        .join("\n")
        .contains("updating Hibernate"));
    let mut catalog = state.endpoints[0].organization.clone().unwrap();
    assert!(
        catalog.organization.collections[0].hibernating,
        "pending command does not unpark optimistically"
    );
    catalog.organization.revision = 4;
    catalog.organization.collections[0].hibernating = false;
    state.set_endpoint_organization_for_generation(&ClientEndpointId::Local, 1, catalog);
    state.compose(106, 40).unwrap();
    let rect = state
        .hits
        .collections
        .iter()
        .find(|(_, _, id)| id.0 == "collection_1")
        .unwrap()
        .0;
    state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column: rect.x,
        row: rect.y,
        modifiers: KeyModifiers::empty(),
    })]);
    assert!(frame_rows(&state.compose(106, 40).unwrap())
        .join("\n")
        .contains("Hibernate collection"));
    let submit = state.handle_input_bytes(b"\r");
    let [ClientShellAction::Endpoint { request, .. }] = &submit.actions[..] else {
        panic!("park request");
    };
    assert_eq!(
        serde_json::to_value(&request.method).unwrap()["params"]["hibernating"],
        true
    );
}

#[test]
fn mc_s2_explicit_pane_jump_reveals_parked_collection_and_family_without_unparking() {
    let mut state = hibernate_client(ClientShellConfig::from_config(&Config::default()));
    state.compose(106, 40).unwrap();
    let rect = state.hits.hibernate[0].0;
    click(&mut state, rect);
    state.compose(106, 40).unwrap();
    let rect = state
        .hits
        .collections
        .iter()
        .find(|(_, _, id)| id.0 == "collection_1")
        .unwrap()
        .0;
    click(&mut state, rect);
    state.compose(106, 40).unwrap();
    let rect = state.hits.hibernate[0].0;
    click(&mut state, rect);
    let mut action = ClientShellInput::default();
    assert!(state.focus_or_activate(
        ClientEndpointId::Local,
        ClientEndpointFocusTarget::Pane("pane_1".into()),
        &mut action
    ));
    let frame = state.compose(106, 40).unwrap();
    assert!(frame_rows(&frame).join("\n").contains("▾ Hibernate"));
    assert_eq!(state.hits.workspaces.len(), 2);
    assert!(
        state.endpoints[0]
            .organization
            .as_ref()
            .unwrap()
            .organization
            .collections[0]
            .hibernating
    );
    let [ClientShellAction::Endpoint { request, .. }] = &action.actions[..] else {
        panic!("only focus request");
    };
    assert_eq!(
        serde_json::to_value(&request.method).unwrap(),
        serde_json::json!({"method":"pane.focus","params":{"pane_id":"pane_1"}})
    );
}

#[test]
fn mc_s2_hibernate_requires_its_advertised_method_and_keeps_mc_s1_actions_available() {
    for methods in [
        None,
        Some(vec![
            "organization.get".into(),
            "collection.create".into(),
            "collection.assign_family".into(),
        ]),
    ] {
        let mut state = hibernate_client(ClientShellConfig::from_config(&Config::default()));
        state.set_endpoint_methods(methods.clone());
        state.compose(106, 40).unwrap();
        let group = state.hits.hibernate[0].0;
        click(&mut state, group);
        state.compose(106, 40).unwrap();
        let rect = state
            .hits
            .collections
            .iter()
            .find(|(_, _, id)| id.0 == "collection_1")
            .unwrap()
            .0;
        state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Right),
            column: rect.x,
            row: rect.y,
            modifiers: KeyModifiers::empty(),
        })]);
        let unavailable = state.handle_input_bytes(b"\r");
        assert!(unavailable.actions.is_empty() && unavailable.requests.is_empty());
        assert!(frame_rows(&state.compose(106, 40).unwrap())
            .join("\n")
            .contains("Action unavailable"));
        assert!(state.handle_input_bytes(b"still typing").requests.iter().any(|request|
            matches!(request, ClientMessage::ClientShellPaneInput { pane_id, .. } if pane_id == "pane_1")));
        assert!(frame_rows(&state.compose(106, 40).unwrap())
            .join("\n")
            .contains("Action unavailable"));
        if methods.is_some() {
            let mut outcome = ClientShellInput::default();
            state.open_new_collection(&mut outcome);
            assert!(matches!(state.overlay, Some(ClientShellOverlay::Rename(_))));
            state.handle_input_bytes(b"Another");
            let submit = state.handle_input_bytes(b"\r");
            assert!(submit.actions.iter().any(|action| matches!(action,
                ClientShellAction::Endpoint { request, .. } if matches!(request.method, crate::api::schema::Method::CollectionCreate(_)))));
        }
    }
}

#[test]
fn mc_s2_multi_endpoint_hibernate_rows_and_hits_stay_inside_the_sidebar() {
    let mut state = hibernate_client(ClientShellConfig::from_config(&Config::default()));
    let remote = crate::client::endpoint::SavedSshEndpoint {
        id: crate::client::endpoint::ProfileId::parse("0123456789abcdef0123456789abcdef").unwrap(),
        label: "Legacy".into(),
        target: "dev@legacy.example".into(),
        session: "agents".into(),
        enabled: true,
    };
    let remote_id = ClientEndpointId::Ssh(remote.id.clone());
    state.set_endpoint_catalog(&[remote]);
    let mut projected = snapshot();
    projected.boot_id = "remote-boot".into();
    state.set_endpoint_status(&remote_id, ClientEndpointStatus::Online);
    state.cache_endpoint_snapshot_for_generation(&remote_id, 1, Box::new(projected));
    let text = frame_rows(&state.compose(106, 40).unwrap()).join("\n");
    assert!(
        text.contains("▸ Hibernate") && text.contains("Legacy") && !text.contains("Infrastructure")
    );
    let rect = state.hits.hibernate[0].0;
    click(&mut state, rect);
    let text = frame_rows(&state.compose(106, 40).unwrap()).join("\n");
    assert!(text.contains("Infrastructure"));
    assert_eq!(
        state
            .hits
            .workspaces
            .iter()
            .filter(|hit| hit.endpoint_id == ClientEndpointId::Local)
            .count(),
        2
    );
    click(&mut state, rect);
    for height in 5..=25 {
        state.compose(106, height).unwrap();
        assert!(
            state
                .hits
                .hibernate
                .iter()
                .all(|(rect, _)| rect.y >= state.hits.workspace_body.y
                    && rect.bottom() <= state.hits.workspace_body.bottom()),
            "Hibernate hits must be clipped at height {height}"
        );
    }
}

fn mission_client() -> ClientShellState {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.cache_endpoint_snapshot_for_generation(&ClientEndpointId::Local, 1, Box::new(snapshot()));
    state.activate_endpoint_projection(&ClientEndpointId::Local);
    state.set_pane_surface(surface());
    state.set_endpoint_organization_supported(&ClientEndpointId::Local, true);
    state.set_endpoint_methods(Some(vec![
        "organization.get".into(),
        "mission.create".into(),
        "mission.assign".into(),
        "collection.create".into(),
        "collection.assign_family".into(),
    ]));
    state.set_endpoint_organization_for_generation(&ClientEndpointId::Local,1,
        serde_json::from_value(serde_json::json!({"boot_id":"boot-1","organization":{"revision":1,"collections":[],"missions":[{"id":"mission_1","name":"Empty mission","order":0}]}})).unwrap());
    state
}
fn click_menu_label(state: &mut ClientShellState, label: &str, global: bool) -> ClientShellInput {
    let frame = state.compose(106, 30).unwrap();
    let row = frame_rows(&frame)
        .iter()
        .position(|row| row.contains(label))
        .unwrap_or_else(|| panic!("missing {label}: {:?}", frame_rows(&frame)))
        as u16;
    let hits = if global {
        &state.hits.global_menu_rows
    } else {
        &state.hits.context_menu_rows
    };
    let rect = hits
        .iter()
        .find(|(rect, _)| rect.y == row)
        .expect("clickable menu item")
        .0;
    click(state, rect)
}
#[test]
fn mc_s3_missions_menu_lists_empty_definitions_and_creates_without_terminal_input() {
    let mut state = mission_client();
    state.compose(106, 30).unwrap();
    let launcher = state.hits.global_launcher;
    click(&mut state, launcher);
    click_menu_label(&mut state, "Missions", true);
    let frame = state.compose(106, 30).unwrap();
    assert!(frame_rows(&frame).join("\n").contains("Empty mission"));
    assert!(frame_rows(&frame).join("\n").contains("0 tabs"));
    click_menu_label(&mut state, "New mission", false);
    assert!(state
        .handle_input_bytes(b"Tako platform")
        .actions
        .is_empty());
    let created = state.handle_input_bytes(b"\r");
    let [ClientShellAction::Endpoint { request, .. }] = &created.actions[..] else {
        panic!("mission creation uses JSON command lane")
    };
    assert_eq!(
        serde_json::to_value(&request.method).unwrap(),
        serde_json::json!({"method":"mission.create","params":{"name":"Tako platform"}})
    );
}

#[test]
fn mc_s3_tab_picker_assigns_without_focus_and_indicator_waits_for_confirmation() {
    let mut state = mission_client();
    let frame = state.compose(106, 30).unwrap();
    let tab = state.hits.tabs[0].0;
    state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column: tab.x,
        row: tab.y,
        modifiers: KeyModifiers::empty(),
    })]);
    let opened = click_menu_label(&mut state, "Add to mission", false);
    assert!(
        opened.actions.is_empty(),
        "opening assignment must not focus the tab"
    );
    let selected = click_menu_label(&mut state, "Empty mission", false);
    let [ClientShellAction::Endpoint { request, .. }] = &selected.actions[..] else {
        panic!("one assignment, no focus command")
    };
    assert_eq!(
        serde_json::to_value(&request.method).unwrap(),
        serde_json::json!({"method":"mission.assign","params":{"target":{"kind":"tab","tab_id":"tab_1"},"mission_id":"mission_1"}})
    );
    assert!(!frame_rows(&state.compose(106, 30).unwrap())
        .join("\n")
        .contains("◆ Empty mission"));
    state.set_endpoint_organization_for_generation(&ClientEndpointId::Local,1,serde_json::from_value(serde_json::json!({"boot_id":"boot-1","organization":{"revision":2,"collections":[],"missions":[{"id":"mission_1","name":"Empty mission","order":0}],"mission_assignments":[{"mission_id":"mission_1","target":{"kind":"tab","tab_id":"tab_1"}}]}})).unwrap());
    assert!(frame_rows(&state.compose(106, 30).unwrap())
        .join("\n")
        .contains("◆ Empty mission"));
    assert_eq!(
        state.snapshot.as_ref().unwrap().focused_pane_id.as_deref(),
        Some("pane_1")
    );
    assert!(frame_rows(&frame).join("\n").contains("LIVE"));
}

#[test]
fn mc_s3_stale_picker_and_missing_mission_advertisement_leave_terminal_usable() {
    let mut state = mission_client();
    state.compose(106, 30).unwrap();
    let tab = state.hits.tabs[0].0;
    state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column: tab.x,
        row: tab.y,
        modifiers: KeyModifiers::empty(),
    })]);
    click_menu_label(&mut state, "Add to mission", false);
    let frame = state.compose(106, 30).unwrap();
    let mut next = snapshot();
    next.revision = 2;
    next.tabs[0].tab_id = "replacement_tab".into();
    next.workspaces[0].active_tab_id = "replacement_tab".into();
    next.focused_tab_id = Some("replacement_tab".into());
    next.panes[0].tab_id = "replacement_tab".into();
    state.cache_endpoint_snapshot_for_generation(&ClientEndpointId::Local, 1, Box::new(next));
    state.activate_endpoint_projection(&ClientEndpointId::Local);
    let mut pane = surface();
    pane.projection_revision = 2;
    state.set_pane_surface(pane);
    assert!(state.handle_input_bytes(b"\r").actions.is_empty());
    assert!(frame_rows(&state.compose(106, 30).unwrap())
        .join("\n")
        .contains("unavailable"));
    assert!(frame_rows(&frame).join("\n").contains("Empty mission"));
    let mut legacy = mission_client();
    legacy.set_endpoint_methods(Some(vec![
        "organization.get".into(),
        "collection.create".into(),
        "collection.assign_family".into(),
    ]));
    legacy.compose(106, 30).unwrap();
    let launcher = legacy.hits.global_launcher;
    click(&mut legacy, launcher);
    assert!(click_menu_label(&mut legacy, "Missions", true)
        .actions
        .is_empty());
    let typing = legacy.handle_input_bytes(b"echo preserved");
    assert!(typing.requests.iter().any(|request| matches!(request,ClientMessage::ClientShellPaneInput {pane_id,..} if pane_id == "pane_1")),"typing still goes to the terminal");
    assert!(frame_rows(&legacy.compose(106, 30).unwrap())
        .join("\n")
        .contains("unavailable"));
    click(&mut legacy, launcher);
    click_menu_label(&mut legacy, "new collection", true);
    legacy.handle_input_bytes(b"Still supported");
    assert!(
        matches!(&legacy.handle_input_bytes(b"\r").actions[..],[ClientShellAction::Endpoint {request,..}] if matches!(request.method,crate::api::schema::Method::CollectionCreate(_)))
    );
}

#[test]
fn mc_s3_compact_missions_menu_keeps_keyboard_selection_visible() {
    let mut state = mission_client();
    let missions = (0..20).map(|i| serde_json::json!({"id":format!("mission_{i}"),"name":format!("Objective {i}"),"order":i})).collect::<Vec<_>>();
    state.set_endpoint_organization_for_generation(&ClientEndpointId::Local,1,serde_json::from_value(serde_json::json!({"boot_id":"boot-1","organization":{"revision":2,"collections":[],"missions":missions}})).unwrap());
    state.compose(106, 30).unwrap();
    let launcher = state.hits.global_launcher;
    click(&mut state, launcher);
    click_menu_label(&mut state, "Missions", true);
    state.handle_input_bytes("\x1b[B".repeat(20).as_bytes());
    let frame = state.compose(45, 8).unwrap();
    assert!(
        frame_rows(&frame).join("\n").contains("Objective 19"),
        "selected mission must be visible after scrolling"
    );
    assert!(state
        .hits
        .context_menu_rows
        .iter()
        .all(|(rect, _)| rect.bottom() <= 8 && rect.right() <= 45));
}

#[test]
fn mc_s3_mission_failure_notice_persists_through_typing_and_notification_ticks() {
    let mut state = mission_client();
    state.compose(106, 30).unwrap();
    let launcher = state.hits.global_launcher;
    click(&mut state, launcher);
    click_menu_label(&mut state, "Missions", true);
    click_menu_label(&mut state, "New mission", false);
    state.handle_input_bytes(b"Rejected objective");
    let input = state.handle_input_bytes(b"\r");
    let [ClientShellAction::Endpoint { request, .. }] = &input.actions[..] else {
        panic!("mission request")
    };
    state.handle_endpoint_result(
        "boot-1",
        &request.id,
        Err(ClientShellEndpointError {
            code: Some("endpoint_timeout".into()),
            message: "Timed out".into(),
        }),
    );
    state.tick_notifications(std::time::Instant::now() + std::time::Duration::from_secs(60));
    let typing = state.handle_input_bytes(b"still typing");
    assert!(typing.requests.iter().any(
        |r| matches!(r,ClientMessage::ClientShellPaneInput {pane_id,..} if pane_id=="pane_1")
    ));
    assert!(frame_rows(&state.compose(106, 30).unwrap())
        .join("\n")
        .contains("Server timed out"));
}

#[test]
fn mc_s3_mission_form_cannot_submit_into_a_new_connection_generation() {
    let mut state = mission_client();
    state.compose(106, 30).unwrap();
    let launcher = state.hits.global_launcher;
    click(&mut state, launcher);
    click_menu_label(&mut state, "Missions", true);
    click_menu_label(&mut state, "New mission", false);
    state.handle_input_bytes(b"Old connection objective");
    state.cache_endpoint_snapshot_for_generation(&ClientEndpointId::Local, 2, Box::new(snapshot()));
    state.activate_endpoint_projection(&ClientEndpointId::Local);
    state.set_pane_surface(surface());
    let submit = state.handle_input_bytes(b"\r");
    assert!(
        submit.actions.is_empty() && submit.requests.is_empty(),
        "a captured mission form must not write to a newer connection"
    );
    assert!(frame_rows(&state.compose(106, 30).unwrap())
        .join("\n")
        .contains("Action unavailable"));
}

fn mc_s4_agents_snapshot() -> ClientShellSnapshot {
    let mut projected = snapshot();
    for (id, name, status) in [
        ("pane_1", "Agent Alpha", AgentStatus::Working),
        ("pane_2", "Agent Beta", AgentStatus::Blocked),
    ] {
        if id != "pane_1" {
            let mut pane = projected.panes[0].clone();
            pane.pane_id = id.into();
            pane.focused = false;
            projected.panes.push(pane);
        }
        projected.agents.push(ClientShellAgent {
            pane_id: id.into(),
            workspace_id: "ws_1".into(),
            tab_id: "tab_1".into(),
            name: Some(name.into()),
            display_agent: None,
            agent: Some("codex".into()),
            title: None,
            terminal_title: None,
            terminal_title_stripped: None,
            agent_status: status,
            state_change_seq: 2,
            state_labels: vec![],
            tokens: vec![],
            focused: id == "pane_1",
        });
    }
    projected.revision = 2;
    projected
}

#[test]
fn mc_s4_mission_grouping_tracks_overrides_clear_and_destination_inheritance() {
    let mut state = mission_client();
    state.cache_endpoint_snapshot_for_generation(
        &ClientEndpointId::Local,
        1,
        Box::new(mc_s4_agents_snapshot()),
    );
    state.activate_endpoint_projection(&ClientEndpointId::Local);
    state.config.agents.rows = vec![vec![crate::config::AgentSidebarToken::Agent]];
    let mut current_surface = surface();
    current_surface.projection_revision = 2;
    state.set_pane_surface(current_surface);
    state.config.agent_panel_sort = crate::config::AgentPanelSortConfig::Missions;
    let mut catalog: crate::protocol::endpoint::EndpointOrganizationCatalog = serde_json::from_value(serde_json::json!({"boot_id":"boot-1","organization":{
        "revision":2,"collections":[],"missions":[{"id":"runtime","name":"Agent runtime","order":0},{"id":"tako","name":"Tako platform","order":1}],
        "mission_assignments":[{"target":{"kind":"tab","tab_id":"tab_1"},"mission_id":"runtime"}],
        "pane_mission_assignments":[{"pane_id":"pane_2","mission_id":"tako"}]
    }})).unwrap();
    state.set_endpoint_organization_for_generation(&ClientEndpointId::Local, 1, catalog.clone());
    let text = frame_rows(&state.compose(106, 40).unwrap()).join("\n");
    let runtime = text
        .find("◆ Agent runtime")
        .unwrap_or_else(|| panic!("missing group: {text}"));
    let tako = text
        .find("◆ Tako platform")
        .unwrap_or_else(|| panic!("override group: {text}"));
    assert!(text[runtime..tako].contains("Agent Alpha"));
    assert!(text[tako..].contains("Agent Beta"));
    assert_eq!(state.hits.endpoint_agents.len(), 2);
    catalog.organization.revision = 3;
    catalog.organization.pane_mission_assignments.clear();
    state.set_endpoint_organization_for_generation(&ClientEndpointId::Local, 1, catalog);
    let text = frame_rows(&state.compose(106, 40).unwrap()).join("\n");
    assert!(!text.contains("◆ Tako platform"));
    assert!(text.contains("Agent Alpha") && text.contains("Agent Beta"));
    let mut moved = mc_s4_agents_snapshot();
    moved.revision = 3;
    let mut tab = moved.tabs[0].clone();
    tab.tab_id = "destination".into();
    tab.focused = false;
    moved.tabs.push(tab);
    moved.panes[1].tab_id = "destination".into();
    moved.agents[1].tab_id = "destination".into();
    state.cache_endpoint_snapshot_for_generation(&ClientEndpointId::Local, 1, Box::new(moved));
    state.activate_endpoint_projection(&ClientEndpointId::Local);
    let mut current_surface = surface();
    current_surface.projection_revision = 3;
    state.set_pane_surface(current_surface);
    let text = frame_rows(&state.compose(106, 40).unwrap()).join("\n");
    assert!(text[text
        .find("Unassigned")
        .expect("destination inheritance refreshed")..]
        .contains("Agent Beta"));
    let toggle = state.hits.agent_sort_toggle;
    click(&mut state, toggle);
    let text = frame_rows(&state.compose(106, 40).unwrap()).join("\n");
    assert!(text.contains("priority"));
    assert_eq!(state.hits.endpoint_agents[0].2, "pane_2");
}

#[test]
fn mc_s4_agent_menu_assigns_exact_pane_and_clear_explains_inheritance() {
    let mut state = mission_client();
    state.set_endpoint_methods(Some(vec![
        "organization.get".into(),
        "mission.assign_pane".into(),
        "mission.clear_pane_override".into(),
    ]));
    state.cache_endpoint_snapshot_for_generation(
        &ClientEndpointId::Local,
        1,
        Box::new(mc_s4_agents_snapshot()),
    );
    state.activate_endpoint_projection(&ClientEndpointId::Local);
    let mut current_surface = surface();
    current_surface.projection_revision = 2;
    state.set_pane_surface(current_surface);
    state.config.agents.rows = vec![vec![crate::config::AgentSidebarToken::Agent]];
    state.compose(106, 40).unwrap();
    let target = state
        .hits
        .endpoint_agents
        .iter()
        .find(|(_, _, id)| id == "pane_2")
        .unwrap()
        .0;
    let open = state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column: target.x,
        row: target.y,
        modifiers: KeyModifiers::empty(),
    })]);
    assert!(open.actions.is_empty() && open.requests.is_empty());
    click_menu_label(&mut state, "Assign mission", false);
    let selected = click_menu_label(&mut state, "Empty mission", false);
    let [ClientShellAction::Endpoint { request, .. }] = &selected.actions[..] else {
        panic!("one pane assignment and no focus")
    };
    assert_eq!(
        serde_json::to_value(&request.method).unwrap(),
        serde_json::json!({"method":"mission.assign_pane","params":{"pane_id":"pane_2","mission_id":"mission_1"}})
    );
    let mut catalog = state.endpoints[0].organization.clone().unwrap();
    assert!(
        catalog.organization.pane_mission_assignments.is_empty(),
        "confirmed-only membership"
    );
    catalog.organization.revision = 2;
    catalog.organization.pane_mission_assignments.push(
        crate::organization::PaneMissionAssignment {
            pane_id: "pane_2".into(),
            mission_id: crate::organization::MissionId("mission_1".into()),
        },
    );
    state.set_endpoint_organization_for_generation(&ClientEndpointId::Local, 1, catalog);
    state.compose(106, 40).unwrap();
    let target = state
        .hits
        .endpoint_agents
        .iter()
        .find(|(_, _, id)| id == "pane_2")
        .unwrap()
        .0;
    state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column: target.x,
        row: target.y,
        modifiers: KeyModifiers::empty(),
    })]);
    let text = frame_rows(&state.compose(106, 40).unwrap()).join("\n");
    assert!(
        text.contains("override"),
        "menu explains explicit membership"
    );
    let clear = click_menu_label(&mut state, "Clear override", false);
    let [ClientShellAction::Endpoint { request, .. }] = &clear.actions[..] else {
        panic!("one clear command")
    };
    assert_eq!(
        serde_json::to_value(&request.method).unwrap(),
        serde_json::json!({"method":"mission.clear_pane_override","params":{"pane_id":"pane_2"}})
    );
}

#[test]
fn mc_s4_collapsed_agent_order_matches_mission_focus_order() {
    let mut state = mission_client();
    state.set_endpoint_methods(Some(vec!["pane.focus".into(), "organization.get".into()]));
    state.cache_endpoint_snapshot_for_generation(
        &ClientEndpointId::Local,
        1,
        Box::new(mc_s4_agents_snapshot()),
    );
    state.activate_endpoint_projection(&ClientEndpointId::Local);
    let mut current_surface = surface();
    current_surface.projection_revision = 2;
    state.set_pane_surface(current_surface);
    state.config.agent_panel_sort = crate::config::AgentPanelSortConfig::Missions;
    state.set_endpoint_organization_for_generation(&ClientEndpointId::Local, 1, serde_json::from_value(serde_json::json!({"boot_id":"boot-1","organization":{
        "revision":2,"collections":[],"missions":[{"id":"runtime","name":"Agent runtime","order":0},{"id":"tako","name":"Tako platform","order":1}],
        "mission_assignments":[{"target":{"kind":"tab","tab_id":"tab_1"},"mission_id":"runtime"}],
        "pane_mission_assignments":[{"pane_id":"pane_1","mission_id":"tako"}]
    }})).unwrap());
    state.sidebar_collapsed = true;
    state.compose(106, 40).unwrap();
    assert_eq!(state.hits.agents[0].1, "pane_2");
    let rect = state.hits.agents[0].0;
    let focused = click(&mut state, rect);
    assert!(
        matches!(&focused.actions[..], [ClientShellAction::Endpoint {request,..}] if matches!(&request.method, crate::api::schema::Method::PaneFocus(p) if p.pane_id == "pane_2"))
    );
}

#[test]
fn mc_s4_single_endpoint_keyboard_focus_follows_visible_mission_order() {
    let mut state = mission_client();
    state.set_endpoint_methods(Some(vec!["pane.focus".into(), "organization.get".into()]));
    state.cache_endpoint_snapshot_for_generation(
        &ClientEndpointId::Local,
        1,
        Box::new(mc_s4_agents_snapshot()),
    );
    state.activate_endpoint_projection(&ClientEndpointId::Local);
    let mut current_surface = surface();
    current_surface.projection_revision = 2;
    state.set_pane_surface(current_surface);
    state.config.agent_panel_sort = crate::config::AgentPanelSortConfig::Missions;
    state.set_endpoint_organization_for_generation(&ClientEndpointId::Local, 1, serde_json::from_value(serde_json::json!({"boot_id":"boot-1","organization":{
        "revision":2,"collections":[],"missions":[{"id":"runtime","name":"Agent runtime","order":0},{"id":"tako","name":"Tako platform","order":1}],
        "mission_assignments":[{"target":{"kind":"tab","tab_id":"tab_1"},"mission_id":"runtime"}],
        "pane_mission_assignments":[{"pane_id":"pane_1","mission_id":"tako"}]
    }})).unwrap());
    state.config.agents.rows = vec![vec![crate::config::AgentSidebarToken::Agent]];
    state.compose(106, 40).unwrap();
    assert_eq!(state.hits.agents[0].1, "pane_2");
    let mut focused = ClientShellInput::default();
    state.record_binding(
        crate::input::KeybindMatch::Action(crate::input::KeybindAction::FocusAgent(0)),
        &mut focused,
    );
    assert!(
        matches!(&focused.actions[..], [ClientShellAction::Endpoint {request,..}] if matches!(&request.method, crate::api::schema::Method::PaneFocus(p) if p.pane_id == "pane_2"))
    );
}

#[test]
fn mc_s4_foreign_agent_context_cannot_assign_duplicate_local_pane_id() {
    let mut state = mission_client();
    state.cache_endpoint_snapshot_for_generation(
        &ClientEndpointId::Local,
        1,
        Box::new(mc_s4_agents_snapshot()),
    );
    state.activate_endpoint_projection(&ClientEndpointId::Local);
    let mut current_surface = surface();
    current_surface.projection_revision = 2;
    state.set_pane_surface(current_surface);
    state.config.agents.rows = vec![vec![crate::config::AgentSidebarToken::Agent]];
    let mut remote = state.endpoints[0].clone();
    let remote_id = ClientEndpointId::Ssh(
        crate::client::endpoint::ProfileId::parse("0123456789abcdef0123456789abcdef").unwrap(),
    );
    remote.endpoint_id = remote_id.clone();
    remote.label = "Foreign".into();
    remote.status = ClientEndpointStatus::Reconnecting;
    state.endpoints.push(remote);
    state.compose(106, 40).unwrap();
    let target = state
        .hits
        .endpoint_agents
        .iter()
        .find(|(_, id, pane)| id == &remote_id && pane == "pane_1")
        .unwrap()
        .0;
    let outcome = state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column: target.x,
        row: target.y,
        modifiers: KeyModifiers::empty(),
    })]);
    assert!(outcome.actions.is_empty() && outcome.requests.is_empty());
    assert!(
        state.overlay.is_none(),
        "a foreign duplicate ID must not open the local pane's menu"
    );
    assert_eq!(state.active_endpoint_id, ClientEndpointId::Local);
    let text = frame_rows(&state.compose(106, 40).unwrap()).join("\n");
    assert!(text.contains("Select endpoint"));
}

#[test]
fn mc_s4_pane_picker_rejects_new_connection_and_missing_method_notice_survives_typing() {
    let mut state = mission_client();
    state.set_endpoint_methods(Some(vec![
        "organization.get".into(),
        "mission.assign_pane".into(),
    ]));
    state.cache_endpoint_snapshot_for_generation(
        &ClientEndpointId::Local,
        1,
        Box::new(mc_s4_agents_snapshot()),
    );
    state.activate_endpoint_projection(&ClientEndpointId::Local);
    let mut current_surface = surface();
    current_surface.projection_revision = 2;
    state.set_pane_surface(current_surface.clone());
    state.config.agents.rows = vec![vec![crate::config::AgentSidebarToken::Agent]];
    state.compose(106, 40).unwrap();
    let target = state.hits.endpoint_agents[1].0;
    state.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column: target.x,
        row: target.y,
        modifiers: KeyModifiers::empty(),
    })]);
    click_menu_label(&mut state, "Assign mission", false);
    state.cache_endpoint_snapshot_for_generation(
        &ClientEndpointId::Local,
        2,
        Box::new(mc_s4_agents_snapshot()),
    );
    state.activate_endpoint_projection(&ClientEndpointId::Local);
    state.set_pane_surface(current_surface.clone());
    let selected = state.handle_input_bytes(b"\r");
    assert!(
        selected.actions.is_empty() && selected.requests.is_empty(),
        "old picker cannot target a new connection"
    );
    assert!(frame_rows(&state.compose(106, 40).unwrap())
        .join("\n")
        .contains("unavailable"));

    let mut legacy = mission_client();
    legacy.cache_endpoint_snapshot_for_generation(
        &ClientEndpointId::Local,
        1,
        Box::new(mc_s4_agents_snapshot()),
    );
    legacy.activate_endpoint_projection(&ClientEndpointId::Local);
    legacy.set_pane_surface(current_surface);
    legacy.config.agents.rows = vec![vec![crate::config::AgentSidebarToken::Agent]];
    legacy.compose(106, 40).unwrap();
    let target = legacy.hits.endpoint_agents[1].0;
    legacy.handle_raw_events(vec![RawInputEvent::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Right),
        column: target.x,
        row: target.y,
        modifiers: KeyModifiers::empty(),
    })]);
    assert!(click_menu_label(&mut legacy, "Assign mission", false)
        .actions
        .is_empty());
    legacy.tick_notifications(std::time::Instant::now() + std::time::Duration::from_secs(60));
    let typing = legacy.handle_input_bytes(b"preserved");
    assert!(typing.requests.iter().any(
        |r| matches!(r,ClientMessage::ClientShellPaneInput {pane_id,..} if pane_id == "pane_1")
    ));
    assert!(frame_rows(&legacy.compose(106, 40).unwrap())
        .join("\n")
        .contains("unavailable"));
}

#[test]
fn mc_s4_parked_mission_agents_remain_visible_and_focus_reveals_exact_target() {
    let mut state = mission_client();
    state.set_endpoint_methods(Some(vec!["organization.get".into(), "pane.focus".into()]));
    state.cache_endpoint_snapshot_for_generation(
        &ClientEndpointId::Local,
        1,
        Box::new(mc_s4_agents_snapshot()),
    );
    state.activate_endpoint_projection(&ClientEndpointId::Local);
    let mut current_surface = surface();
    current_surface.projection_revision = 2;
    state.set_pane_surface(current_surface);
    state.config.agents.rows = vec![vec![crate::config::AgentSidebarToken::Agent]];
    state.config.agent_panel_sort = crate::config::AgentPanelSortConfig::Missions;
    state.set_endpoint_organization_for_generation(&ClientEndpointId::Local,1,serde_json::from_value(serde_json::json!({"boot_id":"boot-1","organization":{
        "revision":2,"collections":[{"id":"parked","name":"Side quests","order":0,"hibernating":true}],
        "family_assignments":[{"family_id":{"kind":"standalone","workspace_id":"ws_1"},"collection_id":"parked"}],
        "missions":[{"id":"runtime","name":"Agent runtime","order":0}],
        "mission_assignments":[{"target":{"kind":"tab","tab_id":"tab_1"},"mission_id":"runtime"}]
    }})).unwrap());
    let text = frame_rows(&state.compose(106, 40).unwrap()).join("\n");
    assert!(text.contains("Agent Alpha") && text.contains("Agent Beta"));
    assert!(text.contains("Hibernate"));
    assert!(!state.expanded_hibernate.contains(&ClientEndpointId::Local));
    let target = state
        .hits
        .endpoint_agents
        .iter()
        .find(|(_, _, id)| id == "pane_2")
        .unwrap()
        .0;
    let focused = click(&mut state, target);
    assert!(
        matches!(&focused.actions[..],[ClientShellAction::Endpoint {request,..}] if matches!(&request.method,crate::api::schema::Method::PaneFocus(p) if p.pane_id=="pane_2"))
    );
    assert!(state.expanded_hibernate.contains(&ClientEndpointId::Local));
    assert!(
        state.endpoints[0]
            .organization
            .as_ref()
            .unwrap()
            .organization
            .collections[0]
            .hibernating
    );
}
