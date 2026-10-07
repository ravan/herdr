mod maintenance;

use super::responses::{encode_error, encode_success};
use crate::api::schema::{CollectionAssignFamilyParams, CollectionCreateParams, ResponseResult};
use crate::app::App;

impl App {
    pub(super) fn handle_mission_assign_pane(
        &mut self,
        id: String,
        params: crate::api::schema::MissionAssignPaneParams,
    ) -> String {
        match self
            .state
            .assign_pane_mission(params.pane_id, params.mission_id)
        {
            Ok(()) => self.handle_organization_get(id),
            Err(code) => encode_error(id, code, "Cannot assign pane mission"),
        }
    }
    pub(super) fn handle_mission_clear_pane_override(
        &mut self,
        id: String,
        params: crate::api::schema::MissionClearPaneOverrideParams,
    ) -> String {
        match self.state.clear_pane_mission(&params.pane_id) {
            Ok(()) => self.handle_organization_get(id),
            Err(code) => encode_error(id, code, "Cannot clear pane mission override"),
        }
    }

    pub(super) fn handle_mission_create(
        &mut self,
        id: String,
        params: crate::api::schema::MissionCreateParams,
    ) -> String {
        match self.state.create_mission(params.name, params.objective) {
            Ok(mission) => encode_success(
                id,
                ResponseResult::MissionCreated {
                    mission,
                    organization: self.state.organization.clone(),
                },
            ),
            Err(code) => encode_error(id, code, "Cannot create mission"),
        }
    }
    pub(super) fn handle_mission_assign(
        &mut self,
        id: String,
        params: crate::api::schema::MissionAssignParams,
    ) -> String {
        match self.state.assign_mission(params.target, params.mission_id) {
            Ok(()) => self.handle_organization_get(id),
            Err(code) => encode_error(id, code, "Cannot assign mission"),
        }
    }

    pub(super) fn handle_collection_set_hibernating(
        &mut self,
        id: String,
        params: crate::api::schema::CollectionSetHibernatingParams,
    ) -> String {
        match self
            .state
            .set_collection_hibernating(params.collection_id, params.hibernating)
        {
            Ok(()) => self.handle_organization_get(id),
            Err(code) => encode_error(id, code, "Cannot change collection hibernation"),
        }
    }

    pub(super) fn handle_collection_assign_family(
        &mut self,
        id: String,
        params: CollectionAssignFamilyParams,
    ) -> String {
        match self
            .state
            .assign_family_to_collection(params.family_id, params.collection_id)
        {
            Ok(()) => self.handle_organization_get(id),
            Err(code) => encode_error(id, code, "Cannot move family to collection"),
        }
    }
    pub(super) fn handle_organization_get(&self, id: String) -> String {
        encode_success(
            id,
            ResponseResult::Organization {
                organization: self.state.organization.clone(),
            },
        )
    }

    pub(super) fn handle_collection_create(
        &mut self,
        id: String,
        params: CollectionCreateParams,
    ) -> String {
        match self.state.create_collection(params.name) {
            Ok(collection) => encode_success(
                id,
                ResponseResult::CollectionCreated {
                    collection,
                    organization: self.state.organization.clone(),
                },
            ),
            Err(code) => encode_error(id, code, "Cannot create collection"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mc_s1_assign_family_changes_one_family_and_preserves_terminal_work() {
        let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &crate::config::Config::default(),
            crate::app::AppPolicy::TEST,
            None,
            rx,
            crate::api::EventHub::default(),
        );
        app.state = crate::app::AppState::test_with_adversarial_identity_state();
        for label in ["child-one", "child-two"] {
            app.state
                .workspaces
                .push(crate::workspace::Workspace::test_new(label));
        }
        for (index, workspace) in app.state.workspaces.iter_mut().enumerate() {
            workspace.worktree_space = Some(crate::workspace::WorktreeSpaceMembership {
                key: "repo-key".into(),
                label: "repo".into(),
                repo_root: "/repo".into(),
                checkout_path: format!("/repo-{index}").into(),
                is_linked_worktree: index > 0,
            });
        }
        app.state.ensure_test_terminals();
        app.state.assert_invariants_for_test();
        let collection = app
            .state
            .create_collection("Agent workshop".into())
            .unwrap();
        let before = app.session_snapshot();
        let request = serde_json::from_value(serde_json::json!({
            "id":"assign", "method":"collection.assign_family", "params": {
                "family_id":{"kind":"managed","key":"repo-key"}, "collection_id": collection.id
            }
        }))
        .expect("family assignment is a public JSON method");
        let result: serde_json::Value =
            serde_json::from_str(&app.handle_api_request(request)).unwrap();
        assert_eq!(result["result"]["organization"]["revision"], 2);
        assert_eq!(
            result["result"]["organization"]["family_assignments"],
            serde_json::json!([
                {"family_id":{"kind":"managed","key":"repo-key"},"collection_id":collection.id}
            ])
        );
        assert_eq!(app.session_snapshot(), before);
        app.state.assert_invariants_for_test();

        let confirmed = app.state.organization.clone();
        for (family, target, code) in [
            (
                serde_json::json!({"kind":"managed","key":"repo-key"}),
                "missing-collection".to_owned(),
                "collection_not_found",
            ),
            (
                serde_json::json!({"kind":"standalone","workspace_id":"missing-workspace"}),
                collection.id.0.clone(),
                "family_not_found",
            ),
        ] {
            app.state.session_dirty = false;
            let request = serde_json::from_value(serde_json::json!({
                "id":"rejected", "method":"collection.assign_family", "params": {
                    "family_id":family, "collection_id":target
                }
            }))
            .unwrap();
            let response: serde_json::Value =
                serde_json::from_str(&app.handle_api_request(request)).unwrap();
            assert_eq!(response["error"]["code"], code);
            assert_eq!(app.state.organization, confirmed);
            assert_eq!(app.session_snapshot(), before);
            assert!(!app.state.session_dirty);
            app.state.assert_invariants_for_test();
        }
    }
}

#[cfg(test)]
mod hibernate_tests {
    use super::*;
    #[test]
    fn mc_s2_json_hibernate_is_atomic_idempotent_and_required_boolean() {
        let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &crate::config::Config::default(),
            crate::app::AppPolicy::TEST,
            None,
            rx,
            crate::api::EventHub::default(),
        );
        app.state = crate::app::AppState::test_with_adversarial_identity_state();
        let collection = app.state.create_collection("Side quests".into()).unwrap();
        let before = app.session_snapshot();
        let call = |app: &mut App, id: &str, hibernating: bool| -> serde_json::Value {
            let request = serde_json::from_value(
                serde_json::json!({"id":"park", "method":"collection.set_hibernating",
                "params":{"collection_id":id,"hibernating":hibernating}}),
            )
            .expect("public Hibernate method");
            serde_json::from_str(&app.handle_api_request(request)).unwrap()
        };
        let parked = call(&mut app, &collection.id.0, true);
        assert_eq!(
            parked["result"]["organization"]["collections"][0]["hibernating"],
            true
        );
        assert_eq!(parked["result"]["organization"]["revision"], 2);
        let confirmed = app.state.organization.clone();
        app.state.session_dirty = false;
        assert_eq!(call(&mut app, &collection.id.0, true), parked);
        assert!(!app.state.session_dirty);
        assert_eq!(
            call(&mut app, "absent", false)["error"]["code"],
            "collection_not_found"
        );
        assert_eq!(app.state.organization, confirmed);
        app.state.organization.revision = u64::MAX;
        assert_eq!(
            call(&mut app, &collection.id.0, false)["error"]["code"],
            "organization_revision_exhausted"
        );
        assert!(app.state.organization.collections[0].hibernating);
        assert!(!app.state.session_dirty);
        assert_eq!(app.session_snapshot(), before);
        app.state.assert_invariants_for_test();
        assert!(serde_json::from_value::<crate::api::schema::Request>(
            serde_json::json!({"id":"bad",
            "method":"collection.set_hibernating", "params":{"collection_id":collection.id}})
        )
        .is_err());
    }
}

#[cfg(test)]
mod mission_tests {
    use super::*;
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
    fn call(app: &mut App, method: &str, params: serde_json::Value) -> serde_json::Value {
        let request = serde_json::from_value(
            serde_json::json!({"id":"mission", "method":method,"params":params}),
        )
        .expect("public mission method");
        serde_json::from_str(&app.handle_api_request(request)).unwrap()
    }
    #[test]
    fn mc_s4_combined_pane_tab_and_family_death_consumes_reserved_cleanup_capacity() {
        let mut app = app();
        app.state
            .workspaces
            .push(crate::workspace::Workspace::test_new("Last pane"));
        app.state.ensure_test_terminals();
        let workspace = app.public_workspace_id(1);
        let tab = app.public_tab_id(1, 0).unwrap();
        let pane = app.state.workspaces[1].tabs[0].root_pane;
        let pane_id = app.public_pane_id(1, pane).unwrap();
        let mission = call(
            &mut app,
            "mission.create",
            serde_json::json!({"name":"Retained"}),
        )["result"]["mission"]["id"]
            .clone();
        let collection = call(
            &mut app,
            "collection.create",
            serde_json::json!({"name":"Retained"}),
        )["result"]["collection"]["id"]
            .clone();
        call(
            &mut app,
            "mission.assign",
            serde_json::json!({"target":{"kind":"tab","tab_id":tab},"mission_id":mission}),
        );
        call(
            &mut app,
            "mission.assign_pane",
            serde_json::json!({"pane_id":pane_id,"mission_id":mission}),
        );
        call(
            &mut app,
            "collection.assign_family",
            serde_json::json!({"family_id":{"kind":"standalone","workspace_id":workspace},"collection_id":collection}),
        );
        app.state.organization.revision = u64::MAX - 3;
        app.state
            .handle_app_event(crate::events::AppEvent::PaneDied {
                pane_id: pane,
                exit_reason: crate::platform::ChildExitReason::Exited,
            });
        let catalog = call(&mut app, "organization.get", serde_json::json!({}))["result"]
            ["organization"]
            .clone();
        assert_eq!(catalog["pane_mission_assignments"], serde_json::json!([]));
        assert_eq!(catalog["mission_assignments"], serde_json::json!([]));
        assert_eq!(catalog["family_assignments"], serde_json::json!([]));
        assert_eq!(catalog["revision"], u64::MAX);
        assert_eq!(catalog["missions"].as_array().unwrap().len(), 1);
        assert_eq!(catalog["collections"].as_array().unwrap().len(), 1);
        app.state.assert_invariants_for_test();
    }

    #[test]
    fn mc_s4_tab_reassignment_and_process_changes_preserve_pane_override() {
        let mut app = app();
        let tab = app.public_tab_id(0, 2).unwrap();
        let panes = app.state.workspaces[0].tabs[2].layout.pane_ids();
        let overridden = app.public_pane_id(0, panes[0]).unwrap();
        let inherited = app.public_pane_id(0, panes[1]).unwrap();
        let first = app
            .state
            .create_mission("Agent runtime".into(), None)
            .unwrap();
        let second = app
            .state
            .create_mission("Tako platform".into(), None)
            .unwrap();
        app.state
            .assign_mission(
                crate::organization::MissionTarget::Tab {
                    tab_id: tab.clone(),
                },
                first.id.clone(),
            )
            .unwrap();
        app.state
            .assign_pane_mission(overridden.clone(), first.id.clone())
            .unwrap();
        app.state
            .assign_mission(
                crate::organization::MissionTarget::Tab {
                    tab_id: tab.clone(),
                },
                second.id.clone(),
            )
            .unwrap();
        assert_eq!(
            app.state
                .organization
                .effective_pane_mission(&overridden, &tab),
            Some(&first.id)
        );
        assert_eq!(
            app.state
                .organization
                .effective_pane_mission(&inherited, &tab),
            Some(&second.id)
        );
        let before = call(&mut app, "organization.get", serde_json::json!({}));
        for state in ["working", "blocked", "idle"] {
            let reported = call(
                &mut app,
                "pane.report_agent",
                serde_json::json!({"pane_id":overridden,"source":"demo","agent":"codex","state":state}),
            );
            assert!(reported.get("error").is_none(), "{reported}");
            assert_eq!(
                call(&mut app, "organization.get", serde_json::json!({})),
                before
            );
        }
        app.state.assert_invariants_for_test();
    }

    #[test]
    fn mc_s4_whole_tab_transfer_remaps_all_pane_overrides_and_tab_membership() {
        let mut app = app();
        app.state
            .workspaces
            .push(crate::workspace::Workspace::test_new("Destination"));
        app.state.ensure_test_terminals();
        let source = app.public_tab_id(0, 2).unwrap();
        let destination = app.public_workspace_id(1);
        let panes = app.state.workspaces[0].tabs[2].layout.pane_ids();
        let first = call(
            &mut app,
            "mission.create",
            serde_json::json!({"name":"Agent runtime"}),
        )["result"]["mission"]["id"]
            .clone();
        let second = call(
            &mut app,
            "mission.create",
            serde_json::json!({"name":"Tako platform"}),
        )["result"]["mission"]["id"]
            .clone();
        call(
            &mut app,
            "mission.assign",
            serde_json::json!({"target":{"kind":"tab","tab_id":source},"mission_id":first}),
        );
        for pane in &panes {
            let pane_id = app.public_pane_id(0, *pane).unwrap();
            call(
                &mut app,
                "mission.assign_pane",
                serde_json::json!({"pane_id":pane_id,"mission_id":second}),
            );
        }
        let revision = app.state.organization.revision;
        app.state.organization.revision = u64::MAX - panes.len() as u64 - 1;
        let before = app.session_snapshot();
        let before_catalog = app.state.organization.clone();
        let rejected = call(
            &mut app,
            "tab.transfer",
            serde_json::json!({"tab_id":source,"workspace_id":destination,"insert_index":0}),
        );
        assert_eq!(rejected["error"]["code"], "organization_revision_exhausted");
        assert_eq!(app.session_snapshot(), before);
        assert_eq!(app.state.organization, before_catalog);
        app.state.organization.revision = revision;
        let moved = call(
            &mut app,
            "tab.transfer",
            serde_json::json!({"tab_id":source,"workspace_id":destination,"insert_index":0}),
        );
        assert!(moved.get("error").is_none(), "{moved}");
        let catalog = call(&mut app, "organization.get", serde_json::json!({}));
        let expected = panes.iter().map(|pane| serde_json::json!({"pane_id":app.public_pane_id(1,*pane).unwrap(),"mission_id":second})).collect::<Vec<_>>();
        assert_eq!(
            catalog["result"]["organization"]["pane_mission_assignments"],
            serde_json::json!(expected)
        );
        assert_eq!(
            catalog["result"]["organization"]["mission_assignments"][0]["target"]["tab_id"],
            moved["result"]["tab"]["tab_id"]
        );
        app.state.assert_invariants_for_test();
    }

    #[test]
    fn mc_s4_pane_move_keeps_override_and_rejects_exhaustion_before_detach() {
        let mut app = app();
        app.state
            .workspaces
            .push(crate::workspace::Workspace::test_new("Destination"));
        app.state.ensure_test_terminals();
        let pane = app.state.workspaces[0].tabs[2].layout.pane_ids()[0];
        let pane_id = app.public_pane_id(0, pane).unwrap();
        let destination = app.public_tab_id(1, 0).unwrap();
        let mission = call(
            &mut app,
            "mission.create",
            serde_json::json!({"name":"Tako platform"}),
        )["result"]["mission"]["id"]
            .clone();
        call(
            &mut app,
            "mission.assign_pane",
            serde_json::json!({"pane_id":pane_id,"mission_id":mission}),
        );
        let params = serde_json::json!({"pane_id":pane_id,"destination":{"type":"tab","tab_id":destination,"split":"right"},"focus":false});
        app.state.organization.revision = u64::MAX - 1;
        let before = app.session_snapshot();
        let org = app.state.organization.clone();
        let failed = call(&mut app, "pane.move", params.clone());
        assert_eq!(failed["error"]["code"], "organization_revision_exhausted");
        assert_eq!(app.session_snapshot(), before);
        assert_eq!(app.state.organization, org);
        app.state.organization.revision = 2;
        let moved = call(&mut app, "pane.move", params);
        assert!(moved.get("error").is_none(), "{moved}");
        let new_id = app.public_pane_id(1, pane).unwrap();
        assert_ne!(new_id, pane_id);
        assert_eq!(
            call(&mut app, "organization.get", serde_json::json!({}))["result"]["organization"]
                ["pane_mission_assignments"],
            serde_json::json!([{"pane_id":new_id,"mission_id":mission}])
        );
        app.state.assert_invariants_for_test();
    }

    #[test]
    fn mc_s4_pane_death_at_revision_boundary_retains_empty_definitions() {
        let mut app = app();
        let pane = app.state.workspaces[0].tabs[2].layout.pane_ids()[0];
        let pane_id = app.public_pane_id(0, pane).unwrap();
        let mission = call(
            &mut app,
            "mission.create",
            serde_json::json!({"name":"Empty"}),
        )["result"]["mission"]
            .clone();
        call(
            &mut app,
            "mission.assign_pane",
            serde_json::json!({"pane_id":pane_id,"mission_id":mission["id"]}),
        );
        app.state.organization.revision = u64::MAX - 1;
        assert_eq!(
            call(
                &mut app,
                "mission.create",
                serde_json::json!({"name":"Overflow"})
            )["error"]["code"],
            "organization_revision_exhausted"
        );
        app.state
            .handle_app_event(crate::events::AppEvent::PaneDied {
                pane_id: pane,
                exit_reason: crate::platform::ChildExitReason::Exited,
            });
        let result = call(&mut app, "organization.get", serde_json::json!({}));
        assert_eq!(
            result["result"]["organization"]["pane_mission_assignments"],
            serde_json::json!([])
        );
        assert_eq!(result["result"]["organization"]["revision"], u64::MAX);
        assert_eq!(
            result["result"]["organization"]["missions"],
            serde_json::json!([mission])
        );
        app.state.assert_invariants_for_test();
    }

    #[test]
    fn mc_s4_pane_override_reassign_clear_preserves_tab_and_terminal() {
        let mut app = app();
        let tab_id = app.public_tab_id(0, 2).unwrap();
        let pane_id = app
            .public_pane_id(0, app.state.workspaces[0].tabs[2].layout.pane_ids()[0])
            .unwrap();
        let before = app.session_snapshot();
        let runtime = call(
            &mut app,
            "mission.create",
            serde_json::json!({"name":"Agent runtime"}),
        )["result"]["mission"]["id"]
            .clone();
        let tako = call(
            &mut app,
            "mission.create",
            serde_json::json!({"name":"Tako platform"}),
        )["result"]["mission"]["id"]
            .clone();
        call(
            &mut app,
            "mission.assign",
            serde_json::json!({"target":{"kind":"tab","tab_id":tab_id},"mission_id":runtime}),
        );
        for id in [&runtime, &tako] {
            let result = call(
                &mut app,
                "mission.assign_pane",
                serde_json::json!({"pane_id":pane_id,"mission_id":id}),
            );
            assert_eq!(
                result["result"]["organization"]["pane_mission_assignments"],
                serde_json::json!([{"pane_id":pane_id,"mission_id":id}])
            );
        }
        let confirmed = app.state.organization.clone();
        app.state.session_dirty = false;
        call(
            &mut app,
            "mission.assign_pane",
            serde_json::json!({"pane_id":pane_id,"mission_id":tako}),
        );
        assert!(!app.state.session_dirty);
        for (pane, mission, code) in [
            ("p_1_1", tako.clone(), "pane_not_found"),
            (&pane_id, serde_json::json!("missing"), "mission_not_found"),
        ] {
            assert_eq!(
                call(
                    &mut app,
                    "mission.assign_pane",
                    serde_json::json!({"pane_id":pane,"mission_id":mission})
                )["error"]["code"],
                code
            );
            assert_eq!(app.state.organization, confirmed);
        }
        let cleared = call(
            &mut app,
            "mission.clear_pane_override",
            serde_json::json!({"pane_id":pane_id}),
        );
        assert_eq!(
            cleared["result"]["organization"]["pane_mission_assignments"],
            serde_json::json!([])
        );
        assert_eq!(
            cleared["result"]["organization"]["mission_assignments"][0]["mission_id"],
            runtime
        );
        app.state.session_dirty = false;
        call(
            &mut app,
            "mission.clear_pane_override",
            serde_json::json!({"pane_id":pane_id}),
        );
        assert!(!app.state.session_dirty);
        assert_eq!(app.session_snapshot(), before);
        app.state.assert_invariants_for_test();
    }

    #[test]
    fn mc_s3_create_and_assign_existing_tab_preserves_terminal_work() {
        let mut app = app();
        let before = app.session_snapshot();
        let tab_id = app.public_tab_id(0, 1).unwrap();
        let created = call(
            &mut app,
            "mission.create",
            serde_json::json!({"name":" Tako platform ","objective":"Ship the prototype"}),
        );
        let mission = &created["result"]["mission"];
        assert_eq!(mission["name"], "Tako platform");
        assert_eq!(mission["objective"], "Ship the prototype");
        assert_eq!(mission["order"], 0);
        assert!(!mission["id"].as_str().unwrap().is_empty());
        let assigned = call(
            &mut app,
            "mission.assign",
            serde_json::json!({"mission_id":mission["id"],"target":{"kind":"tab","tab_id":tab_id}}),
        );
        let read = call(&mut app, "organization.get", serde_json::json!({}));
        assert_eq!(
            assigned["result"]["organization"],
            read["result"]["organization"]
        );
        assert_eq!(
            read["result"]["organization"]["mission_assignments"],
            serde_json::json!([
                {"mission_id":mission["id"],"target":{"kind":"tab","tab_id":tab_id}}
            ])
        );
        assert_eq!(app.session_snapshot(), before);
        app.state.assert_invariants_for_test();
    }
    #[test]
    fn mc_s3_reassignment_and_invalid_catalog_admission_are_atomic() {
        let mut app = app();
        let tab_id = app.public_tab_id(0, 1).unwrap();
        let target = serde_json::json!({"kind":"tab","tab_id":tab_id});
        let first = call(
            &mut app,
            "mission.create",
            serde_json::json!({"name":"Same"}),
        )["result"]["mission"]["id"]
            .clone();
        let second = call(
            &mut app,
            "mission.create",
            serde_json::json!({"name":"Same"}),
        )["result"]["mission"]["id"]
            .clone();
        let before = app.session_snapshot();
        for id in [first, second.clone()] {
            let result = call(
                &mut app,
                "mission.assign",
                serde_json::json!({"mission_id":id,"target":target}),
            );
            assert_eq!(
                result["result"]["organization"]["mission_assignments"],
                serde_json::json!([{ "mission_id":id,"target":target }])
            );
        }
        let confirmed = app.state.organization.clone();
        app.state.session_dirty = false;
        call(
            &mut app,
            "mission.assign",
            serde_json::json!({"mission_id":second,"target":target}),
        );
        assert_eq!(app.state.organization, confirmed);
        assert!(!app.state.session_dirty);
        for (method, params, code) in [
            (
                "mission.create",
                serde_json::json!({"name":"  "}),
                "invalid_mission_name",
            ),
            (
                "mission.assign",
                serde_json::json!({"mission_id":"absent","target":target}),
                "mission_not_found",
            ),
            (
                "mission.assign",
                serde_json::json!({"mission_id":second,"target":{"kind":"tab","tab_id":"t_1_2"}}),
                "tab_not_found",
            ),
        ] {
            assert_eq!(call(&mut app, method, params)["error"]["code"], code);
            assert_eq!(app.state.organization, confirmed);
            assert!(!app.state.session_dirty);
        }
        let mut disk = serde_json::to_value(&confirmed).unwrap();
        disk["mission_assignments"][0]["mission_id"] = serde_json::json!("absent");
        assert!(
            serde_json::from_value::<crate::organization::OrganizationState>(disk).is_err(),
            "invalid mission reference cannot enter through disk or endpoint JSON"
        );
        assert_eq!(app.session_snapshot(), before);
        app.state.assert_invariants_for_test();
    }

    #[test]
    fn mc_s3_rename_reorder_and_close_keep_empty_mission() {
        let mut app = app();
        let tab_id = app.public_tab_id(0, 1).unwrap();
        let mission = call(
            &mut app,
            "mission.create",
            serde_json::json!({"name":"Tako platform"}),
        )["result"]["mission"]
            .clone();
        call(
            &mut app,
            "mission.assign",
            serde_json::json!({"mission_id":mission["id"],"target":{"kind":"tab","tab_id":tab_id}}),
        );
        for (method, params) in [
            (
                "tab.rename",
                serde_json::json!({"tab_id":tab_id,"label":"Prototype"}),
            ),
            (
                "tab.move",
                serde_json::json!({"tab_id":tab_id,"insert_index":0}),
            ),
        ] {
            assert!(call(&mut app, method, params).get("error").is_none());
            let catalog = call(&mut app, "organization.get", serde_json::json!({}));
            assert_eq!(
                catalog["result"]["organization"]["mission_assignments"][0]["target"]["tab_id"],
                tab_id
            );
            app.state.assert_invariants_for_test();
        }
        call(&mut app, "tab.close", serde_json::json!({"tab_id":tab_id}));
        let catalog = call(&mut app, "organization.get", serde_json::json!({}));
        assert_eq!(
            catalog["result"]["organization"]["mission_assignments"],
            serde_json::json!([])
        );
        assert_eq!(
            catalog["result"]["organization"]["missions"],
            serde_json::json!([mission])
        );
        app.state.assert_invariants_for_test();
    }

    #[test]
    fn mc_s3_terminal_death_cleans_assignments_at_revision_boundary() {
        let mut app = app();
        let tab_id = app.public_tab_id(0, 1).unwrap();
        let mission = call(
            &mut app,
            "mission.create",
            serde_json::json!({"name":"Retained"}),
        )["result"]["mission"]
            .clone();
        call(
            &mut app,
            "mission.assign",
            serde_json::json!({"mission_id":mission["id"],"target":{"kind":"tab","tab_id":tab_id}}),
        );
        app.state.organization.revision = u64::MAX - 1;
        assert_eq!(
            call(
                &mut app,
                "mission.create",
                serde_json::json!({"name":"Overflow"})
            )["error"]["code"],
            "organization_revision_exhausted"
        );
        let panes = app.state.workspaces[0].tabs[1].layout.pane_ids();
        for pane_id in panes {
            app.state
                .handle_app_event(crate::events::AppEvent::PaneDied {
                    pane_id,
                    exit_reason: crate::platform::ChildExitReason::Exited,
                });
            app.state.assert_invariants_for_test();
        }
        let catalog = call(&mut app, "organization.get", serde_json::json!({}));
        assert_eq!(
            catalog["result"]["organization"]["mission_assignments"],
            serde_json::json!([])
        );
        assert_eq!(catalog["result"]["organization"]["revision"], u64::MAX);
        assert_eq!(
            catalog["result"]["organization"]["missions"],
            serde_json::json!([mission])
        );
        let mut disk = catalog["result"]["organization"].clone();
        disk["missions"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"id":"mission_extra","name":"Empty","order":1}));
        disk["mission_assignments"] = serde_json::json!([{ "mission_id":"mission_extra","target":{"kind":"tab","tab_id":tab_id}}]);
        assert!(
            serde_json::from_value::<crate::organization::OrganizationState>(disk).is_err(),
            "unsafe revision cannot be admitted with live cleanup obligations"
        );
    }
    #[test]
    fn mc_s3_transfer_split_tab_preserves_mission_terminal_and_focus() {
        let mut app = app();
        app.state
            .workspaces
            .push(crate::workspace::Workspace::test_new("Destination"));
        app.state.ensure_test_terminals();
        let source = app.public_tab_id(0, 2).unwrap();
        let destination = app.public_workspace_id(1);
        let mission = call(
            &mut app,
            "mission.create",
            serde_json::json!({"name":"Tako platform"}),
        )["result"]["mission"]
            .clone();
        call(
            &mut app,
            "mission.assign",
            serde_json::json!({"mission_id":mission["id"],"target":{"kind":"tab","tab_id":source}}),
        );
        app.state.switch_workspace_tab(0, 2);
        let before_tab =
            serde_json::to_value(&app.capture_session_snapshot().workspaces[0].tabs[2]).unwrap();
        let before = app.session_snapshot();
        let failed = call(
            &mut app,
            "tab.transfer",
            serde_json::json!({"tab_id":source,"workspace_id":destination,"insert_index":999}),
        );
        assert_eq!(failed["error"]["code"], "tab_transfer_failed");
        assert_eq!(app.session_snapshot(), before);
        assert_eq!(
            call(&mut app, "organization.get", serde_json::json!({}))["result"]["organization"]
                ["mission_assignments"][0]["target"]["tab_id"],
            source
        );
        let result = call(
            &mut app,
            "tab.transfer",
            serde_json::json!({"tab_id":source,"workspace_id":destination,"insert_index":0}),
        );
        let moved_id = result["result"]["tab"]["tab_id"]
            .as_str()
            .expect("moved public identity");
        assert_ne!(moved_id, source);
        let after_tab =
            serde_json::to_value(&app.capture_session_snapshot().workspaces[1].tabs[0]).unwrap();
        assert_eq!(
            after_tab, before_tab,
            "only workspace-scoped public numbering changes"
        );
        assert_eq!(
            app.session_snapshot().focused_tab_id.as_deref(),
            Some(moved_id)
        );
        assert_eq!(
            call(&mut app, "organization.get", serde_json::json!({}))["result"]["organization"]
                ["mission_assignments"],
            serde_json::json!([{"mission_id":mission["id"],"target":{"kind":"tab","tab_id":moved_id}}])
        );
        app.state.assert_invariants_for_test();
    }

    #[test]
    fn mc_s3_transfer_then_close_prunes_dead_public_targets() {
        let mut app = app();
        app.state
            .workspaces
            .push(crate::workspace::Workspace::test_new("Destination"));
        app.state.ensure_test_terminals();
        let source = app.public_tab_id(0, 2).unwrap();
        let destination = app.public_workspace_id(1);
        let result = call(
            &mut app,
            "tab.transfer",
            serde_json::json!({"tab_id":source,"workspace_id":destination,"insert_index":0}),
        );
        let tab_id = result["result"]["tab"]["tab_id"].clone();
        call(&mut app, "tab.close", serde_json::json!({"tab_id":tab_id}));
        app.state.assert_invariants_for_test();
    }
}

#[cfg(test)]
mod maintenance_tests;
