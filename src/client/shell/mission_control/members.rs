//! Confirmed member metadata projections, outside terminal locks and drawing.
use super::*;
use crate::organization::{FamilyId, MissionId, MissionTarget, OrganizationState};

fn catalog(endpoint: &ClientShellEndpoint) -> Option<&OrganizationState> {
    endpoint
        .organization
        .as_ref()
        .filter(|_| endpoint.organization_supported)
        .map(|c| &c.organization)
}

pub(super) fn target_applicable(endpoint: &ClientShellEndpoint, target: &SpaceTarget) -> bool {
    let Some(snapshot) = endpoint.snapshot.as_deref() else {
        return false;
    };
    if snapshot.boot_id != target.boot_id || endpoint.snapshot_generation != target.generation {
        return false;
    }
    match &target.applicability {
        TargetApplicability::Spaces => match &target.focus {
            ClientEndpointFocusTarget::Workspace(id) => {
                snapshot.workspaces.iter().any(|w| &w.workspace_id == id)
            }
            ClientEndpointFocusTarget::Tab(id) => snapshot.tabs.iter().any(|t| &t.tab_id == id),
            ClientEndpointFocusTarget::Pane(id) => snapshot.panes.iter().any(|p| &p.pane_id == id),
            #[cfg(windows)]
            ClientEndpointFocusTarget::Notification { .. } => false,
        },
        TargetApplicability::Mission(mission) => {
            catalog(endpoint).is_some_and(|c| match &target.focus {
                ClientEndpointFocusTarget::Tab(id) => {
                    snapshot.tabs.iter().any(|t| &t.tab_id == id)
                        && c.mission_for(&MissionTarget::Tab { tab_id: id.clone() })
                            == Some(mission)
                }
                ClientEndpointFocusTarget::Pane(id) => snapshot
                    .panes
                    .iter()
                    .find(|p| &p.pane_id == id)
                    .is_some_and(|p| c.effective_pane_mission(id, &p.tab_id) == Some(mission)),
                _ => false,
            })
        }
        TargetApplicability::NeedsYou => match &target.focus {
            ClientEndpointFocusTarget::Pane(id) => {
                snapshot.panes.iter().any(|p| &p.pane_id == id)
                    && snapshot.agents.iter().any(|a| {
                        &a.pane_id == id
                            && a.agent_status == crate::api::schema::AgentStatus::Blocked
                    })
            }
            _ => false,
        },
    }
}

fn parked(catalog: &OrganizationState, workspace: &crate::protocol::ClientShellWorkspace) -> bool {
    let family = workspace.worktree.as_ref().map_or_else(
        || FamilyId::Standalone {
            workspace_id: workspace.workspace_id.clone(),
        },
        |w| FamilyId::Managed { key: w.key.clone() },
    );
    catalog
        .collection_for(&family)
        .and_then(|id| catalog.collections.iter().find(|c| &c.id == id))
        .is_some_and(|c| c.hibernating)
}

fn target(
    endpoint: &ClientShellEndpoint,
    focus: ClientEndpointFocusTarget,
    mission: &MissionId,
) -> Option<SpaceSelection> {
    Some(SpaceSelection::Target(SpaceTarget {
        endpoint_id: endpoint.endpoint_id.clone(),
        boot_id: endpoint.snapshot.as_ref()?.boot_id.clone(),
        generation: endpoint.snapshot_generation,
        focus,
        applicability: TargetApplicability::Mission(mission.clone()),
    }))
}

pub(super) fn project_missions(endpoint: &ClientShellEndpoint) -> Vec<SpaceRow> {
    let Some(snapshot) = endpoint.snapshot.as_deref() else {
        return Vec::new();
    };
    let Some(catalog) = catalog(endpoint) else {
        return Vec::new();
    };
    let mut missions = catalog.missions.iter().collect::<Vec<_>>();
    missions.sort_by_key(|m| m.order);
    let workspaces = snapshot
        .workspaces
        .iter()
        .map(|w| (w.workspace_id.as_str(), w))
        .collect::<HashMap<_, _>>();
    let tabs = snapshot
        .tabs
        .iter()
        .map(|t| (t.tab_id.as_str(), t))
        .collect::<HashMap<_, _>>();
    let agents = snapshot
        .agents
        .iter()
        .map(|a| (a.pane_id.as_str(), a))
        .collect::<HashMap<_, _>>();
    let mut rows = Vec::new();
    for mission in missions {
        let objective = mission.objective.as_deref().unwrap_or_default();
        rows.push(SpaceRow {
            label: mission.name.as_str().into(),
            detail: format!("{} · {objective}", mission.id.0),
            depth: 0,
            search: format!("{} {} {objective}", mission.name.as_str(), mission.id.0)
                .to_lowercase(),
            selection: Some(SpaceSelection::Mission(mission.id.clone())),
            section: None,
            parked: false,
            family_key: None,
        });
        for tab in &snapshot.tabs {
            if catalog.mission_for(&MissionTarget::Tab {
                tab_id: tab.tab_id.clone(),
            }) != Some(&mission.id)
            {
                continue;
            }
            let Some(workspace) = workspaces.get(tab.workspace_id.as_str()) else {
                continue;
            };
            let detail = format!(
                "{} · explicit tab · {} · {} · {}",
                tab.tab_id,
                workspace.label,
                workspace.branch.as_deref().unwrap_or_default(),
                workspace.new_workspace_cwd
            );
            rows.push(SpaceRow {
                label: tab.label.clone(),
                search: format!(
                    "{} {detail} {} {objective}",
                    tab.label,
                    mission.name.as_str()
                )
                .to_lowercase(),
                detail,
                depth: 1,
                selection: target(
                    endpoint,
                    ClientEndpointFocusTarget::Tab(tab.tab_id.clone()),
                    &mission.id,
                ),
                section: None,
                parked: parked(catalog, workspace),
                family_key: None,
            });
        }
        for pane in &snapshot.panes {
            if catalog.effective_pane_mission(&pane.pane_id, &pane.tab_id) != Some(&mission.id) {
                continue;
            }
            let (Some(workspace), Some(tab)) = (
                workspaces.get(pane.workspace_id.as_str()),
                tabs.get(pane.tab_id.as_str()),
            ) else {
                continue;
            };
            let agent = agents.get(pane.pane_id.as_str()).copied();
            let label = agent
                .and_then(|a| a.display_agent.as_deref().or(a.name.as_deref()))
                .or(pane.label.as_deref())
                .or_else(|| agent.and_then(|a| a.agent.as_deref()))
                .unwrap_or("Terminal");
            let source = if catalog.pane_override(&pane.pane_id).is_some() {
                "explicit override"
            } else {
                "inherited tab"
            };
            let cwd = pane
                .foreground_cwd
                .as_deref()
                .or(pane.cwd.as_deref())
                .unwrap_or(&workspace.new_workspace_cwd);
            let detail = format!(
                "{} · {source} · {} / {} · {cwd}",
                pane.pane_id, workspace.label, tab.label
            );
            let labels = agent
                .map(|a| {
                    a.tokens
                        .iter()
                        .chain(&a.state_labels)
                        .map(|(_, value)| value.as_str())
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
            let search = format!(
                "{label} {detail} {} {} {} {labels} {} {} {objective}",
                pane.label.as_deref().unwrap_or_default(),
                workspace.branch.as_deref().unwrap_or_default(),
                workspace.new_workspace_cwd,
                workspace.worktree.as_ref().map_or("", |w| w.label.as_str()),
                mission.name.as_str()
            )
            .to_lowercase();
            rows.push(SpaceRow {
                label: label.into(),
                detail,
                depth: 1,
                search,
                selection: target(
                    endpoint,
                    ClientEndpointFocusTarget::Pane(pane.pane_id.clone()),
                    &mission.id,
                ),
                section: None,
                parked: parked(catalog, workspace),
                family_key: None,
            });
        }
    }
    rows
}

pub(super) fn project_needs_you(endpoint: &ClientShellEndpoint) -> Vec<SpaceRow> {
    let Some(snapshot) = endpoint.snapshot.as_deref() else {
        return Vec::new();
    };
    let mut agents = snapshot
        .agents
        .iter()
        .filter(|a| a.agent_status == crate::api::schema::AgentStatus::Blocked)
        .collect::<Vec<_>>();
    // Existing authoritative state sequence orders equal-priority agents. A
    // custom display filter cannot suppress an actual request for attention.
    agents.sort_by_key(|a| std::cmp::Reverse(a.state_change_seq));
    let panes = snapshot
        .panes
        .iter()
        .map(|p| (p.pane_id.as_str(), p))
        .collect::<HashMap<_, _>>();
    let tabs = snapshot
        .tabs
        .iter()
        .map(|t| (t.tab_id.as_str(), t))
        .collect::<HashMap<_, _>>();
    let workspaces = snapshot
        .workspaces
        .iter()
        .map(|w| (w.workspace_id.as_str(), w))
        .collect::<HashMap<_, _>>();
    let catalog = catalog(endpoint);
    agents
        .into_iter()
        .filter_map(|agent| {
            let pane = panes.get(agent.pane_id.as_str())?;
            let tab = tabs.get(pane.tab_id.as_str())?;
            let workspace = workspaces.get(pane.workspace_id.as_str())?;
            let label = agent
                .display_agent
                .as_deref()
                .or(agent.name.as_deref())
                .or(pane.label.as_deref())
                .or(agent.agent.as_deref())
                .unwrap_or("Agent");
            let state = agent
                .state_labels
                .iter()
                .find(|(key, _)| key == "blocked")
                .map_or("blocked", |(_, value)| value.as_str());
            let membership = catalog
                .and_then(|c| {
                    c.effective_pane_mission(&pane.pane_id, &pane.tab_id)
                        .and_then(|id| c.missions.iter().find(|m| &m.id == id))
                })
                .map_or("", |m| m.name.as_str());
            let cwd = pane
                .foreground_cwd
                .as_deref()
                .or(pane.cwd.as_deref())
                .unwrap_or(&workspace.new_workspace_cwd);
            let detail = format!(
                "{} · {state} · {} / {} · {cwd} · {membership}",
                pane.pane_id, workspace.label, tab.label
            );
            let labels = agent
                .tokens
                .iter()
                .chain(&agent.state_labels)
                .map(|(_, value)| value.as_str())
                .chain(
                    [
                        agent.name.as_deref(),
                        agent.display_agent.as_deref(),
                        agent.agent.as_deref(),
                        agent.title.as_deref(),
                        pane.label.as_deref(),
                    ]
                    .into_iter()
                    .flatten(),
                )
                .collect::<Vec<_>>()
                .join(" ");
            Some(SpaceRow {
                label: label.into(),
                search: format!(
                    "{label} {detail} {labels} {} {} {}",
                    workspace.branch.as_deref().unwrap_or_default(),
                    workspace.new_workspace_cwd,
                    workspace.worktree.as_ref().map_or("", |w| w.label.as_str())
                )
                .to_lowercase(),
                detail,
                depth: 0,
                selection: Some(SpaceSelection::Target(SpaceTarget {
                    endpoint_id: endpoint.endpoint_id.clone(),
                    boot_id: snapshot.boot_id.clone(),
                    generation: endpoint.snapshot_generation,
                    focus: ClientEndpointFocusTarget::Pane(pane.pane_id.clone()),
                    applicability: TargetApplicability::NeedsYou,
                })),
                section: None,
                parked: catalog.is_some_and(|c| parked(c, workspace)),
                family_key: None,
            })
        })
        .collect()
}
