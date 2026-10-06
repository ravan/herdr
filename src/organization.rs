//! Pure, server-owned organization of existing session resources.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct CollectionId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FamilyId {
    Managed { key: String },
    Standalone { workspace_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FamilyAssignment {
    pub family_id: FamilyId,
    pub collection_id: CollectionId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(try_from = "String")]
pub struct CollectionName(String);

impl CollectionName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for CollectionName {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let name = value.trim();
        if name.is_empty() {
            return Err("invalid_collection_name");
        }
        Ok(Self(name.to_owned()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Collection {
    pub id: CollectionId,
    pub name: CollectionName,
    pub order: u64,
    pub hibernating: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct MissionId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(try_from = "String")]
pub struct MissionName(String);
impl MissionName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for MissionName {
    type Error = &'static str;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        let name = value.trim();
        if name.is_empty() {
            return Err("invalid_mission_name");
        }
        Ok(Self(name.to_owned()))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Mission {
    pub id: MissionId,
    pub name: MissionName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub objective: Option<String>,
    pub order: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MissionTarget {
    Tab { tab_id: String },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MissionAssignment {
    pub target: MissionTarget,
    pub mission_id: MissionId,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct OrganizationState {
    pub revision: u64,
    pub collections: Vec<Collection>,
    #[serde(default)]
    pub family_assignments: Vec<FamilyAssignment>,
    #[serde(default)]
    pub missions: Vec<Mission>,
    #[serde(default)]
    pub mission_assignments: Vec<MissionAssignment>,
}

impl<'de> Deserialize<'de> for OrganizationState {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Catalog {
            revision: u64,
            collections: Vec<Collection>,
            #[serde(default)]
            family_assignments: Vec<FamilyAssignment>,
            #[serde(default)]
            missions: Vec<Mission>,
            #[serde(default)]
            mission_assignments: Vec<MissionAssignment>,
        }
        let catalog = Catalog::deserialize(deserializer)?;
        let state = Self {
            revision: catalog.revision,
            collections: catalog.collections,
            family_assignments: catalog.family_assignments,
            missions: catalog.missions,
            mission_assignments: catalog.mission_assignments,
        };
        state.validate().map_err(serde::de::Error::custom)?;
        Ok(state)
    }
}

impl OrganizationState {
    pub fn create_mission(
        &mut self,
        name: MissionName,
        objective: Option<String>,
    ) -> Result<Mission, &'static str> {
        let revision = self.next_revision(self.standalone_count())?;
        let mission = Mission {
            id: MissionId(format!("mission_{revision}")),
            name,
            objective,
            order: u64::try_from(self.missions.len())
                .map_err(|_| "organization_revision_exhausted")?,
        };
        if self
            .missions
            .iter()
            .any(|existing| existing.id == mission.id)
        {
            return Err("duplicate_mission");
        }
        self.missions.push(mission.clone());
        self.revision = revision;
        Ok(mission)
    }
    pub fn mission_for(&self, target: &MissionTarget) -> Option<&MissionId> {
        self.mission_assignments
            .iter()
            .find(|a| &a.target == target)
            .map(|a| &a.mission_id)
    }
    pub fn assign_mission(
        &mut self,
        target: MissionTarget,
        mission_id: MissionId,
        live: &[MissionTarget],
    ) -> Result<bool, &'static str> {
        if !self.missions.iter().any(|m| m.id == mission_id) {
            return Err("mission_not_found");
        }
        if !live.contains(&target) {
            return Err("tab_not_found");
        }
        if self.mission_for(&target) == Some(&mission_id) {
            return Ok(false);
        }
        let extra = usize::from(self.mission_for(&target).is_none());
        let revision = self.next_revision(self.standalone_count() + extra)?;
        self.mission_assignments.retain(|a| a.target != target);
        self.mission_assignments
            .push(MissionAssignment { target, mission_id });
        self.revision = revision;
        Ok(true)
    }
    pub fn transfer_mission_target(
        &mut self,
        old: &MissionTarget,
        new: MissionTarget,
    ) -> Result<(), &'static str> {
        let Some(index) = self
            .mission_assignments
            .iter()
            .position(|a| &a.target == old)
        else {
            return Ok(());
        };
        if self.mission_for(&new).is_some() {
            return Err("duplicate_mission_target");
        }
        let revision = self.next_revision(self.standalone_count())?;
        self.mission_assignments[index].target = new;
        self.revision = revision;
        Ok(())
    }
    pub fn retain_live_mission_targets(
        &mut self,
        live: &[MissionTarget],
    ) -> Result<Vec<MissionTarget>, &'static str> {
        let removed = self
            .mission_assignments
            .iter()
            .filter(|a| !live.contains(&a.target))
            .map(|a| a.target.clone())
            .collect::<Vec<_>>();
        if removed.is_empty() {
            return Ok(removed);
        }
        let revision = self.next_revision_with_targets(
            self.standalone_count(),
            self.mission_assignments.len() - removed.len(),
        )?;
        self.mission_assignments
            .retain(|a| !removed.contains(&a.target));
        self.revision = revision;
        Ok(removed)
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        let mut ids = std::collections::HashSet::new();
        for collection in &self.collections {
            if collection.id.0.is_empty() || !ids.insert(&collection.id) {
                return Err("duplicate_collection");
            }
        }
        let mut families = std::collections::HashSet::new();
        for assignment in &self.family_assignments {
            if !ids.contains(&assignment.collection_id) {
                return Err("collection_not_found");
            }
            if !families.insert(&assignment.family_id) {
                return Err("duplicate_family");
            }
            let id = match &assignment.family_id {
                FamilyId::Managed { key } => key,
                FamilyId::Standalone { workspace_id } => workspace_id,
            };
            if id.is_empty() {
                return Err("family_not_found");
            }
        }
        let mut missions = std::collections::HashSet::new();
        for mission in &self.missions {
            if mission.id.0.is_empty() || !missions.insert(&mission.id) {
                return Err("duplicate_mission");
            }
        }
        let mut targets = std::collections::HashSet::new();
        for assignment in &self.mission_assignments {
            if !missions.contains(&assignment.mission_id) {
                return Err("mission_not_found");
            }
            if !targets.insert(&assignment.target) {
                return Err("duplicate_mission_target");
            }
            let MissionTarget::Tab { tab_id } = &assignment.target;
            if tab_id.is_empty() {
                return Err("tab_not_found");
            }
        }
        let reserve = u64::try_from(self.standalone_count() + self.mission_assignments.len())
            .map_err(|_| "organization_revision_exhausted")?;
        if self.revision > u64::MAX - reserve {
            return Err("organization_revision_exhausted");
        }
        Ok(())
    }
    pub fn is_empty(&self) -> bool {
        self.revision == 0
            && self.collections.is_empty()
            && self.family_assignments.is_empty()
            && self.missions.is_empty()
            && self.mission_assignments.is_empty()
    }
    pub fn collection_for(&self, family: &FamilyId) -> Option<&CollectionId> {
        self.family_assignments
            .iter()
            .find(|assignment| &assignment.family_id == family)
            .map(|assignment| &assignment.collection_id)
    }

    pub fn transfer_family(&mut self, old: &FamilyId, new: FamilyId) -> Result<(), &'static str> {
        if old == &new {
            return Ok(());
        }
        let Some(collection_id) = self.collection_for(old).cloned() else {
            return Ok(());
        };
        let count = self
            .family_assignments
            .iter()
            .filter(|assignment| {
                &assignment.family_id != old
                    && assignment.family_id != new
                    && matches!(assignment.family_id, FamilyId::Standalone { .. })
            })
            .count()
            + usize::from(matches!(new, FamilyId::Standalone { .. }));
        let revision = self.next_revision(count)?;
        self.family_assignments
            .retain(|assignment| &assignment.family_id != old && assignment.family_id != new);
        self.family_assignments.push(FamilyAssignment {
            family_id: new,
            collection_id,
        });
        self.revision = revision;
        Ok(())
    }
    pub fn create_collection(
        &mut self,
        id: CollectionId,
        name: CollectionName,
    ) -> Result<Collection, &'static str> {
        if self
            .collections
            .iter()
            .any(|collection| collection.id == id)
        {
            return Err("duplicate_collection");
        }
        let revision = self.next_revision(self.standalone_count())?;
        let order =
            u64::try_from(self.collections.len()).map_err(|_| "organization_revision_exhausted")?;
        let collection = Collection {
            id,
            name,
            order,
            hibernating: false,
        };
        self.collections.push(collection.clone());
        self.revision = revision;
        Ok(collection)
    }

    pub fn set_hibernating(
        &mut self,
        id: &CollectionId,
        hibernating: bool,
    ) -> Result<bool, &'static str> {
        let index = self
            .collections
            .iter()
            .position(|collection| &collection.id == id)
            .ok_or("collection_not_found")?;
        if self.collections[index].hibernating == hibernating {
            return Ok(false);
        }
        let revision = self.next_revision(self.standalone_count())?;
        self.collections[index].hibernating = hibernating;
        self.revision = revision;
        Ok(true)
    }

    fn standalone_count(&self) -> usize {
        self.family_assignments
            .iter()
            .filter(|assignment| matches!(assignment.family_id, FamilyId::Standalone { .. }))
            .count()
    }

    fn next_revision(&self, standalone_count: usize) -> Result<u64, &'static str> {
        self.next_revision_with_targets(standalone_count, self.mission_assignments.len())
    }

    fn next_revision_with_targets(
        &self,
        standalone_count: usize,
        target_count: usize,
    ) -> Result<u64, &'static str> {
        let reserve = u64::try_from(standalone_count + target_count)
            .map_err(|_| "organization_revision_exhausted")?;
        let revision = self
            .revision
            .checked_add(1)
            .ok_or("organization_revision_exhausted")?;
        if revision > u64::MAX - reserve {
            return Err("organization_revision_exhausted");
        }
        Ok(revision)
    }

    pub fn assign_family(
        &mut self,
        family: FamilyId,
        collection: CollectionId,
        live_families: &[FamilyId],
    ) -> Result<(), &'static str> {
        if !self.collections.iter().any(|item| item.id == collection) {
            return Err("collection_not_found");
        }
        if !live_families.contains(&family) {
            return Err("family_not_found");
        }
        let previous = self
            .family_assignments
            .iter()
            .position(|assignment| assignment.family_id == family);
        let count = self.standalone_count()
            + usize::from(previous.is_none() && matches!(family, FamilyId::Standalone { .. }));
        let revision = self.next_revision(count)?;
        if let Some(index) = previous {
            self.family_assignments[index].collection_id = collection;
        } else {
            self.family_assignments.push(FamilyAssignment {
                family_id: family,
                collection_id: collection,
            });
        }
        self.revision = revision;
        Ok(())
    }

    pub fn retain_live_standalone_families(
        &mut self,
        live: &[FamilyId],
    ) -> Result<Vec<FamilyId>, &'static str> {
        let removed = self
            .family_assignments
            .iter()
            .filter(|assignment| {
                matches!(assignment.family_id, FamilyId::Standalone { .. })
                    && !live.contains(&assignment.family_id)
            })
            .map(|assignment| assignment.family_id.clone())
            .collect::<Vec<_>>();
        if removed.is_empty() {
            return Ok(removed);
        }
        let revision = self.next_revision(self.standalone_count() - removed.len())?;
        self.family_assignments
            .retain(|assignment| !removed.contains(&assignment.family_id));
        self.revision = revision;
        Ok(removed)
    }
}
