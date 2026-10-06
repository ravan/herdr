use super::responses::{encode_error, encode_success};
use crate::api::schema::{CollectionAssignFamilyParams, CollectionCreateParams, ResponseResult};
use crate::app::App;

impl App {
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
