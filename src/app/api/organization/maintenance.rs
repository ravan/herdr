use super::super::responses::encode_error;
use crate::api::schema::*;
use crate::app::App;

impl App {
    pub(in crate::app::api) fn handle_collection_rename(
        &mut self,
        id: String,
        params: CollectionRenameParams,
    ) -> String {
        let result = crate::organization::CollectionName::try_from(params.name).and_then(|name| {
            self.state
                .organization
                .rename_collection(&params.collection_id, name)
        });
        match result {
            Ok(changed) => {
                if changed {
                    self.state.mark_session_dirty();
                }
                self.handle_organization_get(id)
            }
            Err(code) => encode_error(id, code, "Cannot rename collection"),
        }
    }
}

impl App {
    pub(in crate::app::api) fn handle_mission_rename(
        &mut self,
        id: String,
        params: MissionRenameParams,
    ) -> String {
        let result = crate::organization::MissionName::try_from(params.name).and_then(|name| {
            self.state
                .organization
                .rename_mission(&params.mission_id, name)
        });
        match result {
            Ok(changed) => {
                if changed {
                    self.state.mark_session_dirty();
                }
                self.handle_organization_get(id)
            }
            Err(code) => encode_error(id, code, "Cannot mission.rename"),
        }
    }
}

impl App {
    pub(in crate::app::api) fn handle_collection_move(
        &mut self,
        id: String,
        params: CollectionMoveParams,
    ) -> String {
        let result = self
            .state
            .organization
            .move_collection(&params.collection_id, params.to_index);
        match result {
            Ok(changed) => {
                if changed {
                    self.state.mark_session_dirty();
                }
                self.handle_organization_get(id)
            }
            Err(code) => encode_error(id, code, "Cannot collection.move"),
        }
    }
}

impl App {
    pub(in crate::app::api) fn handle_mission_move(
        &mut self,
        id: String,
        params: MissionMoveParams,
    ) -> String {
        let result = self
            .state
            .organization
            .move_mission(&params.mission_id, params.to_index);
        match result {
            Ok(changed) => {
                if changed {
                    self.state.mark_session_dirty();
                }
                self.handle_organization_get(id)
            }
            Err(code) => encode_error(id, code, "Cannot mission.move"),
        }
    }
}

impl App {
    pub(in crate::app::api) fn handle_mission_set_objective(
        &mut self,
        id: String,
        params: MissionSetObjectiveParams,
    ) -> String {
        let result = self
            .state
            .organization
            .set_mission_objective(&params.mission_id, params.objective);
        match result {
            Ok(changed) => {
                if changed {
                    self.state.mark_session_dirty();
                }
                self.handle_organization_get(id)
            }
            Err(code) => encode_error(id, code, "Cannot mission.set_objective"),
        }
    }
}

impl App {
    pub(in crate::app::api) fn handle_collection_delete(
        &mut self,
        id: String,
        params: CollectionDeleteParams,
    ) -> String {
        let result = self
            .state
            .organization
            .delete_collection(&params.collection_id);
        match result {
            Ok(changed) => {
                if changed {
                    self.state.mark_session_dirty();
                }
                self.handle_organization_get(id)
            }
            Err(code) => encode_error(id, code, "Cannot collection.delete"),
        }
    }
}

impl App {
    pub(in crate::app::api) fn handle_collection_unassign_family(
        &mut self,
        id: String,
        params: CollectionUnassignFamilyParams,
    ) -> String {
        let result = self.state.organization.unassign_family(
            &params.family_id,
            &params.collection_id,
            &self
                .state
                .workspaces
                .iter()
                .map(crate::app::AppState::family_id)
                .collect::<Vec<_>>(),
        );
        match result {
            Ok(changed) => {
                if changed {
                    self.state.mark_session_dirty();
                }
                self.handle_organization_get(id)
            }
            Err(code) => encode_error(id, code, "Cannot collection.unassign_family"),
        }
    }
}

impl App {
    pub(in crate::app::api) fn handle_mission_unassign(
        &mut self,
        id: String,
        params: MissionUnassignParams,
    ) -> String {
        let result = self.state.organization.unassign_mission(
            &params.target,
            &params.mission_id,
            &self.state.live_mission_targets(),
        );
        match result {
            Ok(changed) => {
                if changed {
                    self.state.mark_session_dirty();
                }
                self.handle_organization_get(id)
            }
            Err(code) => encode_error(id, code, "Cannot mission.unassign"),
        }
    }
}

impl App {
    pub(in crate::app::api) fn handle_mission_delete(
        &mut self,
        id: String,
        params: MissionDeleteParams,
    ) -> String {
        let result = self.state.organization.delete_mission(&params.mission_id);
        match result {
            Ok(changed) => {
                if changed {
                    self.state.mark_session_dirty();
                }
                self.handle_organization_get(id)
            }
            Err(code) => encode_error(id, code, "Cannot mission.delete"),
        }
    }
}
