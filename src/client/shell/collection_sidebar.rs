use super::organization::CollectionRow;
use super::render::{put_text, ShellRenderState};
use super::*;

/// Draw a cached organization projection using the existing workspace token rows.
pub(super) fn render_collection_rows(
    buffer: &mut Buffer,
    body: Rect,
    snapshot: &ClientShellSnapshot,
    config: &ClientShellConfig,
    state: &mut ShellRenderState<'_>,
    hits: &mut ShellHitMap,
) -> bool {
    let Some(endpoint) = state
        .endpoints
        .iter()
        .find(|endpoint| &endpoint.endpoint_id == state.active_endpoint_id)
    else {
        return false;
    };
    let Some(catalog) = endpoint.organization.as_ref() else {
        return false;
    };
    if !endpoint.organization_supported {
        return false;
    }
    let entries = &endpoint.organization_rows;
    let palette = &config.palette;
    let heights = entries
        .iter()
        .map(|row| match row {
            CollectionRow::Workspace(entry) => {
                snapshot.workspaces.get(entry.index).map_or(1, |workspace| {
                    sidebar::workspace_rows(
                        workspace,
                        sidebar::displayed_workspace_status(
                            snapshot,
                            workspace,
                            state.collapsed_groups,
                        ),
                        entry.indented,
                        &config.spaces,
                    )
                    .len()
                    .max(1)
                    .min(u16::MAX as usize) as u16
                })
            }
            _ => 1,
        })
        .collect::<Vec<_>>();
    let gaps = vec![0; heights.len()];
    if !body.is_empty() && std::mem::take(state.reveal_focused_workspace) {
        if let Some(target) = entries.iter().position(|row| matches!(row, CollectionRow::Workspace(entry) if snapshot.workspaces[entry.index].focused)) {
            *state.workspace_scroll = super::scroll::list_scroll_start_to_reveal(&heights, &gaps, body.height, *state.workspace_scroll, target);
        }
    }
    let metrics =
        super::scroll::list_scroll_metrics(&heights, &gaps, body.height, *state.workspace_scroll);
    hits.workspace_max_scroll = metrics.max_offset_from_bottom;
    hits.workspace_scroll_metrics = Some(metrics);
    *state.workspace_scroll = metrics
        .max_offset_from_bottom
        .saturating_sub(metrics.offset_from_bottom);
    let scrollbar = metrics.max_offset_from_bottom > 0 && body.width > 1;
    let width = body.width.saturating_sub(u16::from(scrollbar));
    let mut y = body.y;
    for (index, row) in entries.iter().enumerate().skip(*state.workspace_scroll) {
        let height = heights[index].min(body.height);
        if y.saturating_add(height) > body.bottom() {
            break;
        }
        let rect = Rect::new(body.x, y, width, height);
        match row {
            CollectionRow::Hibernate => {
                let marker = if state.expanded_hibernate.contains(state.active_endpoint_id) {
                    "▾"
                } else {
                    "▸"
                };
                put_text(
                    buffer,
                    rect.x,
                    rect.y,
                    rect.width,
                    &format!(" {marker} Hibernate"),
                    Style::default()
                        .fg(palette.overlay0)
                        .add_modifier(Modifier::BOLD),
                );
                hits.hibernate
                    .push((rect, state.active_endpoint_id.clone()));
            }
            CollectionRow::Collection(index) => {
                if let Some(collection) = catalog.organization.collections.get(*index) {
                    let marker = if state
                        .collapsed_collections
                        .get(state.active_endpoint_id)
                        .is_some_and(|ids| ids.contains(&collection.id))
                    {
                        "▸"
                    } else {
                        "▾"
                    };
                    put_text(
                        buffer,
                        rect.x,
                        rect.y,
                        rect.width,
                        &format!(
                            "{}{marker} {}",
                            if collection.hibernating { "   " } else { " " },
                            collection.name.as_str()
                        ),
                        Style::default()
                            .fg(palette.text)
                            .add_modifier(Modifier::BOLD),
                    );
                    hits.collections.push((
                        rect,
                        state.active_endpoint_id.clone(),
                        collection.id.clone(),
                    ));
                }
            }
            CollectionRow::Uncollected => put_text(
                buffer,
                rect.x,
                rect.y,
                rect.width,
                " Uncollected",
                Style::default().fg(palette.overlay0),
            ),
            CollectionRow::Workspace(entry) => {
                let Some(workspace) = snapshot.workspaces.get(entry.index) else {
                    continue;
                };
                let status = sidebar::displayed_workspace_status(
                    snapshot,
                    workspace,
                    state.collapsed_groups,
                );
                let tokens =
                    sidebar::workspace_rows(workspace, status, entry.indented, &config.spaces);
                let nested = Rect::new(
                    rect.x.saturating_add(2),
                    rect.y,
                    rect.width.saturating_sub(2),
                    rect.height,
                );
                let selected = state.selected_workspace_id.is_some_and(|target| {
                    target.matches(state.active_endpoint_id, &workspace.workspace_id)
                });
                sidebar::render_workspace_rows(
                    buffer,
                    nested,
                    status,
                    config.status_indicators,
                    entry,
                    tokens,
                    workspace.focused,
                    selected,
                    state.selected_workspace_id.is_some(),
                    state.dragged_workspace_id == Some(workspace.workspace_id.as_str()),
                    palette,
                );
                let group_toggle = sidebar::render_parent_group_toggle(
                    buffer,
                    nested,
                    snapshot,
                    entry.index,
                    state.collapsed_groups,
                    palette,
                );
                hits.workspaces.push(WorkspaceHit {
                    rect,
                    endpoint_id: state.active_endpoint_id.clone(),
                    workspace_id: workspace.workspace_id.clone(),
                    indented: entry.indented,
                    group_toggle,
                });
            }
        }
        y = y.saturating_add(height);
    }
    if scrollbar {
        let track = Rect::new(body.right().saturating_sub(1), body.y, 1, body.height);
        hits.workspace_scrollbar = track;
        super::scroll::render_list_scrollbar(buffer, track, metrics, palette);
    }
    true
}
