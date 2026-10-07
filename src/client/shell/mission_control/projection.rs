//! Compare semantic input without formatting rows or copying terminal snapshots.
use super::*;
use crate::organization::{FamilyId, MissionId, MissionTarget, OrganizationState};
use crate::protocol::{ClientShellAgent, ClientShellPane, ClientShellTab};

#[cfg(test)]
thread_local! {
    // Actual pane identity comparisons while qualifying queue members. Tests
    // observe the delta after public source ingress/composition, never call a
    // private comparator to manufacture work.
    static QUEUE_METADATA_PROBES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(super) fn queue_metadata_probes() -> usize {
    QUEUE_METADATA_PROBES.with(|count| count.get())
}

pub(super) fn unchanged(
    view: MissionControlView,
    previous: &ClientShellSnapshot,
    current: &ClientShellSnapshot,
    previous_catalog: Option<&OrganizationState>,
    current_catalog: Option<&OrganizationState>,
) -> bool {
    if view == MissionControlView::Missions {
        return missions_unchanged(previous, current, previous_catalog, current_catalog);
    }
    if view == MissionControlView::NeedsYou {
        return needs_you_unchanged(previous, current, previous_catalog, current_catalog);
    }
    spaces_catalog_unchanged(previous, previous_catalog, current_catalog)
        && previous.workspaces.len() == current.workspaces.len()
        && previous
            .workspaces
            .iter()
            .zip(&current.workspaces)
            .all(|(a, b)| {
                a.workspace_id == b.workspace_id
                    && a.label == b.label
                    && a.new_workspace_cwd == b.new_workspace_cwd
                    && a.branch == b.branch
                    && same_values(&a.tokens, &b.tokens)
                    && a.worktree == b.worktree
            })
        && previous.tabs.len() == current.tabs.len()
        && previous.tabs.iter().zip(&current.tabs).all(|(a, b)| {
            a.tab_id == b.tab_id && a.workspace_id == b.workspace_id && a.label == b.label
        })
        && previous.panes.len() == current.panes.len()
        && previous.panes.iter().zip(&current.panes).all(|(a, b)| {
            a.pane_id == b.pane_id
                && a.workspace_id == b.workspace_id
                && a.tab_id == b.tab_id
                && a.label == b.label
                && a.cwd == b.cwd
                && a.foreground_cwd == b.foreground_cwd
        })
        && previous.panes.iter().all(|pane| {
            same_agent(
                agent(previous, &pane.pane_id),
                agent(current, &pane.pane_id),
            )
        })
}

fn queue_member<'a>(
    snapshot: &'a ClientShellSnapshot,
    a: &ClientShellAgent,
) -> Option<(
    &'a ClientShellPane,
    &'a ClientShellWorkspace,
    &'a ClientShellTab,
)> {
    if a.agent_status != crate::api::schema::AgentStatus::Blocked {
        return None;
    }
    let p = snapshot.panes.iter().find(|p| {
        #[cfg(test)]
        QUEUE_METADATA_PROBES.with(|count| count.set(count.get() + 1));
        p.pane_id == a.pane_id
    })?;
    Some((
        p,
        workspace(snapshot, &p.workspace_id)?,
        snapshot.tabs.iter().find(|t| t.tab_id == p.tab_id)?,
    ))
}

fn queue_ties_unchanged(a: &ClientShellSnapshot, b: &ClientShellSnapshot) -> bool {
    use crate::api::schema::AgentStatus;
    // Stable sorting preserves source order only within equal-sequence groups.
    // Visit each group once, including when its first source agent is orphaned.
    a.agents.iter().enumerate().all(|(index, agent)| {
        if agent.agent_status != AgentStatus::Blocked
            || a.agents[..index].iter().any(|previous| {
                previous.agent_status == AgentStatus::Blocked
                    && previous.state_change_seq == agent.state_change_seq
            })
        {
            return true;
        }
        same_ordered(
            a.agents.iter().filter(|candidate| {
                candidate.state_change_seq == agent.state_change_seq
                    && queue_member(a, candidate).is_some()
            }),
            b.agents.iter().filter(|candidate| {
                candidate.state_change_seq == agent.state_change_seq
                    && queue_member(b, candidate).is_some()
            }),
            |left, right| left.pane_id == right.pane_id,
        )
    })
}

fn needs_you_unchanged(
    a: &ClientShellSnapshot,
    b: &ClientShellSnapshot,
    ac: Option<&OrganizationState>,
    bc: Option<&OrganizationState>,
) -> bool {
    let count = |s: &ClientShellSnapshot| {
        s.agents
            .iter()
            .filter(|p| queue_member(s, p).is_some())
            .count()
    };
    // A definite member-field change already requires projection; avoid global
    // qualification and ordering work until all existing member fields agree.
    a.agents
        .iter()
        .filter_map(|aa| Some((aa, queue_member(a, aa)?)))
        .all(|(aa, (ap, aw, at))| {
            let Some(ba) = agent(b, &aa.pane_id) else {
                return false;
            };
            let Some((bp, bw, bt)) = queue_member(b, ba) else {
                return false;
            };
            aa.state_change_seq == ba.state_change_seq
                && same_agent(Some(aa), Some(ba))
                && blocked_caption(aa) == blocked_caption(ba)
                && ap.pane_id == bp.pane_id
                && ap.workspace_id == bp.workspace_id
                && ap.tab_id == bp.tab_id
                && ap.label == bp.label
                && ap.cwd == bp.cwd
                && ap.foreground_cwd == bp.foreground_cwd
                && at.label == bt.label
                && same_location(aw, bw, ac, bc)
                && aw.worktree.as_ref().map(|w| &w.label) == bw.worktree.as_ref().map(|w| &w.label)
                && mission_name(ac, pane_mission(ac, ap)) == mission_name(bc, pane_mission(bc, bp))
        })
        && count(a) == count(b)
        && queue_ties_unchanged(a, b)
}

fn same_ordered<A, B>(
    mut a: impl Iterator<Item = A>,
    mut b: impl Iterator<Item = B>,
    same: impl Fn(A, B) -> bool,
) -> bool {
    loop {
        match (a.next(), b.next()) {
            (Some(a), Some(b)) => {
                if !same(a, b) {
                    return false;
                }
            }
            (None, None) => return true,
            _ => return false,
        }
    }
}

fn workspace<'a>(snapshot: &'a ClientShellSnapshot, id: &str) -> Option<&'a ClientShellWorkspace> {
    snapshot.workspaces.iter().find(|w| w.workspace_id == id)
}

fn agent<'a>(snapshot: &'a ClientShellSnapshot, id: &str) -> Option<&'a ClientShellAgent> {
    snapshot.agents.iter().find(|a| a.pane_id == id)
}

/// Search consumes the union of values, independently of unused keys/order.
fn same_values(a: &[(String, String)], b: &[(String, String)]) -> bool {
    a.iter()
        .all(|(_, value)| b.iter().any(|(_, other)| value == other))
        && b.iter()
            .all(|(_, value)| a.iter().any(|(_, other)| value == other))
}

fn blocked_caption(agent: &ClientShellAgent) -> &str {
    agent
        .state_labels
        .iter()
        .find(|(key, _)| key == "blocked")
        .map_or("blocked", |(_, value)| value.as_str())
}

fn same_agent(a: Option<&ClientShellAgent>, b: Option<&ClientShellAgent>) -> bool {
    a.and_then(|a| a.name.as_deref()) == b.and_then(|a| a.name.as_deref())
        && a.and_then(|a| a.display_agent.as_deref()) == b.and_then(|a| a.display_agent.as_deref())
        && a.and_then(|a| a.agent.as_deref()) == b.and_then(|a| a.agent.as_deref())
        && a.and_then(|a| a.title.as_deref()) == b.and_then(|a| a.title.as_deref())
        && same_values(
            a.map_or(&[][..], |a| a.tokens.as_slice()),
            b.map_or(&[][..], |b| b.tokens.as_slice()),
        )
        && same_values(
            a.map_or(&[][..], |a| a.state_labels.as_slice()),
            b.map_or(&[][..], |b| b.state_labels.as_slice()),
        )
}

fn is_parked(catalog: Option<&OrganizationState>, w: &ClientShellWorkspace) -> bool {
    workspace_collection(catalog, w)
        .and_then(|id| catalog?.collections.iter().find(|c| &c.id == id))
        .is_some_and(|c| c.hibernating)
}

fn same_location(
    a: &ClientShellWorkspace,
    b: &ClientShellWorkspace,
    ac: Option<&OrganizationState>,
    bc: Option<&OrganizationState>,
) -> bool {
    a.label == b.label
        && a.branch == b.branch
        && a.new_workspace_cwd == b.new_workspace_cwd
        && is_parked(ac, a) == is_parked(bc, b)
}

fn tab_member<'a>(
    snapshot: &'a ClientShellSnapshot,
    catalog: Option<&OrganizationState>,
    t: &ClientShellTab,
) -> Option<&'a ClientShellWorkspace> {
    mission_name(catalog, tab_mission(catalog, &t.tab_id))?;
    workspace(snapshot, &t.workspace_id)
}

fn pane_member<'a>(
    snapshot: &'a ClientShellSnapshot,
    catalog: Option<&OrganizationState>,
    p: &ClientShellPane,
) -> Option<(&'a ClientShellWorkspace, &'a ClientShellTab)> {
    mission_name(catalog, pane_mission(catalog, p))?;
    Some((
        workspace(snapshot, &p.workspace_id)?,
        snapshot.tabs.iter().find(|t| t.tab_id == p.tab_id)?,
    ))
}

fn missions_unchanged(
    a: &ClientShellSnapshot,
    b: &ClientShellSnapshot,
    ac: Option<&OrganizationState>,
    bc: Option<&OrganizationState>,
) -> bool {
    let am = ac.map_or(&[][..], |c| c.missions.as_slice());
    let bm = bc.map_or(&[][..], |c| c.missions.as_slice());
    am.len() == bm.len()
        && am.iter().all(|m| bm.contains(m))
        && am.iter().all(|mission| {
            same_ordered(
                a.tabs
                    .iter()
                    .filter(|t| tab_mission(ac, &t.tab_id) == Some(&mission.id))
                    .filter_map(|t| Some((t, tab_member(a, ac, t)?))),
                b.tabs
                    .iter()
                    .filter(|t| tab_mission(bc, &t.tab_id) == Some(&mission.id))
                    .filter_map(|t| Some((t, tab_member(b, bc, t)?))),
                |(at, aw), (bt, bw)| {
                    at.tab_id == bt.tab_id
                        && at.workspace_id == bt.workspace_id
                        && at.label == bt.label
                        && tab_mission(ac, &at.tab_id) == tab_mission(bc, &bt.tab_id)
                        && same_location(aw, bw, ac, bc)
                },
            ) && same_ordered(
                a.panes
                    .iter()
                    .filter(|p| pane_mission(ac, p) == Some(&mission.id))
                    .filter_map(|p| Some((p, pane_member(a, ac, p)?))),
                b.panes
                    .iter()
                    .filter(|p| pane_mission(bc, p) == Some(&mission.id))
                    .filter_map(|p| Some((p, pane_member(b, bc, p)?))),
                |(ap, (aw, at)), (bp, (bw, bt))| {
                    ap.pane_id == bp.pane_id
                        && ap.workspace_id == bp.workspace_id
                        && ap.tab_id == bp.tab_id
                        && ap.label == bp.label
                        && ap.cwd == bp.cwd
                        && ap.foreground_cwd == bp.foreground_cwd
                        && pane_mission(ac, ap) == pane_mission(bc, bp)
                        && ac.is_some_and(|c| c.pane_override(&ap.pane_id).is_some())
                            == bc.is_some_and(|c| c.pane_override(&bp.pane_id).is_some())
                        && same_location(aw, bw, ac, bc)
                        && at.label == bt.label
                        && aw.worktree.as_ref().map(|w| &w.label)
                            == bw.worktree.as_ref().map(|w| &w.label)
                        && same_agent(agent(a, &ap.pane_id), agent(b, &bp.pane_id))
                },
            )
        })
}

fn tab_mission<'a>(catalog: Option<&'a OrganizationState>, tab: &str) -> Option<&'a MissionId> {
    catalog?.mission_assignments.iter().find(|assignment| {
        matches!(&assignment.target, MissionTarget::Tab { tab_id } if tab_id == tab)
    }).map(|assignment| &assignment.mission_id)
}

fn pane_mission<'a>(
    catalog: Option<&'a OrganizationState>,
    pane: &ClientShellPane,
) -> Option<&'a MissionId> {
    catalog?
        .pane_override(&pane.pane_id)
        .or_else(|| tab_mission(catalog, &pane.tab_id))
}

fn mission_name<'a>(
    catalog: Option<&'a OrganizationState>,
    mission: Option<&MissionId>,
) -> Option<&'a str> {
    let mission = mission?;
    catalog?
        .missions
        .iter()
        .find(|m| &m.id == mission)
        .map(|m| m.name.as_str())
}

fn workspace_collection<'a>(
    catalog: Option<&'a OrganizationState>,
    workspace: &ClientShellWorkspace,
) -> Option<&'a crate::organization::CollectionId> {
    catalog?
        .family_assignments
        .iter()
        .find(|a| match (&a.family_id, &workspace.worktree) {
            (FamilyId::Managed { key }, Some(worktree)) => key == &worktree.key,
            (FamilyId::Standalone { workspace_id }, None) => {
                workspace_id == &workspace.workspace_id
            }
            _ => false,
        })
        .map(|a| &a.collection_id)
}

fn spaces_catalog_unchanged(
    snapshot: &ClientShellSnapshot,
    a: Option<&OrganizationState>,
    b: Option<&OrganizationState>,
) -> bool {
    let ac = a.map_or(&[][..], |c| c.collections.as_slice());
    let bc = b.map_or(&[][..], |c| c.collections.as_slice());
    ac.len() == bc.len()
        && ac.iter().all(|c| bc.contains(c))
        && snapshot
            .workspaces
            .iter()
            .all(|w| workspace_collection(a, w) == workspace_collection(b, w))
        && snapshot.tabs.iter().all(|t| {
            mission_name(a, tab_mission(a, &t.tab_id)) == mission_name(b, tab_mission(b, &t.tab_id))
        })
        && snapshot
            .panes
            .iter()
            .all(|p| mission_name(a, pane_mission(a, p)) == mission_name(b, pane_mission(b, p)))
}
