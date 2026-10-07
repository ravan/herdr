use std::time::{Duration, Instant};

use super::{App, SESSION_SAVE_DEBOUNCE};

enum SessionSaveJob {
    Clear,
    Save {
        snapshot: Box<crate::persist::SessionSnapshot>,
        history: Option<crate::persist::SessionHistorySnapshot>,
    },
}

impl App {
    pub(crate) fn capture_session_snapshot(&self) -> crate::persist::SessionSnapshot {
        let mut snapshot = crate::persist::capture(
            &self.state.workspaces,
            &self.state.terminals,
            &self.terminal_runtimes,
            self.state.active,
            self.state.selected,
        );
        snapshot.organization = self.state.organization.clone();
        snapshot
    }
    pub(super) fn schedule_session_save(&mut self) {
        if self.policy.persist_session {
            self.pane_exit_checkpoint_pending = false;
            self.session_save_deadline = Some(Instant::now() + SESSION_SAVE_DEBOUNCE);
        }
    }

    pub(crate) fn sync_session_save_schedule(&mut self) {
        if self.state.session_dirty {
            self.state.session_dirty = false;
            self.schedule_session_save();
        }
    }

    fn reap_finished_session_save(&mut self) {
        if self
            .session_save_thread
            .as_ref()
            .is_some_and(std::thread::JoinHandle::is_finished)
        {
            if let Some(thread) = self.session_save_thread.take() {
                let _ = thread.join();
            }
        }
    }

    fn capture_session_save_job(&self) -> SessionSaveJob {
        if self.state.workspaces.is_empty() && self.state.organization.is_empty() {
            SessionSaveJob::Clear
        } else {
            let snapshot = self.capture_session_snapshot();
            let history = self.persist_pane_history.then(|| {
                crate::persist::capture_history(
                    &snapshot,
                    &self.state.workspaces,
                    &self.terminal_runtimes,
                )
            });
            SessionSaveJob::Save {
                snapshot: Box::new(snapshot),
                history,
            }
        }
    }

    pub(crate) fn start_background_session_save(&mut self) {
        if !self.policy.persist_session {
            self.session_save_deadline = None;
            return;
        }

        self.reap_finished_session_save();
        if self.session_save_thread.is_some() {
            self.session_save_deadline = Some(Instant::now() + Duration::from_millis(250));
            return;
        }

        let job = self.capture_session_save_job();
        self.pane_exit_checkpoint_pending = false;
        self.session_save_deadline = None;
        let writer = self.session_writer.clone();
        match std::thread::Builder::new()
            .name("herdr-session-save".into())
            .spawn(move || run_session_save_job(job, &writer))
        {
            Ok(thread) => self.session_save_thread = Some(thread),
            Err(err) => {
                tracing::warn!(err = %err, "failed to spawn session save thread; saving inline");
                run_session_save_job(self.capture_session_save_job(), &self.session_writer);
            }
        }
    }

    pub(crate) fn save_session_now(&mut self) {
        if let Some(thread) = self.session_save_thread.take() {
            let _ = thread.join();
        }

        if !self.policy.persist_session {
            self.session_save_deadline = None;
            return;
        }

        run_session_save_job(self.capture_session_save_job(), &self.session_writer);
        self.pane_exit_checkpoint_pending = false;
        self.session_save_deadline = None;
    }

    pub(crate) fn checkpoint_session_before_pane_exit(&mut self) {
        if !self.policy.persist_session
            || (self.pane_exit_checkpoint_pending && !self.state.session_dirty)
        {
            return;
        }
        self.save_session_now();
        self.pane_exit_checkpoint_pending = true;
        self.state.session_dirty = false;
    }

    pub(crate) fn finish_checkpointed_pane_exit(&mut self) {
        if self.pane_exit_checkpoint_pending {
            self.state.session_dirty = false;
            self.session_save_deadline = Some(Instant::now() + SESSION_SAVE_DEBOUNCE);
        }
    }

    pub(crate) fn save_session_on_shutdown(&mut self) {
        if self.pane_exit_checkpoint_pending && !self.state.session_dirty {
            self.session_save_deadline = None;
            return;
        }
        self.save_session_now();
    }
}

fn run_session_save_job(
    job: SessionSaveJob,
    writer: &std::sync::Mutex<crate::persist::SessionWriter>,
) {
    let mut writer = match writer.lock() {
        Ok(writer) => writer,
        Err(err) => {
            tracing::warn!(err = %err, "session writer is poisoned; refusing to modify session");
            return;
        }
    };
    match job {
        SessionSaveJob::Clear => writer.clear(),
        SessionSaveJob::Save { snapshot, history } => writer.save(&snapshot, history.as_ref()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn mc_s1_empty_collection_capture_and_handoff_keep_the_same_catalog() {
        let config = crate::config::Config::default();
        let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &config,
            crate::app::AppPolicy::TEST,
            None,
            rx,
            crate::api::EventHub::default(),
        );
        app.state
            .create_collection("Agent workshop".into())
            .unwrap();
        let snapshot = app.capture_session_snapshot();
        let encoded = serde_json::to_string(&snapshot).unwrap();
        let parsed: crate::persist::SessionSnapshot = serde_json::from_str(&encoded).unwrap();
        let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let restored = App::new_from_handoff(
            &config,
            None,
            rx,
            crate::api::EventHub::default(),
            &parsed,
            &mut std::collections::HashMap::new(),
        )
        .unwrap();
        assert!(restored.state.workspaces.is_empty());
        assert_eq!(restored.state.organization, app.state.organization);
        assert_eq!(restored.state.organization.revision, 1);
        restored.state.assert_invariants_for_test();
    }

    #[test]
    fn mc_s1_empty_collection_is_saved_after_the_last_workspace_closes() {
        let root =
            std::env::temp_dir().join(format!("herdr-mc-s1-empty-save-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let path = root.join("session.json");
        let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &crate::config::Config::default(),
            crate::app::AppPolicy {
                persist_session: true,
                ..crate::app::AppPolicy::TEST
            },
            None,
            rx,
            crate::api::EventHub::default(),
        );
        app.session_writer = std::sync::Arc::new(std::sync::Mutex::new(
            crate::persist::SessionWriter::at_path(path.clone(), false),
        ));
        app.state = crate::app::AppState::test_with_adversarial_identity_state();
        let collection = app
            .state
            .create_collection("Agent workshop".into())
            .unwrap();
        let family = crate::app::AppState::family_id(&app.state.workspaces[0]);
        app.state
            .assign_family_to_collection(family, collection.id.clone())
            .unwrap();
        app.state.close_workspaces(vec![0]);
        app.state.assert_invariants_for_test();
        assert!(app.state.workspaces.is_empty());
        app.save_session_now();
        let snapshot: crate::persist::SessionSnapshot = serde_json::from_str(
            &std::fs::read_to_string(path).expect("organization-only session is retained on disk"),
        )
        .unwrap();
        assert!(snapshot.workspaces.is_empty());
        assert_eq!(snapshot.organization.collections, vec![collection]);
        assert_eq!(snapshot.organization.revision, 3);
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(all(test, unix))]
mod hibernate_tests {
    use super::*;

    #[test]
    fn mc_s2_parked_empty_collection_survives_save_load_and_handoff() {
        let root = std::env::temp_dir().join(format!("herdr-mc-s2-save-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let path = root.join("session.json");
        let config = crate::config::Config::default();
        let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &config,
            crate::app::AppPolicy {
                persist_session: true,
                ..crate::app::AppPolicy::TEST
            },
            None,
            rx,
            crate::api::EventHub::default(),
        );
        app.session_writer = std::sync::Arc::new(std::sync::Mutex::new(
            crate::persist::SessionWriter::at_path(path.clone(), false),
        ));
        app.state = crate::app::AppState::test_with_adversarial_identity_state();
        let id = app
            .state
            .create_collection("Side quests".into())
            .unwrap()
            .id;
        app.state.set_collection_hibernating(id, true).unwrap();
        let populated = app.capture_session_snapshot();
        assert!(populated.organization.collections[0].hibernating);
        app.state.close_workspaces(vec![0]);
        app.save_session_now();
        let saved: crate::persist::SessionSnapshot =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert!(saved.workspaces.is_empty());
        assert!(saved.organization.collections[0].hibernating);
        let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let restored = App::new_from_handoff(
            &config,
            None,
            rx,
            crate::api::EventHub::default(),
            &saved,
            &mut std::collections::HashMap::new(),
        )
        .unwrap();
        assert_eq!(restored.state.organization, app.state.organization);
        restored.state.assert_invariants_for_test();
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod mission_tests {
    use super::*;
    #[tokio::test]
    async fn mc_s3_capture_restore_resolves_public_tab_numbers_and_reports_stale_targets() {
        let mut config = crate::config::Config::default();
        // These assertions cover restored identity, not an interactive shell lifecycle.
        config.terminal.default_shell = crate::app::exiting_test_command().into();
        let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &config,
            crate::app::AppPolicy::TEST,
            None,
            rx,
            crate::api::EventHub::default(),
        );
        app.state = crate::app::AppState::test_with_adversarial_identity_state();
        let workspace = app.state.workspaces[0].id.clone();
        let id = crate::workspace::public_tab_id_for_number(&workspace, 4);
        let mission = app
            .state
            .create_mission("Tako platform".into(), Some("Ship".into()))
            .unwrap();
        app.state
            .assign_mission(
                crate::organization::MissionTarget::Tab { tab_id: id.clone() },
                mission.id.clone(),
            )
            .unwrap();
        let snapshot = app.capture_session_snapshot();
        let mut raw = serde_json::to_value(&snapshot).unwrap();
        raw["organization"]["mission_assignments"].as_array_mut().unwrap().push(serde_json::json!({"mission_id":mission.id,"target":{"kind":"tab","tab_id":format!("{workspace}:t2")}}));
        let parsed: crate::persist::SessionSnapshot = serde_json::from_value(raw).unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(32);
        let (workspaces, terminals, runtimes) = crate::persist::restore(
            &parsed,
            None,
            24,
            80,
            config.advanced.scrollback_limit_bytes,
            &config.terminal.default_shell,
            config.terminal.shell_mode,
            false,
            tx,
            std::sync::Arc::new(tokio::sync::Notify::new()),
            std::sync::Arc::new(crate::render_signal::RenderSignal::default()),
        );
        app.state.workspaces = workspaces;
        app.state.terminals = terminals;
        app.terminal_runtimes = runtimes.into();
        app.state.organization = parsed.organization;
        assert_eq!(
            app.state.reconcile_mission_targets().unwrap(),
            vec![crate::organization::MissionTarget::Tab {
                tab_id: format!("{workspace}:t2")
            }]
        );
        assert_eq!(
            app.state
                .organization
                .mission_for(&crate::organization::MissionTarget::Tab { tab_id: id }),
            Some(&mission.id)
        );
        app.state.assert_invariants_for_test();
        for (_, runtime) in app.terminal_runtimes.drain() {
            runtime.shutdown();
        }
    }
    #[tokio::test]
    async fn mc_s4_capture_restore_keeps_pane_overrides_after_final_public_maps() {
        let mut config = crate::config::Config::default();
        // An exiting command keeps Windows runtime cleanup independent of shell input.
        config.terminal.default_shell = crate::app::exiting_test_command().into();
        let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &config,
            crate::app::AppPolicy::TEST,
            None,
            rx,
            crate::api::EventHub::default(),
        );
        app.state = crate::app::AppState::test_with_adversarial_identity_state();
        let workspace = app.state.workspaces[0].id.clone();
        let runtime_pane = app.state.workspaces[0].tabs[2].layout.pane_ids()[1];
        let id = app.public_pane_id(0, runtime_pane).unwrap();
        let mission = app
            .state
            .create_mission("Tako platform".into(), Some("Ship".into()))
            .unwrap();
        app.state
            .assign_pane_mission(id.clone(), mission.id.clone())
            .unwrap();
        let snapshot = app.capture_session_snapshot();
        let mut raw = serde_json::to_value(&snapshot).unwrap();
        raw["organization"]["pane_mission_assignments"]
            .as_array_mut()
            .unwrap()
            .push(
                serde_json::json!({"mission_id":mission.id,"pane_id":format!("{workspace}:p999")}),
            );
        let parsed: crate::persist::SessionSnapshot = serde_json::from_value(raw).unwrap();
        let (tx, _rx) = tokio::sync::mpsc::channel(32);
        let (workspaces, terminals, runtimes) = crate::persist::restore(
            &parsed,
            None,
            24,
            80,
            config.advanced.scrollback_limit_bytes,
            &config.terminal.default_shell,
            config.terminal.shell_mode,
            false,
            tx,
            std::sync::Arc::new(tokio::sync::Notify::new()),
            std::sync::Arc::new(crate::render_signal::RenderSignal::default()),
        );
        app.state.workspaces = workspaces;
        app.state.terminals = terminals;
        app.terminal_runtimes = runtimes.into();
        app.state.organization = parsed.organization;
        assert_ne!(
            app.state.workspaces[0].tabs[2].layout.pane_ids()[1],
            runtime_pane,
            "cold restore allocates new runtime pane IDs"
        );
        assert_eq!(
            app.state
                .organization
                .retain_live_pane_missions(&app.state.live_mission_panes())
                .unwrap(),
            vec![format!("{workspace}:p999")]
        );
        assert_eq!(app.state.organization.pane_override(&id), Some(&mission.id));
        app.state.assert_invariants_for_test();
        for (_, runtime) in app.terminal_runtimes.drain() {
            runtime.shutdown();
        }
    }
    #[cfg(unix)]
    #[test]
    fn mc_s3_empty_mission_save_load_and_handoff_retains_definition() {
        let root = std::env::temp_dir().join(format!("herdr-mc-s3-empty-{}", std::process::id()));
        let path = root.join("session.json");
        let config = crate::config::Config::default();
        let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(
            &config,
            crate::app::AppPolicy {
                persist_session: true,
                ..crate::app::AppPolicy::TEST
            },
            None,
            rx,
            crate::api::EventHub::default(),
        );
        app.session_writer = std::sync::Arc::new(std::sync::Mutex::new(
            crate::persist::SessionWriter::at_path(path.clone(), false),
        ));
        app.state = crate::app::AppState::test_with_adversarial_identity_state();
        let id = crate::workspace::public_tab_id_for_number(&app.state.workspaces[0].id, 4);
        let mission = app
            .state
            .create_mission("Retained empty".into(), None)
            .unwrap();
        app.state
            .assign_mission(
                crate::organization::MissionTarget::Tab { tab_id: id },
                mission.id.clone(),
            )
            .unwrap();
        app.state.close_workspaces(vec![0]);
        app.save_session_now();
        let parsed: crate::persist::SessionSnapshot =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let restored = App::new_from_handoff(
            &config,
            None,
            rx,
            crate::api::EventHub::default(),
            &parsed,
            &mut std::collections::HashMap::new(),
        )
        .unwrap();
        assert_eq!(restored.state.organization.missions, vec![mission]);
        assert!(restored.state.organization.mission_assignments.is_empty());
        assert_eq!(restored.state.organization, app.state.organization);
        restored.state.assert_invariants_for_test();
        std::fs::remove_dir_all(root).unwrap();
    }
}
