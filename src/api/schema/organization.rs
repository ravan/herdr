use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CollectionCreateParams {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CollectionAssignFamilyParams {
    pub family_id: crate::organization::FamilyId,
    pub collection_id: crate::organization::CollectionId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CollectionSetHibernatingParams {
    pub collection_id: crate::organization::CollectionId,
    pub hibernating: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MissionCreateParams {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub objective: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MissionAssignParams {
    pub target: crate::organization::MissionTarget,
    pub mission_id: crate::organization::MissionId,
}
