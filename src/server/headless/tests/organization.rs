use super::*;

#[tokio::test]
async fn mc_s1_endpoint_catalog_reaches_two_clients_and_new_attachments() {
    let mut server = test_headless_server();
    server.app.state.workspaces = vec![crate::workspace::Workspace::test_new("collection")];
    server.app.state.ensure_test_terminals();
    server.app.state.active = Some(0);
    let (first, _) = connect_test_shell(&mut server, 71, 80, 24);
    let (second, _) = connect_test_shell(&mut server, 72, 80, 24);
    for receiver in [&first, &second] {
        receiver.try_iter().for_each(drop);
    }
    let request = serde_json::from_value(serde_json::json!({
        "id": "create", "method": "collection.create", "params": { "name": "Agent workshop" }
    }))
    .unwrap();
    assert!(
        server.handle_server_event(ServerEvent::ClientShellEndpointRequest {
            client_id: 71,
            boot_id: server.client_shell_boot_id.clone(),
            request: Box::new(request),
        })
    );
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let event = server.server_event_rx.recv().await.expect("endpoint response event");
            let complete = matches!(&event, ServerEvent::ClientShellEndpointResponseChunkReady {
                client_id: 71, request_id, final_chunk: true, data, ..
            } if request_id == "create" && {
                let response: serde_json::Value = serde_json::from_slice(data).expect("JSON response");
                assert_eq!(response["result"]["type"], "collection_created", "{response}");
                true
            });
            server.handle_server_event(event);
            if complete { break; }
        }
    }).await.expect("confirmed collection response");
    server.render_and_stream();
    // Endpoint writers deliver on another thread; render completion does not
    // imply that the confirmed catalog has reached every client yet.
    let first_catalog = catalog_at_revision(&first, 1);
    assert_eq!(first_catalog["boot_id"], server.client_shell_boot_id);
    assert_eq!(first_catalog["organization"]["revision"], 1);
    assert_eq!(
        first_catalog["organization"]["collections"][0]["name"],
        "Agent workshop"
    );
    assert_eq!(catalog_at_revision(&second, 1), first_catalog);
    let (third, _) = connect_test_shell(&mut server, 73, 80, 24);
    assert_eq!(catalog_at_revision(&third, 1), first_catalog);
    server.app.state.workspaces[0].set_custom_name("renamed workspace".into());
    server.render_and_stream();
    for receiver in [&first, &second, &third] {
        assert!(
            receiver.try_iter().all(|bytes| !matches!(
                read_server_message(bytes),
                ServerMessage::EndpointControl { kind, .. } if kind == "endpoint.organization.v1"
            )),
            "unrelated session changes do not re-encode the unchanged catalog"
        );
    }
    assert!(
        protocol::endpoint::EndpointServerWelcome::compatible(Vec::new())
            .capabilities
            .iter()
            .any(|capability| capability == "organization_catalog")
    );
    shutdown_test_runtimes(&mut server);
}

fn catalog_at_revision(
    receiver: &std::sync::mpsc::Receiver<Vec<u8>>,
    revision: u64,
) -> serde_json::Value {
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    loop {
        let bytes = receiver
            .recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))
            .expect("expected catalog revision reaches the attached client");
        if let ServerMessage::EndpointControl { kind, data } = read_server_message(bytes) {
            if kind == "endpoint.organization.v1" {
                let catalog: serde_json::Value = serde_json::from_str(&data).unwrap();
                if catalog["organization"]["revision"].as_u64().unwrap() >= revision {
                    return catalog;
                }
            }
        }
    }
}

#[tokio::test]
async fn mc_s2_hibernate_revision_reaches_clients_and_new_attachment_without_terminal_changes() {
    let mut server = test_headless_server();
    server.app.state.workspaces = vec![crate::workspace::Workspace::test_new("parked")];
    server.app.state.ensure_test_terminals();
    server.app.state.active = Some(0);
    let created: serde_json::Value = serde_json::from_str(&server.app.handle_api_request(
        serde_json::from_value(serde_json::json!({"id":"create", "method":"collection.create", "params":{"name":"Side quests"}})).unwrap())).unwrap();
    let id = created["result"]["collection"]["id"].clone();
    let (first, _first_render) = connect_test_shell(&mut server, 81, 80, 24);
    let (second, _second_render) = connect_test_shell(&mut server, 82, 80, 24);
    server.render_and_stream();
    let before = server.app.session_snapshot();
    first.try_iter().for_each(drop);
    second.try_iter().for_each(drop);
    server.handle_server_event(ServerEvent::ClientShellEndpointRequest {
        client_id: 81,
        boot_id: server.client_shell_boot_id.clone(),
        request: Box::new(
            serde_json::from_value(
                serde_json::json!({"id":"hibernate", "method":"collection.set_hibernating",
                "params":{"collection_id":id, "hibernating":true}}),
            )
            .unwrap(),
        ),
    });
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let event = server.server_event_rx.recv().await.unwrap();
            let complete = matches!(&event, ServerEvent::ClientShellEndpointResponseChunkReady {
                client_id:81, request_id, final_chunk:true, data, ..
            } if request_id == "hibernate" && {
                let response: serde_json::Value = serde_json::from_slice(data).unwrap();
                assert_eq!(response["result"]["organization"]["collections"][0]["hibernating"], true);
                true
            });
            server.handle_server_event(event);
            if complete { break; }
        }
    }).await.unwrap();
    server.render_and_stream();
    let confirmed = catalog_at_revision(&first, 2);
    assert_eq!(confirmed["organization"]["revision"], 2);
    assert_eq!(
        confirmed["organization"]["collections"][0]["hibernating"],
        true
    );
    assert_eq!(catalog_at_revision(&second, 2), confirmed);
    let (third, _third_render) = connect_test_shell(&mut server, 83, 80, 24);
    assert_eq!(catalog_at_revision(&third, 2), confirmed);
    assert_eq!(server.app.session_snapshot(), before);
    server.render_and_stream();
    for receiver in [&first, &second, &third] {
        assert!(receiver
            .try_iter()
            .all(|bytes| !matches!(read_server_message(bytes),
            ServerMessage::EndpointControl { kind, .. } if kind == "endpoint.organization.v1")));
    }
    shutdown_test_runtimes(&mut server);
}

#[tokio::test]
async fn mc_s3_transfer_keeps_each_attached_clients_existing_terminal() {
    let mut server = test_headless_server();
    let mut source = crate::workspace::Workspace::test_new("source");
    source.test_add_tab(Some("remaining"));
    source.switch_tab(0);
    server.app.state.workspaces =
        vec![source, crate::workspace::Workspace::test_new("destination")];
    server.app.state.active = Some(0);
    server.app.state.selected = 0;
    server.app.state.ensure_test_terminals();
    let source_id = server.app.public_tab_id(0, 0).unwrap();
    let destination = server.app.public_workspace_id(1);
    let destination_tab = server.app.public_tab_id(1, 0).unwrap();
    let (first, _first_render) = connect_test_shell(&mut server, 91, 80, 24);
    let (second, _second_render) = connect_test_shell(&mut server, 92, 80, 24);
    let first_before = client_shell_snapshot(&first);
    second.try_iter().for_each(drop);
    let (tx, rx) = std::sync::mpsc::channel();
    server.handle_client_shell_api_request(92,crate::api::ApiRequestMessage {
        request:serde_json::from_value(serde_json::json!({"id":"focus","method":"tab.focus","params":{"tab_id":destination_tab}})).unwrap(),
        respond_to:tx,response_write_complete:None,
    });
    rx.recv().unwrap();
    server.render_and_stream();
    let second_before = client_shell_snapshot(&second);
    first.try_iter().for_each(drop);
    let (tx, rx) = std::sync::mpsc::channel();
    server.handle_api_request_with_shutdown_check(crate::api::ApiRequestMessage {
        request:serde_json::from_value(serde_json::json!({"id":"transfer","method":"tab.transfer","params":{"tab_id":source_id,"workspace_id":destination,"insert_index":0}})).unwrap(),
        respond_to:tx,response_write_complete:None,
    });
    let response: serde_json::Value = serde_json::from_str(&rx.recv().unwrap()).unwrap();
    let moved = response["result"]["tab"]["tab_id"].as_str().unwrap();
    server.render_and_stream();
    let first_after = client_shell_snapshot(&first);
    let second_after = client_shell_snapshot(&second);
    assert_eq!(first_after.focused_tab_id.as_deref(), Some(moved));
    assert_eq!(
        first_after.focused_pane_id.as_deref(),
        server
            .app
            .public_pane_id(1, server.app.state.workspaces[1].tabs[0].root_pane)
            .as_deref()
    );
    assert_ne!(first_before.focused_tab_id, first_after.focused_tab_id);
    assert_eq!(second_after.focused_tab_id, second_before.focused_tab_id);
    assert_eq!(second_after.focused_pane_id, second_before.focused_pane_id);
    server.app.state.assert_invariants_for_test();
    shutdown_test_runtimes(&mut server);
}
