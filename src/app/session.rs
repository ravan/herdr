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
