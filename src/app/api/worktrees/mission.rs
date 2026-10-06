//! Contextual creation publishes family and initial-tab membership together.
use std::path::{Path, PathBuf};

use crate::api::schema::ResponseResult;
use crate::app::{App, AppState};
use crate::events::ApiWorktreeAddRequest;
use crate::organization::{FamilyId, MissionTarget};

use super::super::responses::{encode_error, encode_success};
use super::{worktree_membership, WorktreeSource};

impl App {
    pub(super) fn finish_mission_worktree_create(
        &mut self,
        api: ApiWorktreeAddRequest,
        path: PathBuf,
    ) {
        let response = self.commit_mission_worktree_create(&api, &path);
        Self::send_api_response(api.respond_to, response);
    }

    fn commit_mission_worktree_create(
        &mut self,
        api: &ApiWorktreeAddRequest,
        path: &Path,
    ) -> String {
        let partial = |code, reason: String| {
            encode_error(
                api.id.clone(),
                code,
                format!("created worktree at {} but failed to open in mission: {reason}; checkout retained", path.display()),
            )
        };
        let Some(mission) = api.mission_id.clone() else {
            return partial("mission_not_found", "mission intent missing".into());
        };
        if !self
            .state
            .organization
            .missions
            .iter()
            .any(|m| m.id == mission)
        {
            return partial(
                "mission_not_found",
                "selected mission no longer exists".into(),
            );
        }
        let mut source = WorktreeSource {
            workspace_idx: self.api_create_source_workspace_idx(api),
            source_checkout_path: api.source_checkout_path.clone(),
            source_repo_root: api.source_repo_root.clone(),
            repo_key: api.repo_key.clone(),
            repo_name: api.repo_name.clone(),
        };
        let existing_target = self.open_workspace_idx_for_checkout(path);
        // Preflight every fallible organization operation before starting a terminal.
        // The placeholder exists only in this local plan; final public IDs replace it
        // before publication, without a second fallible revision increment.
        let mut organization = self.state.organization.clone();
        let managed = FamilyId::Managed {
            key: source.repo_key.clone(),
        };
        for index in [source.workspace_idx, existing_target]
            .into_iter()
            .flatten()
        {
            let old = AppState::family_id(&self.state.workspaces[index]);
            if matches!(old, FamilyId::Standalone { .. }) {
                if let Err(code) = organization.transfer_family(&old, managed.clone()) {
                    return partial(code, code.into());
                }
            }
        }
        let target = existing_target
            .and_then(|idx| self.tab_info(idx, 0))
            .map(|tab| MissionTarget::Tab { tab_id: tab.tab_id })
            .unwrap_or_else(|| MissionTarget::Tab {
                tab_id: "__unpublished_worktree_initial_tab".into(),
            });
        if let Err(code) =
            organization.assign_mission(target.clone(), mission, std::slice::from_ref(&target))
        {
            return partial(code, code.into());
        }
        let previous_focus = (self.state.active, self.state.selected, self.state.mode);
        let (target_index, created_target) = match existing_target {
            Some(index) => (index, false),
            None => match self.create_workspace_with_options(path.to_path_buf(), false) {
                Ok(index) => (index, true),
                Err(err) => return partial("worktree_open_failed", err.to_string()),
            },
        };
        let created_parent = source.workspace_idx.is_none();
        if created_parent {
            match self
                .create_workspace_with_options(source.source_checkout_path.to_path_buf(), false)
            {
                Ok(index) => source.workspace_idx = Some(index),
                Err(err) => {
                    if created_target {
                        self.state.close_workspaces(vec![target_index]);
                        self.shutdown_detached_terminal_runtimes();
                    }
                    (self.state.active, self.state.selected, self.state.mode) = previous_focus;
                    return partial("worktree_open_failed", err.to_string());
                }
            }
        }
        // A concurrent open may already have additional tabs. Contextual creation
        // assigns the initial tab, regardless of the user's current active tab.
        let tab_index = 0;
        let tab = self
            .tab_info(target_index, tab_index)
            .expect("allocated workspace has initial tab");
        for assignment in &mut organization.mission_assignments {
            if assignment.target == target {
                assignment.target = MissionTarget::Tab {
                    tab_id: tab.tab_id.clone(),
                };
            }
        }
        // No yielding or event publication between preflight and this commit.
        if let Some(index) = source.workspace_idx {
            self.state.workspaces[index].worktree_space = Some(worktree_membership(
                &source,
                source.source_checkout_path.clone(),
                false,
            ));
        }
        self.state.workspaces[target_index].worktree_space =
            Some(worktree_membership(&source, path.to_path_buf(), true));
        self.state.organization = organization;
        if let Some(label) = &api.label {
            self.state.workspaces[target_index].set_custom_name(label.clone());
        }
        if api.focus {
            self.state.switch_workspace(target_index);
        }
        self.state.mark_session_dirty();
        if let Some(index) = source.workspace_idx {
            if created_parent {
                self.emit_workspace_open_events(index);
            } else {
                self.emit_workspace_updated(index);
            }
        }
        if created_target {
            self.emit_workspace_open_events(target_index);
        } else {
            self.emit_workspace_updated(target_index);
        }
        let worktree = self
            .worktree_info_for_workspace(target_index)
            .expect("committed worktree membership");
        self.emit_worktree_created_event(target_index, worktree.clone());
        encode_success(
            api.id.clone(),
            ResponseResult::WorktreeCreated {
                workspace: self.workspace_info(target_index),
                tab,
                root_pane: self
                    .root_pane_info(target_index, tab_index)
                    .expect("allocated workspace has root pane"),
                worktree,
            },
        )
    }
}
