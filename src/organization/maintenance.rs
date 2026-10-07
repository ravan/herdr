use super::*;

impl OrganizationState {
    pub fn rename_collection(
        &mut self,
        id: &CollectionId,
        name: CollectionName,
    ) -> Result<bool, &'static str> {
        let index = self
            .collections
            .iter()
            .position(|c| &c.id == id)
            .ok_or("collection_not_found")?;
        if self.collections[index].name == name {
            return Ok(false);
        }
        let revision = self.next_revision(self.standalone_count())?;
        self.collections[index].name = name;
        self.revision = revision;
        Ok(true)
    }
}

impl OrganizationState {
    pub fn rename_mission(
        &mut self,
        id: &MissionId,
        name: MissionName,
    ) -> Result<bool, &'static str> {
        let index = self
            .missions
            .iter()
            .position(|m| &m.id == id)
            .ok_or("mission_not_found")?;
        if self.missions[index].name == name {
            return Ok(false);
        }
        let revision = self.next_revision(self.standalone_count())?;
        self.missions[index].name = name;
        self.revision = revision;
        Ok(true)
    }
}

impl OrganizationState {
    pub fn move_collection(
        &mut self,
        id: &CollectionId,
        to_index: u64,
    ) -> Result<bool, &'static str> {
        let mut ordered = self.collections.clone();
        ordered.sort_by_key(|item| item.order);
        let from = ordered
            .iter()
            .position(|c| &c.id == id)
            .ok_or("collection_not_found")?;
        let to = usize::try_from(to_index)
            .ok()
            .filter(|&i| i < self.collections.len())
            .ok_or("invalid_order_index")?;
        if from == to {
            return Ok(false);
        }
        let revision = self.next_revision(self.standalone_count())?;
        let moved = ordered.remove(from);
        ordered.insert(to, moved);
        for (index, collection) in ordered.iter_mut().enumerate() {
            collection.order = index as u64;
        }
        self.collections = ordered;
        self.revision = revision;
        Ok(true)
    }
}

impl OrganizationState {
    pub fn move_mission(&mut self, id: &MissionId, to_index: u64) -> Result<bool, &'static str> {
        let mut ordered = self.missions.clone();
        ordered.sort_by_key(|item| item.order);
        let from = ordered
            .iter()
            .position(|m| &m.id == id)
            .ok_or("mission_not_found")?;
        let to = usize::try_from(to_index)
            .ok()
            .filter(|&i| i < self.missions.len())
            .ok_or("invalid_order_index")?;
        if from == to {
            return Ok(false);
        }
        let revision = self.next_revision(self.standalone_count())?;
        let moved = ordered.remove(from);
        ordered.insert(to, moved);
        for (index, mission) in ordered.iter_mut().enumerate() {
            mission.order = index as u64;
        }
        self.missions = ordered;
        self.revision = revision;
        Ok(true)
    }
}

impl OrganizationState {
    pub fn set_mission_objective(
        &mut self,
        id: &MissionId,
        objective: Option<String>,
    ) -> Result<bool, &'static str> {
        let index = self
            .missions
            .iter()
            .position(|m| &m.id == id)
            .ok_or("mission_not_found")?;
        let objective = objective.filter(|text| !text.trim().is_empty());
        if self.missions[index].objective == objective {
            return Ok(false);
        }
        let revision = self.next_revision(self.standalone_count())?;
        self.missions[index].objective = objective;
        self.revision = revision;
        Ok(true)
    }
}

impl OrganizationState {
    pub fn delete_collection(&mut self, id: &CollectionId) -> Result<bool, &'static str> {
        let index = self
            .collections
            .iter()
            .position(|c| &c.id == id)
            .ok_or("collection_not_found")?;
        let surviving_standalones = self
            .family_assignments
            .iter()
            .filter(|a| {
                &a.collection_id != id && matches!(a.family_id, FamilyId::Standalone { .. })
            })
            .count();
        let revision = self.next_revision(surviving_standalones)?;
        self.collections.remove(index);
        self.family_assignments.retain(|a| &a.collection_id != id);
        self.collections.sort_by_key(|item| item.order);
        for (index, collection) in self.collections.iter_mut().enumerate() {
            collection.order = index as u64;
        }
        self.revision = revision;
        Ok(true)
    }
}

impl OrganizationState {
    pub fn unassign_family(
        &mut self,
        family: &FamilyId,
        expected: &CollectionId,
        live: &[FamilyId],
    ) -> Result<bool, &'static str> {
        if !self.collections.iter().any(|c| &c.id == expected) {
            return Err("collection_not_found");
        }
        if !live.contains(family) {
            return Err("family_not_found");
        }
        let Some(current) = self.collection_for(family) else {
            return Ok(false);
        };
        if current != expected {
            return Err("assignment_changed");
        }
        let surviving =
            self.standalone_count() - usize::from(matches!(family, FamilyId::Standalone { .. }));
        let revision = self.next_revision(surviving)?;
        self.family_assignments.retain(|a| &a.family_id != family);
        self.revision = revision;
        Ok(true)
    }
}

impl OrganizationState {
    pub fn unassign_mission(
        &mut self,
        target: &MissionTarget,
        expected: &MissionId,
        live: &[MissionTarget],
    ) -> Result<bool, &'static str> {
        if !self.missions.iter().any(|m| &m.id == expected) {
            return Err("mission_not_found");
        }
        if !live.contains(target) {
            return Err("tab_not_found");
        }
        let Some(current) = self.mission_for(target) else {
            return Ok(false);
        };
        if current != expected {
            return Err("assignment_changed");
        }
        let revision = self.next_revision_with_targets(
            self.standalone_count(),
            self.mission_assignments.len() - 1,
        )?;
        self.mission_assignments.retain(|a| &a.target != target);
        self.revision = revision;
        Ok(true)
    }
}

impl OrganizationState {
    pub fn delete_mission(&mut self, id: &MissionId) -> Result<bool, &'static str> {
        let index = self
            .missions
            .iter()
            .position(|m| &m.id == id)
            .ok_or("mission_not_found")?;
        let tabs = self
            .mission_assignments
            .iter()
            .filter(|a| &a.mission_id != id)
            .count();
        let panes = self
            .pane_mission_assignments
            .iter()
            .filter(|a| &a.mission_id != id)
            .count();
        let revision = self.next_revision_with_counts(self.standalone_count(), tabs, panes)?;
        self.missions.remove(index);
        self.mission_assignments.retain(|a| &a.mission_id != id);
        self.pane_mission_assignments
            .retain(|a| &a.mission_id != id);
        self.missions.sort_by_key(|item| item.order);
        for (index, mission) in self.missions.iter_mut().enumerate() {
            mission.order = index as u64;
        }
        self.revision = revision;
        Ok(true)
    }
}
