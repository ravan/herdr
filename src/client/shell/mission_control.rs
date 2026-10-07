use super::render::display_width;
use super::*;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
mod members;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum MissionControlView {
    #[default]
    Spaces,
    Missions,
    NeedsYou,
}

impl MissionControlView {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Spaces => "Spaces",
            Self::Missions => "Missions",
            Self::NeedsYou => "Needs you",
        }
    }

    fn next(self, backwards: bool) -> Self {
        match (self, backwards) {
            (Self::Spaces, false) | (Self::NeedsYou, true) => Self::Missions,
            (Self::Missions, false) | (Self::Spaces, true) => Self::NeedsYou,
            _ => Self::Spaces,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SpaceTarget {
    endpoint_id: ClientEndpointId,
    boot_id: String,
    generation: Option<u64>,
    focus: ClientEndpointFocusTarget,
    applicability: TargetApplicability,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TargetApplicability {
    Spaces,
    Mission(crate::organization::MissionId),
    NeedsYou,
}

impl SpaceTarget {
    pub(super) fn public_id(&self) -> &str {
        match &self.focus {
            ClientEndpointFocusTarget::Workspace(id)
            | ClientEndpointFocusTarget::Tab(id)
            | ClientEndpointFocusTarget::Pane(id) => id,
            #[cfg(windows)]
            ClientEndpointFocusTarget::Notification { pane_id, .. } => pane_id,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct SpaceRow {
    pub(super) label: String,
    pub(super) detail: String,
    pub(super) depth: u16,
    search: String,
    pub(super) selection: Option<SpaceSelection>,
    section: Option<crate::organization::CollectionId>,
    pub(super) parked: bool,
    family_key: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum SpaceSelection {
    Target(SpaceTarget),
    Collection(crate::organization::CollectionId),
    Hibernate,
    Family(String),
    Mission(crate::organization::MissionId),
}

#[derive(Debug, PartialEq, Eq)]
struct ProjectionStamp {
    boot_id: String,
    generation: Option<u64>,
    revision: u64,
    organization_revision: Option<u64>,
}

/// Client presentation only. Shared membership remains in OrganizationState.
#[derive(Debug)]
pub(super) struct MissionControl {
    pub(super) view: MissionControlView,
    pub(super) scope: String,
    scope_source: Option<(String, String)>,
    scope_width: Option<u16>,
    pub(super) query: TextEditor,
    endpoint_id: ClientEndpointId,
    stamp: Option<ProjectionStamp>,
    all_rows: Vec<SpaceRow>,
    pub(super) rows: Vec<SpaceRow>,
    pub(super) selected: Option<SpaceSelection>,
    selection_stale: bool,
    collapsed: HashSet<crate::organization::CollectionId>,
    hibernate_expanded: bool,
    collapsed_families: HashSet<String>,
    pub(super) scroll: usize,
    pub(super) error: Option<String>,
    pub(super) mission_worktree_available: bool,
    pub(super) geometry: SpaceGeometry,
    reveal_selected: bool,
    scrollbar_grab: Option<u16>,
    input_projection_dirty: bool,
}

impl MissionControl {
    fn fit_scope(&mut self, width: u16) {
        if self.scope_width == Some(width) {
            return;
        }
        self.scope_width = Some(width);
        let Some((label, session)) = self.scope_source.as_ref() else {
            return;
        };
        let separator = if width >= 20 { " · session " } else { " · " };
        let available = usize::from(width.saturating_sub(display_width(separator)));
        if available < 2 {
            self.scope = crate::ui::truncate_end(session, usize::from(width));
            return;
        }
        // Keep a recognizable endpoint prefix while giving the selected session
        // priority. Elision affects presentation only, never action identity.
        let minimum_label = usize::from(width / 3).min(8).min(available - 1);
        let session_width = usize::from(display_width(session)).min(available - minimum_label);
        let label_width = available - session_width;
        self.scope = format!(
            "{}{separator}{}",
            crate::ui::truncate_end(label, label_width),
            crate::ui::truncate_end(session, session_width)
        );
    }

    fn filter(&mut self, query_changed: bool) {
        let query = self.query.as_str().to_lowercase();
        let terms = query.split_whitespace().collect::<Vec<_>>();
        let matches_query = |row: &SpaceRow| terms.iter().all(|term| row.search.contains(term));
        if self.view != MissionControlView::Spaces {
            let matching_missions = self
                .all_rows
                .iter()
                .filter(|row| matches_query(row))
                .filter_map(|row| match &row.selection {
                    Some(SpaceSelection::Target(SpaceTarget {
                        applicability: TargetApplicability::Mission(id),
                        ..
                    })) => Some(id.clone()),
                    Some(SpaceSelection::Mission(id)) => Some(id.clone()),
                    _ => None,
                })
                .collect::<HashSet<_>>();
            self.rows = self
                .all_rows
                .iter()
                .filter(|row| {
                    terms.is_empty()
                        || match &row.selection {
                            Some(SpaceSelection::Mission(id)) => matching_missions.contains(id),
                            _ => matches_query(row),
                        }
                })
                .cloned()
                .collect();
            if !self.selection_stale
                && (self.selected.is_none()
                    || (query_changed
                        && (!matches!(self.selected, Some(SpaceSelection::Target(_)))
                            || !self
                                .rows
                                .iter()
                                .any(|row| row.selection.as_ref() == self.selected.as_ref()))))
            {
                self.selected = self
                    .rows
                    .iter()
                    .find(|row| matches!(row.selection, Some(SpaceSelection::Target(_))))
                    .or_else(|| self.rows.iter().find(|row| row.selection.is_some()))
                    .and_then(|row| row.selection.clone());
            }
            if query_changed {
                self.scroll = 0;
                self.reveal_selected = true;
                self.input_projection_dirty = true;
            }
            return;
        }
        let sections = self
            .all_rows
            .iter()
            .filter(|row| {
                matches_query(row) && matches!(row.selection, Some(SpaceSelection::Target(_)))
            })
            .map(|row| row.section.clone())
            .collect::<HashSet<_>>();
        let families = self
            .all_rows
            .iter()
            .filter(|row| {
                matches_query(row) && matches!(row.selection, Some(SpaceSelection::Target(_)))
            })
            .filter_map(|row| row.family_key.as_ref())
            .collect::<HashSet<_>>();
        let parked_match = self.all_rows.iter().any(|row| {
            row.parked
                && matches_query(row)
                && matches!(row.selection, Some(SpaceSelection::Target(_)))
        });
        self.rows = self
            .all_rows
            .iter()
            .filter(|row| {
                if terms.is_empty() {
                    if row.parked && !self.hibernate_expanded {
                        return false;
                    }
                    if row
                        .section
                        .as_ref()
                        .is_some_and(|id| self.collapsed.contains(id))
                        && !matches!(row.selection, Some(SpaceSelection::Collection(_)))
                    {
                        return false;
                    }
                    return !matches!(row.selection, Some(SpaceSelection::Target(_)))
                        || !row
                            .family_key
                            .as_ref()
                            .is_some_and(|key| self.collapsed_families.contains(key));
                }
                match &row.selection {
                    Some(SpaceSelection::Target(_)) => matches_query(row),
                    Some(SpaceSelection::Hibernate) => parked_match,
                    Some(SpaceSelection::Family(key)) => families.contains(key),
                    _ => sections.contains(&row.section),
                }
            })
            .cloned()
            .collect();
        let search_started_from_heading = query_changed
            && !terms.is_empty()
            && !matches!(self.selected, Some(SpaceSelection::Target(_)));
        if self.selected.is_none()
            || search_started_from_heading
            || (query_changed
                // A disappeared capture stays a tombstone until a deliberate
                // row selection; typing must not adopt a same-name replacement.
                && self.all_rows.iter().any(|row| row.selection.as_ref() == self.selected.as_ref())
                && !self
                    .rows
                    .iter()
                    .any(|row| row.selection.as_ref() == self.selected.as_ref()))
        {
            self.selected = self
                .rows
                .iter()
                .find(|row| matches!(row.selection, Some(SpaceSelection::Target(_))))
                .or_else(|| self.rows.iter().find(|row| row.selection.is_some()))
                .and_then(|row| row.selection.clone());
        }
        if query_changed {
            self.scroll = 0;
            self.reveal_selected = true;
            self.input_projection_dirty = true;
        }
    }
    pub(super) fn row_marker(&self, row: &SpaceRow) -> &'static str {
        match &row.selection {
            Some(SpaceSelection::Hibernate) => {
                if self.hibernate_expanded {
                    "▾ "
                } else {
                    "▸ "
                }
            }
            Some(SpaceSelection::Collection(id)) => {
                if self.collapsed.contains(id) {
                    "▸ "
                } else {
                    "▾ "
                }
            }
            Some(SpaceSelection::Family(key)) => {
                if self.collapsed_families.contains(key) {
                    "▸ "
                } else {
                    "▾ "
                }
            }
            _ => "",
        }
    }
    pub(super) fn hit_rows(&self) -> Vec<(Rect, SpaceSelection)> {
        let body = self.geometry.body;
        self.rows
            .iter()
            .enumerate()
            .skip(self.scroll)
            .take(usize::from(body.height))
            .filter_map(|(index, row)| {
                row.selection.clone().map(|selection| {
                    (
                        Rect::new(body.x, body.y + (index - self.scroll) as u16, body.width, 1),
                        selection,
                    )
                })
            })
            .collect()
    }
    pub(super) fn scroll_metrics(&self) -> crate::pane::ScrollMetrics {
        let viewport = usize::from(self.geometry.body.height);
        let max_scroll = self.rows.len().saturating_sub(viewport);
        crate::pane::ScrollMetrics {
            offset_from_bottom: max_scroll.saturating_sub(self.scroll),
            max_offset_from_bottom: max_scroll,
            viewport_rows: viewport,
        }
    }
    pub(super) fn selected_index(&self) -> Option<usize> {
        self.rows.iter().position(|row| {
            row.selection
                .as_ref()
                .is_some_and(|selection| self.selection_matches(selection))
        })
    }

    /// Keep same-boot logical selection visible without adopting a fresh action lease.
    pub(super) fn selection_matches(&self, selection: &SpaceSelection) -> bool {
        match (self.selected.as_ref(), selection) {
            (Some(SpaceSelection::Target(previous)), SpaceSelection::Target(current)) => {
                previous.endpoint_id == current.endpoint_id
                    && previous.boot_id == current.boot_id
                    && previous.focus == current.focus
                    && previous.applicability == current.applicability
            }
            (Some(previous), current) => previous == current,
            _ => false,
        }
    }
}

impl ClientShellState {
    pub(super) fn open_mission_control(&mut self) {
        // Finalize client selection without copying or changing scrollback. The
        // terminal's already forwarded mouse press still owns its owed release.
        if let Some(autoscroll) = self.selection_autoscroll.as_ref() {
            self.pane_scroll_queued.remove(&autoscroll.pane_id);
        }
        self.stop_selection_autoscroll();
        self.word_selection_gesture = None;
        if let Some(selection) = self.selection.as_mut() {
            selection.finish();
        }
        self.chrome_drag = None;
        self.workspace_press = None;
        self.tab_press = None;
        self.overlay = Some(ClientShellOverlay::MissionControl(Box::new(
            MissionControl {
                query: TextEditor::default(),
                view: MissionControlView::Spaces,
                scope: String::new(),
                scope_source: None,
                scope_width: None,
                endpoint_id: self.active_endpoint_id.clone(),
                stamp: None,
                all_rows: Vec::new(),
                rows: Vec::new(),
                selected: None,
                selection_stale: false,
                scroll: 0,
                collapsed: HashSet::new(),
                hibernate_expanded: false,
                collapsed_families: HashSet::new(),
                error: None,
                mission_worktree_available: false,
                geometry: SpaceGeometry::default(),
                reveal_selected: true,
                scrollbar_grab: None,
                input_projection_dirty: false,
            },
        )));
        self.refresh_mission_control();
    }

    /// Cached once per snapshot revision while open, before drawing or input dispatch.
    pub(super) fn refresh_mission_control(&mut self) {
        let Some(ClientShellOverlay::MissionControl(control)) = self.overlay.as_mut() else {
            return;
        };
        let Some(endpoint) = self
            .endpoints
            .iter()
            .find(|e| e.endpoint_id == control.endpoint_id)
        else {
            return;
        };
        if control
            .scope_source
            .as_ref()
            .is_none_or(|(label, session)| {
                label != &endpoint.label || session != &endpoint.session_name
            })
        {
            control.scope_source = Some((endpoint.label.clone(), endpoint.session_name.clone()));
            control.scope_width = None;
        }
        control.mission_worktree_available = endpoint.status == ClientEndpointStatus::Online
            && endpoint.organization_supported
            && endpoint.organization.is_some()
            && endpoint
                .methods
                .as_ref()
                .is_some_and(|m| m.contains("worktree.create_in_mission"));
        let Some(snapshot) = endpoint.snapshot.as_deref() else {
            return;
        };
        let catalog = endpoint
            .organization
            .as_ref()
            .filter(|_| endpoint.organization_supported)
            .map(|catalog| &catalog.organization);
        let stamp = ProjectionStamp {
            boot_id: snapshot.boot_id.clone(),
            generation: endpoint.snapshot_generation,
            revision: snapshot.revision,
            organization_revision: catalog.map(|c| c.revision),
        };
        if control.stamp.as_ref() == Some(&stamp) {
            return;
        }
        if let Some(SpaceSelection::Mission(id)) = &control.selected {
            control.selection_stale |= control.stamp.as_ref().is_some_and(|previous| {
                previous.boot_id != stamp.boot_id || previous.generation != stamp.generation
            }) || !catalog
                .is_some_and(|c| c.missions.iter().any(|mission| &mission.id == id));
        }
        if let Some(SpaceSelection::Target(target)) = control.selected.as_ref() {
            control.selection_stale |= !members::target_applicable(endpoint, target);
        }
        let rows = match control.view {
            MissionControlView::Spaces => project_spaces(endpoint),
            MissionControlView::Missions => members::project_missions(endpoint),
            MissionControlView::NeedsYou => members::project_needs_you(endpoint),
        };
        control.all_rows = rows;
        control.stamp = Some(stamp);
        control.filter(false);
    }

    pub(super) fn insert_mission_control_text(&mut self, text: &str) -> bool {
        self.refresh_mission_control();
        let Some(ClientShellOverlay::MissionControl(control)) = self.overlay.as_mut() else {
            return false;
        };
        if control.query.insert(text) {
            control.filter(true);
        }
        self.sync_mission_control_input_projection();
        true
    }

    pub(super) fn route_mission_control_key(
        &mut self,
        key: &crate::input::TerminalKey,
        outcome: &mut ClientShellInput,
    ) -> bool {
        if !matches!(self.overlay, Some(ClientShellOverlay::MissionControl(_))) {
            return false;
        }
        self.refresh_mission_control();
        if let Some((cols, rows)) = self.last_composed_size {
            self.prepare_mission_control_geometry(cols, rows);
        }
        if key.code == KeyCode::Char('n')
            && key.modifiers == crossterm::event::KeyModifiers::CONTROL
        {
            self.create_mission_control_worktree(outcome);
        } else if key.code == KeyCode::Esc {
            self.overlay = None;
        } else if matches!(key.code, KeyCode::Tab | KeyCode::BackTab) {
            if let Some(ClientShellOverlay::MissionControl(control)) = self.overlay.as_ref() {
                self.switch_mission_control_view(control.view.next(key.code == KeyCode::BackTab));
            }
        } else if key.code == KeyCode::Enter {
            self.accept_space_target(outcome);
        } else if let Some(ClientShellOverlay::MissionControl(control)) = self.overlay.as_mut() {
            match key.code {
                KeyCode::Up | KeyCode::Down | KeyCode::PageUp | KeyCode::PageDown => {
                    let choices = control
                        .rows
                        .iter()
                        .filter_map(|row| row.selection.as_ref())
                        .collect::<Vec<_>>();
                    let current = choices
                        .iter()
                        .position(|choice| control.selection_matches(choice));
                    let delta = match key.code {
                        KeyCode::Up => -1,
                        KeyCode::Down => 1,
                        KeyCode::PageUp => {
                            -isize::try_from(control.geometry.body.height.max(1)).unwrap_or(1)
                        }
                        _ => isize::try_from(control.geometry.body.height.max(1)).unwrap_or(1),
                    };
                    let first_target = choices
                        .iter()
                        .position(|choice| matches!(choice, SpaceSelection::Target(_)))
                        .unwrap_or(0);
                    let next = current
                        .map_or(first_target, |i| i.saturating_add_signed(delta))
                        .min(choices.len().saturating_sub(1));
                    if let Some(choice) = choices.get(next) {
                        control.selected = Some((*choice).clone());
                        control.selection_stale = false;
                    }
                    control.reveal_selected = true;
                    control.input_projection_dirty = true;
                }
                _ => {
                    if control.query.handle_key(key) == Some(true) {
                        control.filter(true);
                    }
                }
            }
        }
        self.sync_mission_control_input_projection();
        outcome.repaint = true;
        true
    }

    /// Explicit query/scroll edits take effect within one input batch. Metadata
    /// updates keep last-drawn captures until composition so closing a row cannot
    /// silently shift a click onto its neighbor.
    fn sync_mission_control_input_projection(&mut self) {
        if !matches!(&self.overlay,Some(ClientShellOverlay::MissionControl(control)) if control.input_projection_dirty)
        {
            return;
        }
        let Some((cols, rows)) = self.last_composed_size else {
            return;
        };
        self.prepare_mission_control_geometry(cols, rows);
        if let Some(ClientShellOverlay::MissionControl(control)) = self.overlay.as_mut() {
            self.hits.mission_control_rows = control.hit_rows();
            self.hits.mission_control_scrollbar = control.geometry.scrollbar;
            self.hits.mission_control_scroll_metrics = Some(control.scroll_metrics());
            self.hits.overlay_cancel = control.geometry.close;
            self.hits.overlay_clear = if control.view == MissionControlView::Missions {
                control.geometry.mission_worktree_action()
            } else {
                Rect::default()
            };
            control.input_projection_dirty = false;
        }
    }

    pub(super) fn prepare_mission_control_geometry(&mut self, cols: u16, rows: u16) {
        let Some(ClientShellOverlay::MissionControl(control)) = self.overlay.as_mut() else {
            return;
        };
        let geometry = SpaceGeometry::new(cols, rows, control.rows.len());
        control.fit_scope(geometry.header.width);
        let viewport = usize::from(geometry.body.height).max(1);
        control.scroll = control
            .scroll
            .min(control.rows.len().saturating_sub(viewport));
        if control.reveal_selected || control.geometry.body.height != geometry.body.height {
            if let Some(selected) = control.selected_index() {
                if selected < control.scroll {
                    control.scroll = selected;
                }
                if selected >= control.scroll + viewport {
                    control.scroll = selected + 1 - viewport;
                }
            }
        }
        control.reveal_selected = false;
        control.geometry = geometry;
    }

    pub(super) fn handle_mission_control_mouse(
        &mut self,
        mouse: MouseEvent,
        outcome: &mut ClientShellInput,
    ) {
        let point = (mouse.column, mouse.row);
        if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
            let selected_view = self.overlay.as_ref().and_then(|overlay| match overlay {
                ClientShellOverlay::MissionControl(control) => control
                    .geometry
                    .view_tabs()
                    .into_iter()
                    .find(|(rect, _)| contains(*rect, point))
                    .map(|(_, view)| view),
                _ => None,
            });
            if let Some(view) = selected_view {
                self.switch_mission_control_view(view);
                outcome.repaint = true;
                return;
            }
        }
        if mouse.kind == MouseEventKind::Down(MouseButton::Left)
            && contains(self.hits.overlay_clear, point)
        {
            self.create_mission_control_worktree(outcome);
            return;
        }
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Right) => {
                if let Some((_, selection)) = self
                    .hits
                    .mission_control_rows
                    .iter()
                    .find(|(rect, _)| contains(*rect, point))
                    .cloned()
                {
                    if let Some(ClientShellOverlay::MissionControl(control)) = self.overlay.as_mut()
                    {
                        control.selected = Some(selection);
                        control.selection_stale = false;
                    }
                    if matches!(&self.overlay, Some(ClientShellOverlay::MissionControl(control)) if matches!(control.selected, Some(SpaceSelection::Mission(_))))
                    {
                        self.create_mission_control_worktree(outcome);
                    } else {
                        self.assign_mission_control_target(outcome);
                    }
                    outcome.repaint = true;
                }
            }
            MouseEventKind::Down(MouseButton::Left)
                if contains(self.hits.overlay_cancel, point) =>
            {
                self.overlay = None;
                outcome.repaint = true;
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some((_, selection)) = self
                    .hits
                    .mission_control_rows
                    .iter()
                    .find(|(rect, _)| contains(*rect, point))
                    .cloned()
                {
                    if let Some(ClientShellOverlay::MissionControl(control)) = self.overlay.as_mut()
                    {
                        control.selected = Some(selection);
                        control.selection_stale = false;
                    }
                    self.accept_space_target(outcome);
                    outcome.repaint = true;
                } else if contains(self.hits.mission_control_scrollbar, point) {
                    if let Some(metrics) = self.hits.mission_control_scroll_metrics {
                        if let Some(ClientShellOverlay::MissionControl(control)) =
                            self.overlay.as_mut()
                        {
                            control.scrollbar_grab = crate::ui::scrollbar_thumb_grab_offset(
                                metrics,
                                self.hits.mission_control_scrollbar,
                                mouse.row,
                            );
                            if control.scrollbar_grab.is_none() {
                                let offset = crate::ui::scrollbar_offset_from_row(
                                    metrics,
                                    self.hits.mission_control_scrollbar,
                                    mouse.row,
                                );
                                control.scroll =
                                    metrics.max_offset_from_bottom.saturating_sub(offset);
                                control.input_projection_dirty = true;
                            }
                        }
                        outcome.repaint = true;
                    }
                }
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                if let Some(ClientShellOverlay::MissionControl(control)) = self.overlay.as_mut() {
                    let delta = if mouse.kind == MouseEventKind::ScrollUp {
                        -3
                    } else {
                        3
                    };
                    let viewport = usize::from(control.geometry.body.height).max(1);
                    control.scroll = control
                        .scroll
                        .saturating_add_signed(delta)
                        .min(control.rows.len().saturating_sub(viewport));
                    control.reveal_selected = false;
                    control.input_projection_dirty = true;
                }
                outcome.repaint = true;
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let (Some(ClientShellOverlay::MissionControl(control)), Some(metrics)) = (
                    self.overlay.as_mut(),
                    self.hits.mission_control_scroll_metrics,
                ) {
                    if let Some(grab) = control.scrollbar_grab {
                        let offset = crate::ui::scrollbar_offset_from_drag_row(
                            metrics,
                            self.hits.mission_control_scrollbar,
                            mouse.row,
                            grab,
                        );
                        control.scroll = metrics.max_offset_from_bottom.saturating_sub(offset);
                        control.input_projection_dirty = true;
                        outcome.repaint = true;
                    }
                }
            }
            MouseEventKind::Up(_) => {
                if let Some(ClientShellOverlay::MissionControl(control)) = self.overlay.as_mut() {
                    control.scrollbar_grab = None;
                }
            }
            _ => {}
        }
        self.sync_mission_control_input_projection();
    }

    pub(super) fn space_target_notice(&mut self, message: &str, outcome: &mut ClientShellInput) {
        if let Some(ClientShellOverlay::MissionControl(control)) = self.overlay.as_mut() {
            control.error = Some(message.to_owned());
        }
        outcome.repaint |= self.push_endpoint_notice(
            ClientEndpointNoticeKind::Unavailable,
            "organization.mission_control.target",
            "Target unavailable",
            message,
        );
    }

    fn switch_mission_control_view(&mut self, view: MissionControlView) {
        let previous = self.overlay.as_ref().and_then(|overlay| match overlay {
            ClientShellOverlay::MissionControl(control) if !control.selection_stale => {
                match &control.selected {
                    Some(SpaceSelection::Target(target)) => Some(target.clone()),
                    _ => None,
                }
            }
            _ => None,
        });
        if let Some(ClientShellOverlay::MissionControl(control)) = self.overlay.as_mut() {
            control.view = view;
            control.stamp = None;
            control.scroll = 0;
            control.reveal_selected = true;
            control.input_projection_dirty = true;
        }
        self.refresh_mission_control();
        if let (Some(previous), Some(ClientShellOverlay::MissionControl(control))) =
            (previous, self.overlay.as_mut())
        {
            if !control.selection_stale {
                if let Some(selection) = control
                    .all_rows
                    .iter()
                    .find_map(|row| match &row.selection {
                        Some(SpaceSelection::Target(current))
                            if current.endpoint_id == previous.endpoint_id
                                && current.boot_id == previous.boot_id
                                && current.generation == previous.generation
                                && current.focus == previous.focus =>
                        {
                            Some(row.selection.clone())
                        }
                        _ => None,
                    })
                    .flatten()
                {
                    control.selected = Some(selection);
                }
            }
        }
        self.sync_mission_control_input_projection();
    }

    fn create_mission_control_worktree(&mut self, outcome: &mut ClientShellInput) {
        self.refresh_mission_control();
        let Some(ClientShellOverlay::MissionControl(control)) = self.overlay.as_ref() else {
            return;
        };
        if control.view != MissionControlView::Missions || control.selection_stale {
            self.space_target_notice(
                "Select a current mission or member to create a worktree.",
                outcome,
            );
            return;
        }
        let (mission, member_workspace) = match &control.selected {
            Some(SpaceSelection::Mission(id)) => (id.clone(), None),
            Some(SpaceSelection::Target(target)) => {
                let TargetApplicability::Mission(id) = &target.applicability else {
                    return;
                };
                let Some(endpoint) = self
                    .endpoints
                    .iter()
                    .find(|e| e.endpoint_id == target.endpoint_id)
                else {
                    return;
                };
                if !members::target_applicable(endpoint, target) {
                    self.space_target_notice(
                        "The selected mission member is no longer available.",
                        outcome,
                    );
                    return;
                }
                let workspace = endpoint
                    .snapshot
                    .as_deref()
                    .and_then(|s| match &target.focus {
                        ClientEndpointFocusTarget::Workspace(id) => Some(id.clone()),
                        ClientEndpointFocusTarget::Tab(id) => s
                            .tabs
                            .iter()
                            .find(|t| &t.tab_id == id)
                            .map(|t| t.workspace_id.clone()),
                        ClientEndpointFocusTarget::Pane(id) => s
                            .panes
                            .iter()
                            .find(|p| &p.pane_id == id)
                            .map(|p| p.workspace_id.clone()),
                        #[cfg(windows)]
                        ClientEndpointFocusTarget::Notification { .. } => None,
                    });
                (id.clone(), workspace)
            }
            _ => {
                self.space_target_notice("Select a mission to create a worktree.", outcome);
                return;
            }
        };
        self.begin_mission_worktree(mission, member_workspace, outcome);
    }

    fn assign_mission_control_target(&mut self, outcome: &mut ClientShellInput) {
        self.refresh_mission_control();
        let Some(ClientShellOverlay::MissionControl(control)) = self.overlay.as_ref() else {
            return;
        };
        let Some(SpaceSelection::Target(target)) = control.selected.clone() else {
            return;
        };
        let endpoint = self
            .endpoints
            .iter()
            .find(|e| e.endpoint_id == target.endpoint_id);
        let valid = !control.selection_stale
            && target.endpoint_id == self.active_endpoint_id
            && endpoint.is_some_and(|e| {
                e.status == ClientEndpointStatus::Online && members::target_applicable(e, &target)
            })
            && control
                .all_rows
                .iter()
                .any(|row| row.selection.as_ref() == Some(&SpaceSelection::Target(target.clone())));
        if !valid {
            self.space_target_notice("The selected member or connection is no longer available. Select a current member to assign.", outcome);
            return;
        }
        match target.focus {
            ClientEndpointFocusTarget::Tab(tab_id) => {
                let workspace_id = endpoint
                    .and_then(|e| e.snapshot.as_ref())
                    .and_then(|s| s.tabs.iter().find(|t| t.tab_id == tab_id))
                    .map(|t| t.workspace_id.clone());
                if let Some(workspace) =
                    workspace_id.and_then(|id| self.navigation_target(&target.endpoint_id, &id))
                {
                    self.open_mission_picker(tab_id, workspace, outcome);
                }
            }
            ClientEndpointFocusTarget::Pane(pane_id) => {
                if let Some(context) = self.pane_mission_context(&pane_id) {
                    self.open_pane_mission_picker(context, outcome);
                }
            }
            _ => {}
        }
    }

    fn accept_space_target(&mut self, outcome: &mut ClientShellInput) {
        let Some(ClientShellOverlay::MissionControl(control)) = self.overlay.as_ref() else {
            return;
        };
        let Some(selection) = control.selected.clone() else {
            return;
        };
        match selection {
            SpaceSelection::Target(target) => {
                let applicable = !control.selection_stale
                    && control.all_rows.iter().any(|row| {
                        row.selection.as_ref() == Some(&SpaceSelection::Target(target.clone()))
                    });
                let current = self
                    .endpoints
                    .iter()
                    .find(|e| e.endpoint_id == target.endpoint_id);
                let valid = current.is_some_and(|endpoint| {
                    applicable
                        && members::target_applicable(endpoint, &target)
                        && endpoint.status == ClientEndpointStatus::Online
                        && endpoint.snapshot_generation == target.generation
                        && endpoint.snapshot.as_deref().is_some_and(|snapshot| {
                            snapshot.boot_id == target.boot_id
                                && match &target.focus {
                                    ClientEndpointFocusTarget::Workspace(id) => {
                                        snapshot.workspaces.iter().any(|w| &w.workspace_id == id)
                                    }
                                    ClientEndpointFocusTarget::Tab(id) => {
                                        snapshot.tabs.iter().any(|t| &t.tab_id == id)
                                    }
                                    ClientEndpointFocusTarget::Pane(id) => {
                                        snapshot.panes.iter().any(|p| &p.pane_id == id)
                                    }
                                    #[cfg(windows)]
                                    ClientEndpointFocusTarget::Notification { .. } => false,
                                }
                        })
                });
                let method = match &target.focus {
                    ClientEndpointFocusTarget::Workspace(_) => "workspace.focus",
                    ClientEndpointFocusTarget::Tab(_) => "tab.focus",
                    ClientEndpointFocusTarget::Pane(_) => "pane.focus",
                    #[cfg(windows)]
                    ClientEndpointFocusTarget::Notification { .. } => "pane.focus",
                };
                let supported =
                    current.is_some_and(|e| e.methods.as_ref().is_none_or(|m| m.contains(method)));
                if !valid || !supported {
                    self.space_target_notice("The selected target is closed, disconnected, or unavailable. Select a current target to try again.", outcome);
                    return;
                }
                let action_count = outcome.actions.len();
                if self.focus_or_activate(target.endpoint_id, target.focus, outcome)
                    && outcome.actions.len() > action_count
                {
                    if let Some(ClientShellAction::Endpoint { request, .. }) =
                        outcome.actions.last()
                    {
                        if let Some(pending) = self.pending_requests.get_mut(&request.id) {
                            pending.kind = PendingEndpointKind::MissionControlFocus;
                        }
                    }
                    self.overlay = None;
                }
            }
            _ => {
                if let Some(ClientShellOverlay::MissionControl(control)) = self.overlay.as_mut() {
                    match selection {
                        SpaceSelection::Collection(id) => {
                            if !control.collapsed.remove(&id) {
                                control.collapsed.insert(id);
                            }
                        }
                        SpaceSelection::Hibernate => {
                            control.hibernate_expanded = !control.hibernate_expanded
                        }
                        SpaceSelection::Family(key) => {
                            if !control.collapsed_families.remove(&key) {
                                control.collapsed_families.insert(key);
                            }
                        }
                        SpaceSelection::Target(_) => {}
                        SpaceSelection::Mission(_) => {}
                    }
                    control.filter(false);
                    control.input_projection_dirty = true;
                }
            }
        }
    }
}

fn project_spaces(endpoint: &ClientShellEndpoint) -> Vec<SpaceRow> {
    let Some(snapshot) = endpoint.snapshot.as_deref() else {
        return Vec::new();
    };
    let catalog = endpoint
        .organization
        .as_ref()
        .filter(|_| endpoint.organization_supported)
        .map(|c| &c.organization);
    let target = |focus| {
        Some(SpaceSelection::Target(SpaceTarget {
            endpoint_id: endpoint.endpoint_id.clone(),
            boot_id: snapshot.boot_id.clone(),
            generation: endpoint.snapshot_generation,
            focus,
            applicability: TargetApplicability::Spaces,
        }))
    };
    let mut tabs = HashMap::<&str, Vec<&crate::protocol::ClientShellTab>>::new();
    let mut panes = HashMap::<&str, Vec<&crate::protocol::ClientShellPane>>::new();
    let agents = snapshot
        .agents
        .iter()
        .map(|a| (a.pane_id.as_str(), a))
        .collect::<HashMap<_, _>>();
    for tab in &snapshot.tabs {
        tabs.entry(&tab.workspace_id).or_default().push(tab);
    }
    for pane in &snapshot.panes {
        panes.entry(&pane.tab_id).or_default().push(pane);
    }
    let mut sections = HashMap::<Option<crate::organization::CollectionId>, Vec<SpaceRow>>::new();
    let mut emitted_families = HashSet::new();
    for entry in sidebar::workspace_entries(snapshot, &HashSet::new()) {
        let workspace = &snapshot.workspaces[entry.index];
        let family = match &workspace.worktree {
            Some(worktree) => crate::organization::FamilyId::Managed {
                key: worktree.key.clone(),
            },
            None => crate::organization::FamilyId::Standalone {
                workspace_id: workspace.workspace_id.clone(),
            },
        };
        let collection = catalog.and_then(|c| {
            c.collection_for(&family)
                .and_then(|id| c.collections.iter().find(|v| &v.id == id))
        });
        let section = collection.map(|c| c.id.clone());
        let parked = collection.is_some_and(|c| c.hibernating);
        let context = format!(
            "{} {} {} {} {} {}",
            workspace.label,
            workspace.branch.as_deref().unwrap_or_default(),
            workspace.new_workspace_cwd,
            workspace.worktree.as_ref().map_or("", |w| w.label.as_str()),
            collection.map_or("", |c| c.name.as_str()),
            workspace
                .tokens
                .iter()
                .map(|(_, v)| v.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        );
        let family_key = workspace.worktree.as_ref().map(|w| w.key.clone());
        let depth = 1 + u16::from(family_key.is_some()) + u16::from(entry.indented);
        let rows = sections.entry(section.clone()).or_default();
        if let Some(worktree) = workspace
            .worktree
            .as_ref()
            .filter(|w| emitted_families.insert(w.key.clone()))
        {
            rows.push(SpaceRow {
                label: worktree.label.clone(),
                detail: String::new(),
                depth: 1,
                search: context.to_lowercase(),
                selection: Some(SpaceSelection::Family(worktree.key.clone())),
                section: section.clone(),
                parked,
                family_key: family_key.clone(),
            });
        }
        rows.push(SpaceRow {
            label: workspace.label.clone(),
            detail: format!(
                "{} · {} · {}",
                workspace.workspace_id,
                workspace.branch.as_deref().unwrap_or_default(),
                workspace.new_workspace_cwd
            ),
            depth,
            search: context.to_lowercase(),
            selection: target(ClientEndpointFocusTarget::Workspace(
                workspace.workspace_id.clone(),
            )),
            section: section.clone(),
            parked,
            family_key: family_key.clone(),
        });
        for tab in tabs
            .get(workspace.workspace_id.as_str())
            .into_iter()
            .flatten()
        {
            let mission = catalog
                .and_then(|c| {
                    c.mission_for(&crate::organization::MissionTarget::Tab {
                        tab_id: tab.tab_id.clone(),
                    })
                    .and_then(|id| c.missions.iter().find(|m| &m.id == id))
                })
                .map_or("", |m| m.name.as_str());
            let tab_context = format!("{context} {} {mission}", tab.label);
            rows.push(SpaceRow {
                label: tab.label.clone(),
                detail: format!(
                    "{} · {} · {} {mission}",
                    tab.tab_id, workspace.label, workspace.new_workspace_cwd
                ),
                depth: depth + 1,
                search: tab_context.to_lowercase(),
                selection: target(ClientEndpointFocusTarget::Tab(tab.tab_id.clone())),
                section: section.clone(),
                parked,
                family_key: family_key.clone(),
            });
            for pane in panes.get(tab.tab_id.as_str()).into_iter().flatten() {
                let agent = agents.get(pane.pane_id.as_str()).copied();
                let label = agent
                    .and_then(|a| a.name.as_deref())
                    .or(pane.label.as_deref())
                    .or_else(|| {
                        agent.and_then(|a| a.display_agent.as_deref().or(a.agent.as_deref()))
                    })
                    .unwrap_or("Terminal");
                let location = pane
                    .foreground_cwd
                    .as_deref()
                    .or(pane.cwd.as_deref())
                    .unwrap_or(&workspace.new_workspace_cwd);
                let membership = catalog
                    .and_then(|c| {
                        c.effective_pane_mission(&pane.pane_id, &pane.tab_id)
                            .and_then(|id| c.missions.iter().find(|m| &m.id == id))
                    })
                    .map_or("", |m| m.name.as_str());
                // Search every available label independently of display priority.
                let labels = agent
                    .map(|a| {
                        a.tokens
                            .iter()
                            .chain(&a.state_labels)
                            .map(|(_, v)| v.as_str())
                            .chain(
                                [
                                    a.name.as_deref(),
                                    a.display_agent.as_deref(),
                                    a.agent.as_deref(),
                                    a.title.as_deref(),
                                ]
                                .into_iter()
                                .flatten(),
                            )
                            .collect::<Vec<_>>()
                            .join(" ")
                    })
                    .unwrap_or_default();
                let pane_label = pane.label.as_deref().unwrap_or_default();
                rows.push(SpaceRow {
                    label: label.to_owned(),
                    detail: format!(
                        "{} · {} / {} · {location} {membership}",
                        pane.pane_id, workspace.label, tab.label
                    ),
                    depth: depth + 2,
                    search: format!(
                        "{tab_context} {label} {} {location} {membership} {labels} {pane_label}",
                        pane.cwd.as_deref().unwrap_or_default()
                    )
                    .to_lowercase(),
                    selection: target(ClientEndpointFocusTarget::Pane(pane.pane_id.clone())),
                    section: section.clone(),
                    parked,
                    family_key: family_key.clone(),
                });
            }
        }
    }
    let heading = |label: String, selection, section, parked| SpaceRow {
        label,
        detail: String::new(),
        depth: 0,
        search: String::new(),
        selection,
        section,
        parked,
        family_key: None,
    };
    let mut collections = catalog
        .map(|c| c.collections.iter().collect::<Vec<_>>())
        .unwrap_or_default();
    collections.sort_by_key(|c| c.order);
    let mut rows = Vec::new();
    for c in collections.iter().filter(|c| !c.hibernating) {
        rows.push(heading(
            c.name.as_str().to_owned(),
            Some(SpaceSelection::Collection(c.id.clone())),
            Some(c.id.clone()),
            false,
        ));
        let members = sections.remove(&Some(c.id.clone())).unwrap_or_default();
        if members.is_empty() {
            rows.push(SpaceRow {
                label: "Empty collection".into(),
                detail: "Assign a family from its workspace menu".into(),
                depth: 1,
                search: c.name.as_str().to_lowercase(),
                selection: None,
                section: Some(c.id.clone()),
                parked: c.hibernating,
                family_key: None,
            });
        } else {
            rows.extend(members);
        }
    }
    rows.push(heading("Uncollected".into(), None, None, false));
    rows.extend(sections.remove(&None).unwrap_or_default());
    rows.push(heading(
        "Hibernate".into(),
        Some(SpaceSelection::Hibernate),
        None,
        false,
    ));
    for c in collections.iter().filter(|c| c.hibernating) {
        rows.push(heading(
            c.name.as_str().to_owned(),
            Some(SpaceSelection::Collection(c.id.clone())),
            Some(c.id.clone()),
            true,
        ));
        let members = sections.remove(&Some(c.id.clone())).unwrap_or_default();
        if members.is_empty() {
            rows.push(SpaceRow {
                label: "Empty collection".into(),
                detail: "Assign a family from its workspace menu".into(),
                depth: 1,
                search: c.name.as_str().to_lowercase(),
                selection: None,
                section: Some(c.id.clone()),
                parked: c.hibernating,
                family_key: None,
            });
        } else {
            rows.extend(members);
        }
    }
    rows
}

/// One row geometry shared by selection reveal, drawing, scrollbar and hit testing.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SpaceGeometry {
    pub(super) area: Rect,
    pub(super) header: Rect,
    pub(super) search: Rect,
    pub(super) body: Rect,
    pub(super) footer: Rect,
    pub(super) close: Rect,
    pub(super) scrollbar: Rect,
}

impl SpaceGeometry {
    pub(super) fn mission_worktree_action(self) -> Rect {
        Rect::new(self.footer.x, self.footer.y, 13, 1).intersection(self.footer)
    }
    pub(super) fn view_tabs(self) -> [(Rect, MissionControlView); 3] {
        let y = self.header.y.saturating_add(1);
        let area = Rect::new(self.header.x, y, self.header.width, 1).intersection(self.area);
        [
            (
                Rect::new(area.x, y, 8, 1).intersection(area),
                MissionControlView::Spaces,
            ),
            (
                Rect::new(area.x.saturating_add(9), y, 10, 1).intersection(area),
                MissionControlView::Missions,
            ),
            (
                Rect::new(area.x.saturating_add(20), y, 11, 1).intersection(area),
                MissionControlView::NeedsYou,
            ),
        ]
    }

    fn new(cols: u16, rows: u16, row_count: usize) -> Self {
        let width = if cols < 60 {
            cols
        } else {
            cols.saturating_sub(4).min(116)
        };
        let height = if rows < 12 {
            rows
        } else {
            rows.saturating_sub(2).min(42)
        };
        let area = Rect::new((cols - width) / 2, (rows - height) / 2, width, height);
        let inner = Rect::new(
            area.x.saturating_add(1),
            area.y.saturating_add(1),
            width.saturating_sub(2),
            height.saturating_sub(2),
        );
        let header = Rect::new(inner.x, inner.y, inner.width, u16::from(inner.height > 0));
        let search =
            Rect::new(inner.x, inner.y.saturating_add(2), inner.width, 1).intersection(inner);
        let footer = Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1)
            .intersection(inner);
        let mut body = Rect::new(
            inner.x,
            inner.y.saturating_add(4),
            inner.width,
            inner.height.saturating_sub(5),
        );
        let scrollbar = if row_count > usize::from(body.height) && body.width > 1 && body.height > 0
        {
            body.width = body.width.saturating_sub(1);
            Rect::new(body.right(), body.y, 1, body.height)
        } else {
            Rect::default()
        };
        let close = crate::ui::release_notes_close_button_rect(header).intersection(header);
        Self {
            area,
            header,
            search,
            body,
            footer,
            close,
            scrollbar,
        }
    }
}
