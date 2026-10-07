use crate::app::App;
use serde_json::{json, Value};

fn app() -> App {
    let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let mut app = App::new(
        &crate::config::Config::default(),
        crate::app::AppPolicy::TEST,
        None,
        rx,
        crate::api::EventHub::default(),
    );
    app.state = crate::app::AppState::test_with_adversarial_identity_state();
    app
}
fn call(app: &mut App, method: &str, params: Value) -> Value {
    let request =
        serde_json::from_value(json!({"id":"maintenance","method":method,"params":params}))
            .expect("public maintenance method");
    serde_json::from_str(&app.handle_api_request(request)).unwrap()
}

#[test]
fn mc_s8_collection_rename_preserves_exact_identity_members_parked_and_terminal_work() {
    let mut app = app();
    let collection = app.state.create_collection("Old".into()).unwrap();
    let family = crate::app::AppState::family_id(&app.state.workspaces[0]);
    app.state
        .assign_family_to_collection(family.clone(), collection.id.clone())
        .unwrap();
    app.state
        .set_collection_hibernating(collection.id.clone(), true)
        .unwrap();
    let terminals = app.session_snapshot();
    let response = call(
        &mut app,
        "collection.rename",
        json!({"collection_id":collection.id,"name":" Tako platform "}),
    );
    assert!(response.get("error").is_none(), "{response}");
    let catalog = &response["result"]["organization"];
    assert_eq!(
        catalog["collections"][0],
        json!({"id":collection.id,"name":"Tako platform","order":0,"hibernating":true})
    );
    assert_eq!(
        catalog["family_assignments"],
        json!([{"family_id":family,"collection_id":collection.id}])
    );
    assert_eq!(app.session_snapshot(), terminals);
    app.state.session_dirty = false;
    assert_eq!(
        call(
            &mut app,
            "collection.rename",
            json!({"collection_id":collection.id,"name":"Tako platform"})
        ),
        response
    );
    assert!(!app.state.session_dirty);
    let confirmed = app.state.organization.clone();
    for (id, name, code) in [
        (collection.id.0.as_str(), " ", "invalid_collection_name"),
        ("absent", "Tako platform", "collection_not_found"),
    ] {
        assert_eq!(
            call(
                &mut app,
                "collection.rename",
                json!({"collection_id":id,"name":name})
            )["error"]["code"],
            code
        );
        assert_eq!(app.state.organization, confirmed);
        assert!(!app.state.session_dirty);
    }
    app.state.assert_invariants_for_test();
}

#[test]
fn mc_s8_mission_rename_retains_tab_pane_identity_and_work() {
    let mut app = app();
    let mission = app
        .state
        .create_mission("Old".into(), Some("Keep objective".into()))
        .unwrap();
    let tab = app.public_tab_id(0, 2).unwrap();
    let pane = app
        .public_pane_id(0, app.state.workspaces[0].tabs[2].root_pane)
        .unwrap();
    app.state
        .assign_mission(
            crate::organization::MissionTarget::Tab {
                tab_id: tab.clone(),
            },
            mission.id.clone(),
        )
        .unwrap();
    app.state
        .assign_pane_mission(pane.clone(), mission.id.clone())
        .unwrap();
    let terminals = app.session_snapshot();
    let response = call(
        &mut app,
        "mission.rename",
        json!({"mission_id":mission.id,"name":" Tako platform "}),
    );
    assert_eq!(
        response["result"]["organization"]["missions"][0],
        json!({"id":mission.id,"name":"Tako platform","objective":"Keep objective","order":0})
    );
    assert_eq!(
        app.state.organization.effective_pane_mission(&pane, &tab),
        Some(&mission.id)
    );
    assert_eq!(app.session_snapshot(), terminals);
    app.state.session_dirty = false;
    assert_eq!(
        call(
            &mut app,
            "mission.rename",
            json!({"mission_id":mission.id,"name":"Tako platform"})
        ),
        response
    );
    assert!(!app.state.session_dirty);
    for (id, name, code) in [
        (mission.id.0.as_str(), " ", "invalid_mission_name"),
        ("absent", "Tako platform", "mission_not_found"),
    ] {
        assert_eq!(
            call(
                &mut app,
                "mission.rename",
                json!({"mission_id":id,"name":name})
            )["error"]["code"],
            code
        );
        assert!(!app.state.session_dirty);
    }
    app.state.assert_invariants_for_test();
}

#[test]
fn mc_s8_collection_move_orders_globally_including_parked_without_moving_work() {
    let mut app = app();
    let first = app.state.create_collection("First".into()).unwrap();
    let parked = app.state.create_collection("Parked".into()).unwrap();
    let last = app.state.create_collection("Last".into()).unwrap();
    app.state
        .set_collection_hibernating(parked.id.clone(), true)
        .unwrap();
    let terminal = app.session_snapshot();
    let response = call(
        &mut app,
        "collection.move",
        json!({"collection_id":last.id,"to_index":0}),
    );
    assert!(response.get("error").is_none(), "{response}");
    assert_eq!(
        app.state
            .organization
            .collections
            .iter()
            .map(|c| (&c.id, c.order))
            .collect::<Vec<_>>(),
        vec![(&last.id, 0), (&first.id, 1), (&parked.id, 2)]
    );
    assert!(app.state.organization.collections[2].hibernating);
    app.state.session_dirty = false;
    assert_eq!(
        call(
            &mut app,
            "collection.move",
            json!({"collection_id":last.id,"to_index":0})
        ),
        response
    );
    assert!(!app.state.session_dirty);
    let confirmed = app.state.organization.clone();
    for (id, index, code) in [
        (last.id.0.as_str(), 3, "invalid_order_index"),
        ("absent", 0, "collection_not_found"),
    ] {
        assert_eq!(
            call(
                &mut app,
                "collection.move",
                json!({"collection_id":id,"to_index":index})
            )["error"]["code"],
            code
        );
        assert_eq!(app.state.organization, confirmed);
        assert!(!app.state.session_dirty);
    }
    assert_eq!(app.session_snapshot(), terminal);
    app.state.assert_invariants_for_test();
}

#[test]
fn mc_s8_mission_move_preserves_identity_objective_and_explicit_members() {
    let mut app = app();
    let first = app
        .state
        .create_mission("Same".into(), Some("First objective".into()))
        .unwrap();
    let middle = app.state.create_mission("Same".into(), None).unwrap();
    let last = app.state.create_mission("Last".into(), None).unwrap();
    let tab = crate::organization::MissionTarget::Tab {
        tab_id: app.public_tab_id(0, 2).unwrap(),
    };
    app.state
        .assign_mission(tab.clone(), first.id.clone())
        .unwrap();
    let terminals = app.session_snapshot();
    let response = call(
        &mut app,
        "mission.move",
        json!({"mission_id":last.id,"to_index":0}),
    );
    assert!(response.get("error").is_none(), "{response}");
    assert_eq!(
        app.state
            .organization
            .missions
            .iter()
            .map(|m| (&m.id, m.order))
            .collect::<Vec<_>>(),
        vec![(&last.id, 0), (&first.id, 1), (&middle.id, 2)]
    );
    assert_eq!(app.state.organization.mission_for(&tab), Some(&first.id));
    assert_eq!(
        app.state.organization.missions[1].objective.as_deref(),
        Some("First objective")
    );
    app.state.session_dirty = false;
    assert_eq!(
        call(
            &mut app,
            "mission.move",
            json!({"mission_id":last.id,"to_index":0})
        ),
        response
    );
    let confirmed = app.state.organization.clone();
    for (id, index, code) in [
        (last.id.0.as_str(), 3, "invalid_order_index"),
        ("absent", 0, "mission_not_found"),
    ] {
        assert_eq!(
            call(
                &mut app,
                "mission.move",
                json!({"mission_id":id,"to_index":index})
            )["error"]["code"],
            code
        );
        assert_eq!(app.state.organization, confirmed);
        assert!(!app.state.session_dirty);
    }
    assert_eq!(app.session_snapshot(), terminals);
    app.state.assert_invariants_for_test();
}

#[test]
fn mc_s8_objective_edit_requires_nullable_key_and_preserves_work() {
    let mut app = app();
    let mission = app
        .state
        .create_mission("Tako platform".into(), None)
        .unwrap();
    let terminals = app.session_snapshot();
    let response = call(
        &mut app,
        "mission.set_objective",
        json!({"mission_id":mission.id,"objective":" A concrete objective "}),
    );
    assert_eq!(
        response["result"]["organization"]["missions"][0]["objective"],
        " A concrete objective "
    );
    app.state.session_dirty = false;
    assert_eq!(
        call(
            &mut app,
            "mission.set_objective",
            json!({"mission_id":mission.id,"objective":" A concrete objective "})
        ),
        response
    );
    assert!(!app.state.session_dirty);
    assert_eq!(
        call(
            &mut app,
            "mission.set_objective",
            json!({"mission_id":"absent","objective":null})
        )["error"]["code"],
        "mission_not_found"
    );
    let cleared = call(
        &mut app,
        "mission.set_objective",
        json!({"mission_id":mission.id,"objective":null}),
    );
    assert!(cleared["result"]["organization"]["missions"][0]
        .get("objective")
        .is_none());
    app.state.session_dirty = false;
    assert_eq!(
        call(
            &mut app,
            "mission.set_objective",
            json!({"mission_id":mission.id,"objective":" \n "})
        ),
        cleared
    );
    assert!(!app.state.session_dirty);
    assert!(serde_json::from_value::<crate::api::schema::Request>(
        json!({"id":"bad","method":"mission.set_objective","params":{"mission_id":mission.id}})
    )
    .is_err());
    assert!(serde_json::from_value::<crate::api::schema::Request>(json!({"id":"bad","method":"mission.set_objective","params":{"mission_id":mission.id,"objective":42}})).is_err());
    let schema = serde_json::to_value(schemars::schema_for!(crate::api::schema::Request)).unwrap();
    assert!(schema["$defs"]["MissionSetObjectiveParams"]["required"]
        .as_array()
        .unwrap()
        .contains(&json!("objective")));
    assert!(
        schema["$defs"]["MissionSetObjectiveParams"]["properties"]["objective"]["type"]
            .as_array()
            .is_some_and(|types| types.contains(&json!("null"))),
        "objective schema must accept explicit null: {}",
        schema["$defs"]["MissionSetObjectiveParams"]
    );
    assert_eq!(app.session_snapshot(), terminals);
    app.state.assert_invariants_for_test();
}

#[test]
fn mc_s8_collection_delete_returns_live_and_retained_families_to_uncollected_without_terminal_effects(
) {
    let mut app = app();
    let first = app.state.create_collection("First".into()).unwrap();
    let removed = app.state.create_collection("Delete".into()).unwrap();
    let last = app.state.create_collection("Last".into()).unwrap();
    let live = crate::app::AppState::family_id(&app.state.workspaces[0]);
    app.state
        .assign_family_to_collection(live.clone(), removed.id.clone())
        .unwrap();
    // A managed family remains retained after its final workspace closes.
    app.state
        .organization
        .assign_family(
            crate::organization::FamilyId::Managed {
                key: "offline-repository".into(),
            },
            removed.id.clone(),
            &[crate::organization::FamilyId::Managed {
                key: "offline-repository".into(),
            }],
        )
        .unwrap();
    let terminals = app.session_snapshot();
    let response = call(
        &mut app,
        "collection.delete",
        json!({"collection_id":removed.id}),
    );
    assert!(response.get("error").is_none(), "{response}");
    assert_eq!(app.state.organization.collection_for(&live), None);
    assert!(app.state.organization.family_assignments.is_empty());
    assert_eq!(
        app.state
            .organization
            .collections
            .iter()
            .map(|c| (&c.id, c.order))
            .collect::<Vec<_>>(),
        vec![(&first.id, 0), (&last.id, 1)]
    );
    let next = app.state.create_collection("New".into()).unwrap();
    assert_eq!(next.order, 2);
    assert_eq!(app.session_snapshot(), terminals);
    app.state.session_dirty = false;
    let confirmed = app.state.organization.clone();
    assert_eq!(
        call(
            &mut app,
            "collection.delete",
            json!({"collection_id":removed.id})
        )["error"]["code"],
        "collection_not_found"
    );
    assert_eq!(app.state.organization, confirmed);
    assert!(!app.state.session_dirty);
    app.state.assert_invariants_for_test();
}

#[test]
fn mc_s8_collection_unassign_uses_expected_ownership_and_exact_live_family() {
    let mut app = app();
    let first = app.state.create_collection("First".into()).unwrap();
    let second = app.state.create_collection("Second".into()).unwrap();
    let family = crate::app::AppState::family_id(&app.state.workspaces[0]);
    app.state
        .assign_family_to_collection(family.clone(), second.id.clone())
        .unwrap();
    let terminals = app.session_snapshot();
    app.state.session_dirty = false;
    let confirmed = app.state.organization.clone();
    assert_eq!(
        call(
            &mut app,
            "collection.unassign_family",
            json!({"family_id":family,"collection_id":first.id})
        )["error"]["code"],
        "assignment_changed"
    );
    assert_eq!(app.state.organization, confirmed);
    assert!(!app.state.session_dirty);
    let response = call(
        &mut app,
        "collection.unassign_family",
        json!({"family_id":family,"collection_id":second.id}),
    );
    assert!(response.get("error").is_none(), "{response}");
    assert_eq!(app.state.organization.collection_for(&family), None);
    app.state.session_dirty = false;
    assert_eq!(
        call(
            &mut app,
            "collection.unassign_family",
            json!({"family_id":family,"collection_id":second.id})
        ),
        response
    );
    assert!(!app.state.session_dirty);
    assert_eq!(
        call(
            &mut app,
            "collection.unassign_family",
            json!({"family_id":{"kind":"managed","key":"offline"},"collection_id":second.id})
        )["error"]["code"],
        "family_not_found"
    );
    assert_eq!(app.session_snapshot(), terminals);
    app.state.assert_invariants_for_test();
}

#[test]
fn mc_s8_mission_unassign_removes_tab_inheritance_and_keeps_pane_override() {
    let mut app = app();
    let first = app.state.create_mission("First".into(), None).unwrap();
    let second = app.state.create_mission("Second".into(), None).unwrap();
    let tab = app.public_tab_id(0, 2).unwrap();
    let panes = app.state.workspaces[0].tabs[2].layout.pane_ids();
    let explicit = app.public_pane_id(0, panes[0]).unwrap();
    let inherited = app.public_pane_id(0, panes[1]).unwrap();
    let target = crate::organization::MissionTarget::Tab {
        tab_id: tab.clone(),
    };
    app.state
        .assign_mission(target.clone(), second.id.clone())
        .unwrap();
    app.state
        .assign_pane_mission(explicit.clone(), first.id.clone())
        .unwrap();
    let terminals = app.session_snapshot();
    app.state.session_dirty = false;
    let confirmed = app.state.organization.clone();
    assert_eq!(
        call(
            &mut app,
            "mission.unassign",
            json!({"target":target,"mission_id":first.id})
        )["error"]["code"],
        "assignment_changed"
    );
    assert_eq!(app.state.organization, confirmed);
    assert!(!app.state.session_dirty);
    let response = call(
        &mut app,
        "mission.unassign",
        json!({"target":target,"mission_id":second.id}),
    );
    assert!(response.get("error").is_none(), "{response}");
    assert_eq!(
        app.state
            .organization
            .effective_pane_mission(&inherited, &tab),
        None
    );
    assert_eq!(
        app.state
            .organization
            .effective_pane_mission(&explicit, &tab),
        Some(&first.id)
    );
    app.state.session_dirty = false;
    assert_eq!(
        call(
            &mut app,
            "mission.unassign",
            json!({"target":target,"mission_id":second.id})
        ),
        response
    );
    assert!(!app.state.session_dirty);
    assert_eq!(
        call(
            &mut app,
            "mission.unassign",
            json!({"target":{"kind":"tab","tab_id":"missing"},"mission_id":second.id})
        )["error"]["code"],
        "tab_not_found"
    );
    assert_eq!(app.session_snapshot(), terminals);
    app.state.assert_invariants_for_test();
}

#[test]
fn mc_s8_mission_delete_clears_all_its_tab_and_pane_assignments_preserving_other_mission_and_work()
{
    let mut app = app();
    let first = app.state.create_mission("First".into(), None).unwrap();
    let removed = app.state.create_mission("Delete".into(), None).unwrap();
    let last = app.state.create_mission("Last".into(), None).unwrap();
    let tab = app.public_tab_id(0, 2).unwrap();
    let panes = app.state.workspaces[0].tabs[2].layout.pane_ids();
    let explicit = app.public_pane_id(0, panes[0]).unwrap();
    let inherited = app.public_pane_id(0, panes[1]).unwrap();
    let other = app
        .public_pane_id(0, app.state.workspaces[0].tabs[0].root_pane)
        .unwrap();
    app.state
        .assign_mission(
            crate::organization::MissionTarget::Tab {
                tab_id: tab.clone(),
            },
            removed.id.clone(),
        )
        .unwrap();
    app.state
        .assign_pane_mission(explicit.clone(), first.id.clone())
        .unwrap();
    app.state
        .assign_pane_mission(other.clone(), removed.id.clone())
        .unwrap();
    let terminals = app.session_snapshot();
    let response = call(&mut app, "mission.delete", json!({"mission_id":removed.id}));
    assert!(response.get("error").is_none(), "{response}");
    assert_eq!(
        app.state
            .organization
            .effective_pane_mission(&inherited, &tab),
        None
    );
    assert_eq!(app.state.organization.pane_override(&other), None);
    assert_eq!(
        app.state.organization.pane_override(&explicit),
        Some(&first.id)
    );
    assert!(app.state.organization.mission_assignments.is_empty());
    assert_eq!(
        app.state
            .organization
            .missions
            .iter()
            .map(|m| (&m.id, m.order))
            .collect::<Vec<_>>(),
        vec![(&first.id, 0), (&last.id, 1)]
    );
    assert_eq!(
        app.state.create_mission("New".into(), None).unwrap().order,
        2
    );
    assert_eq!(app.session_snapshot(), terminals);
    app.state.session_dirty = false;
    let confirmed = app.state.organization.clone();
    assert_eq!(
        call(&mut app, "mission.delete", json!({"mission_id":removed.id}))["error"]["code"],
        "mission_not_found"
    );
    assert_eq!(app.state.organization, confirmed);
    assert!(!app.state.session_dirty);
    app.state.assert_invariants_for_test();
}

#[test]
fn mc_s8_move_uses_persisted_display_order_rather_than_storage_position() {
    let mut app = app();
    app.state.organization=serde_json::from_value(json!({"revision":4,
        "collections":[{"id":"a","name":"A","order":2,"hibernating":false},{"id":"b","name":"B","order":0,"hibernating":true},{"id":"c","name":"C","order":1,"hibernating":false}],
        "missions":[{"id":"x","name":"X","order":2},{"id":"y","name":"Y","order":0},{"id":"z","name":"Z","order":1}]})).unwrap();
    let terminals = app.session_snapshot();
    call(
        &mut app,
        "collection.move",
        json!({"collection_id":"a","to_index":0}),
    );
    assert_eq!(
        app.state
            .organization
            .collections
            .iter()
            .map(|c| (c.id.0.as_str(), c.order))
            .collect::<Vec<_>>(),
        vec![("a", 0), ("b", 1), ("c", 2)]
    );
    call(
        &mut app,
        "mission.move",
        json!({"mission_id":"x","to_index":0}),
    );
    assert_eq!(
        app.state
            .organization
            .missions
            .iter()
            .map(|m| (m.id.0.as_str(), m.order))
            .collect::<Vec<_>>(),
        vec![("x", 0), ("y", 1), ("z", 2)]
    );
    assert_eq!(app.session_snapshot(), terminals);
    app.state.assert_invariants_for_test();
}

#[test]
fn mc_s8_revision_capacity_rejects_edits_atomically_but_reserves_membership_removal() {
    let mut app = app();
    let collection = app.state.create_collection("Collection".into()).unwrap();
    let other = app.state.create_collection("Other".into()).unwrap();
    let mission = app.state.create_mission("Mission".into(), None).unwrap();
    let target = crate::organization::MissionTarget::Tab {
        tab_id: app.public_tab_id(0, 2).unwrap(),
    };
    let pane = app
        .public_pane_id(0, app.state.workspaces[0].tabs[2].root_pane)
        .unwrap();
    app.state
        .assign_mission(target, mission.id.clone())
        .unwrap();
    app.state
        .assign_pane_mission(pane, mission.id.clone())
        .unwrap();
    let terminals = app.session_snapshot();
    let revision = app.state.organization.revision;
    app.state.organization.revision = u64::MAX - 2;
    let exhausted = app.state.organization.clone();
    app.state.session_dirty = false;
    for (method, params) in [
        (
            "collection.rename",
            json!({"collection_id":collection.id,"name":"Changed"}),
        ),
        (
            "collection.move",
            json!({"collection_id":other.id,"to_index":0}),
        ),
        (
            "mission.rename",
            json!({"mission_id":mission.id,"name":"Changed"}),
        ),
        (
            "mission.set_objective",
            json!({"mission_id":mission.id,"objective":"Changed"}),
        ),
    ] {
        assert_eq!(
            call(&mut app, method, params)["error"]["code"],
            "organization_revision_exhausted"
        );
        assert_eq!(app.state.organization, exhausted);
        assert!(!app.state.session_dirty);
    }
    assert!(
        call(&mut app, "mission.delete", json!({"mission_id":mission.id}))
            .get("error")
            .is_none()
    );
    assert_eq!(app.state.organization.revision, u64::MAX - 1);
    assert!(
        app.state.organization.mission_assignments.is_empty()
            && app.state.organization.pane_mission_assignments.is_empty()
    );
    assert_eq!(app.session_snapshot(), terminals);
    app.state.organization.revision = revision + 1;
    app.state.assert_invariants_for_test();
}

#[test]
fn mc_s8_clear_same_mission_override_truthfully_retains_inherited_membership() {
    let mut app = app();
    let mission = app.state.create_mission("Mission".into(), None).unwrap();
    let tab = app.public_tab_id(0, 2).unwrap();
    let pane = app
        .public_pane_id(0, app.state.workspaces[0].tabs[2].root_pane)
        .unwrap();
    app.state
        .assign_mission(
            crate::organization::MissionTarget::Tab {
                tab_id: tab.clone(),
            },
            mission.id.clone(),
        )
        .unwrap();
    app.state
        .assign_pane_mission(pane.clone(), mission.id.clone())
        .unwrap();
    let terminals = app.session_snapshot();
    assert!(call(
        &mut app,
        "mission.clear_pane_override",
        json!({"pane_id":pane})
    )
    .get("error")
    .is_none());
    assert_eq!(app.state.organization.pane_override(&pane), None);
    assert_eq!(
        app.state.organization.effective_pane_mission(&pane, &tab),
        Some(&mission.id)
    );
    assert_eq!(app.session_snapshot(), terminals);
    app.state.assert_invariants_for_test();
}

#[cfg(unix)]
#[test]
fn mc_s8_maintained_empty_definitions_survive_disk_recovery_writer_restart_and_handoff() {
    let mut app = app();
    let first = app.state.create_collection("First".into()).unwrap();
    let retained = app.state.create_collection("Retained".into()).unwrap();
    let mission = app.state.create_mission("Old".into(), None).unwrap();
    let other = app.state.create_mission("Other".into(), None).unwrap();
    let path = std::env::temp_dir().join(format!(
        "herdr-mc-s8-recovery-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).unwrap();
    let file = path.join("session.json");
    let mut writer = crate::persist::SessionWriter::at_path(file.clone(), false);
    writer.save(&app.capture_session_snapshot(), None);
    call(
        &mut app,
        "collection.rename",
        json!({"collection_id":retained.id,"name":"Retained renamed"}),
    );
    call(
        &mut app,
        "collection.set_hibernating",
        json!({"collection_id":retained.id,"hibernating":true}),
    );
    call(
        &mut app,
        "collection.delete",
        json!({"collection_id":first.id}),
    );
    call(
        &mut app,
        "mission.rename",
        json!({"mission_id":mission.id,"name":"Tako platform"}),
    );
    call(
        &mut app,
        "mission.set_objective",
        json!({"mission_id":mission.id,"objective":"Retain this objective"}),
    );
    call(
        &mut app,
        "mission.move",
        json!({"mission_id":other.id,"to_index":0}),
    );
    app.state.close_workspaces(vec![0]);
    let expected = app.capture_session_snapshot();
    assert!(expected.workspaces.is_empty());
    writer = crate::persist::SessionWriter::at_path(file.clone(), false);
    writer.save(&expected, None);
    let parsed: crate::persist::SessionSnapshot =
        serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(
        parsed.organization.collections[0].name.as_str(),
        "Retained renamed"
    );
    assert!(parsed.organization.collections[0].hibernating);
    assert_eq!(parsed.organization.collections[0].order, 0);
    assert_eq!(
        parsed
            .organization
            .missions
            .iter()
            .map(|m| m.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Other", "Tako platform"]
    );
    assert_eq!(
        parsed.organization.missions[1].objective.as_deref(),
        Some("Retain this objective")
    );
    let histories = std::fs::read_dir(path.join("session-snapshots"))
        .unwrap()
        .map(|e| {
            serde_json::from_slice::<crate::persist::SessionSnapshot>(
                &std::fs::read(e.unwrap().path()).unwrap(),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    assert!(histories
        .iter()
        .any(|s| s.organization == expected.organization && s.workspaces.is_empty()));
    let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let mut restored = App::new_from_handoff(
        &crate::config::Config::default(),
        None,
        rx,
        crate::api::EventHub::default(),
        &parsed,
        &mut std::collections::HashMap::new(),
    )
    .unwrap();
    assert_eq!(
        call(&mut restored, "organization.get", json!({}))["result"]["organization"],
        serde_json::to_value(&expected.organization).unwrap()
    );
    restored.state.assert_invariants_for_test();
    std::fs::remove_dir_all(path).unwrap();
}
