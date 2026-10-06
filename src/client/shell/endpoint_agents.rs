use super::render::put_text;
use super::*;

pub(super) fn render_collapsed(
    buffer: &mut Buffer,
    area: Rect,
    endpoints: &[ClientShellEndpoint],
    active_endpoint_id: &ClientEndpointId,
    config: &ClientShellConfig,
    hits: &mut ShellHitMap,
) {
    let rows = agent_rows(endpoints, active_endpoint_id, config);
    for (index, row) in rows.into_iter().take(area.height as usize).enumerate() {
        let rect = Rect::new(area.x, area.y + index as u16, area.width, 1);
        if row.agent.focused {
            buffer.set_style(rect, Style::default().bg(config.palette.active_row_bg));
        }
        let initial = row.machine_label.chars().next().unwrap_or('?');
        put_text(
            buffer,
            rect.x,
            rect.y,
            rect.width,
            &format!(
                "{initial}{}",
                status_icon(row.agent.status, config.status_indicators)
            ),
            Style::default()
                .fg(if row.stale {
                    config.palette.overlay0
                } else {
                    status_color(row.agent.status, &config.palette)
                })
                .add_modifier(if row.stale {
                    Modifier::DIM
                } else {
                    Modifier::empty()
                }),
        );
        hits.endpoint_agents
            .push((rect, row.endpoint_id, row.agent.pane_id));
    }
}

pub(super) fn render_expanded(
    buffer: &mut Buffer,
    area: Rect,
    agent_view_label: Option<&str>,
    endpoints: &[ClientShellEndpoint],
    active_endpoint_id: &ClientEndpointId,
    config: &ClientShellConfig,
    agent_scroll: &mut usize,
    hits: &mut ShellHitMap,
) {
    if !super::agent_sidebar::render_agent_panel_header(
        buffer,
        area,
        agent_view_label,
        config,
        hits,
    ) {
        return;
    }
    let rows = agent_rows(endpoints, active_endpoint_id, config);
    super::agent_sidebar::render_agent_list(
        buffer,
        area,
        &rows,
        agent_view_label.map(|_| " no matching agents"),
        config,
        agent_scroll,
        hits,
        |row| row.height(),
        |buffer, rect, row, hits| {
            let heading_height = u16::from(row.heading.is_some());
            if let Some(heading) = &row.heading {
                put_text(
                    buffer,
                    rect.x,
                    rect.y,
                    rect.width,
                    heading,
                    Style::default()
                        .fg(config.palette.accent)
                        .add_modifier(Modifier::BOLD),
                );
            }
            let agent_rect = Rect::new(
                rect.x,
                rect.y + heading_height,
                rect.width,
                rect.height.saturating_sub(heading_height),
            );
            super::agent_sidebar::render_agent_row(buffer, agent_rect, &row.agent, config);
            if row.parked && agent_rect.height > row.agent.rows.len() as u16 {
                put_text(
                    buffer,
                    agent_rect.x + 3,
                    agent_rect.bottom() - 1,
                    agent_rect.width.saturating_sub(3),
                    "Hibernate",
                    Style::default().fg(config.palette.overlay0),
                );
            }
            if row.stale {
                buffer.set_style(
                    rect,
                    Style::default()
                        .fg(config.palette.overlay0)
                        .add_modifier(Modifier::DIM),
                );
            }
            if endpoints.len() == 1 {
                hits.agents.push((agent_rect, row.agent.pane_id.clone()));
            }
            hits.endpoint_agents.push((
                agent_rect,
                row.endpoint_id.clone(),
                row.agent.pane_id.clone(),
            ));
        },
    );
}

impl ClientShellState {
    pub(super) fn reveal_endpoint_agent(
        &mut self,
        endpoint_id: &ClientEndpointId,
        pane_id: &str,
        body_height: u16,
    ) {
        if body_height == 0 {
            return;
        }
        let rows = agent_rows(&self.endpoints, &self.active_endpoint_id, &self.config);
        let Some(target) = rows
            .iter()
            .position(|row| &row.endpoint_id == endpoint_id && row.agent.pane_id == pane_id)
        else {
            return;
        };
        let heights = rows
            .iter()
            .map(|row| row.height().min(u16::MAX as usize) as u16)
            .collect::<Vec<_>>();
        let mut gaps = vec![self.config.agents.row_gap; rows.len()];
        if let Some(last) = gaps.last_mut() {
            *last = 0;
        }
        self.agent_scroll = super::scroll::list_scroll_start_to_reveal(
            &heights,
            &gaps,
            body_height,
            self.agent_scroll,
            target,
        );
    }
}

struct EndpointAgentRow {
    endpoint_id: ClientEndpointId,
    machine_label: String,
    stale: bool,
    agent: super::agent_sidebar::AgentRow,
    heading: Option<String>,
    parked: bool,
}

impl EndpointAgentRow {
    fn height(&self) -> usize {
        self.agent.rows.len().max(1)
            + usize::from(self.heading.is_some())
            + usize::from(self.parked)
    }
}

fn agent_rows(
    endpoints: &[ClientShellEndpoint],
    active_endpoint_id: &ClientEndpointId,
    config: &ClientShellConfig,
) -> Vec<EndpointAgentRow> {
    let mut rendered_rows = endpoints
        .iter()
        .filter_map(|endpoint| {
            endpoint.snapshot.as_deref().map(|snapshot| {
                snapshot
                    .agents
                    .iter()
                    .filter_map(|agent| {
                        super::agent_sidebar::agent_row(
                            snapshot,
                            &agent.pane_id,
                            config,
                            Some(&endpoint.label),
                        )
                    })
                    .map(|agent| ((endpoint.endpoint_id.clone(), agent.pane_id.clone()), agent))
                    .collect::<Vec<_>>()
            })
        })
        .flatten()
        .collect::<HashMap<_, _>>();

    let grouped = config.agent_panel_sort == crate::config::AgentPanelSortConfig::Missions
        && endpoints
            .iter()
            .find(|e| &e.endpoint_id == active_endpoint_id)
            .and_then(|e| e.snapshot.as_ref())
            .is_none_or(|s| s.agent_view_label.is_none());
    let mut previous_group = None;
    super::aggregate_navigation::aggregate_agent_rows(
        endpoints,
        active_endpoint_id,
        config.agent_panel_sort,
    )
    .into_iter()
    .filter_map(|row| {
        let key = (row.endpoint.endpoint_id.clone(), row.agent.pane_id.clone());
        let mut agent = rendered_rows.remove(&key)?;
        agent.focused &= row.endpoint.endpoint_id == active_endpoint_id;
        let membership = row.endpoint.pane_missions.get(&row.agent.pane_id);
        let group = (
            row.endpoint.endpoint_id.clone(),
            membership.and_then(|m| m.mission_id.clone()),
        );
        let heading = if grouped && previous_group.as_ref() != Some(&group) {
            previous_group = Some(group);
            Some(
                membership
                    .and_then(|m| m.label.as_ref())
                    .map_or_else(|| " Unassigned".to_owned(), |label| format!(" ◆ {label}")),
            )
        } else {
            None
        };
        Some(EndpointAgentRow {
            endpoint_id: row.endpoint.endpoint_id.clone(),
            machine_label: row.endpoint.label.to_owned(),
            stale: row.endpoint.stale(),
            agent,
            heading,
            parked: membership.is_some_and(|m| m.parked),
        })
    })
    .collect()
}
