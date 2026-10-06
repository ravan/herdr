use super::*;

fn catalog(receiver: &std::sync::mpsc::Receiver<Vec<u8>>) -> serde_json::Value {
    receiver
        .try_iter()
        .filter_map(|bytes| match read_server_message(bytes) {
            ServerMessage::EndpointControl { kind, data } if kind == "endpoint.organization.v1" => {
                Some(serde_json::from_str(&data).unwrap())
            }
            _ => None,
        })
        .max_by_key(|catalog: &serde_json::Value| {
            catalog["organization"]["revision"].as_u64().unwrap()
        })
        .expect("a complete organization catalog is delivered")
}

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
    let first_catalog = catalog(&first);
    assert_eq!(first_catalog["boot_id"], server.client_shell_boot_id);
    assert_eq!(first_catalog["organization"]["revision"], 1);
    assert_eq!(
        first_catalog["organization"]["collections"][0]["name"],
        "Agent workshop"
    );
    assert_eq!(catalog(&second), first_catalog);
    let (third, _) = connect_test_shell(&mut server, 73, 80, 24);
    assert_eq!(catalog(&third), first_catalog);
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
