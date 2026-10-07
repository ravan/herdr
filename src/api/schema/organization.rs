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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MissionAssignPaneParams {
    pub pane_id: String,
    pub mission_id: crate::organization::MissionId,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MissionClearPaneOverrideParams {
    pub pane_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CollectionRenameParams {
    pub collection_id: crate::organization::CollectionId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MissionRenameParams {
    pub mission_id: crate::organization::MissionId,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CollectionMoveParams {
    pub collection_id: crate::organization::CollectionId,
    pub to_index: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MissionMoveParams {
    pub mission_id: crate::organization::MissionId,
    pub to_index: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MissionSetObjectiveParams {
    pub mission_id: crate::organization::MissionId,
    #[serde(deserialize_with = "required_objective")]
    #[schemars(required, schema_with = "nullable_objective_schema")]
    pub objective: Option<String>,
}

// A nullable edit must contain the key: omission cannot silently clear an objective.
fn required_objective<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    Option::<String>::deserialize(deserializer)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CollectionDeleteParams {
    pub collection_id: crate::organization::CollectionId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CollectionUnassignFamilyParams {
    pub family_id: crate::organization::FamilyId,
    pub collection_id: crate::organization::CollectionId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MissionUnassignParams {
    pub target: crate::organization::MissionTarget,
    pub mission_id: crate::organization::MissionId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct MissionDeleteParams {
    pub mission_id: crate::organization::MissionId,
}

// `required` alone removes Option's null type; keep null as the explicit clear operation.
fn nullable_objective_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({"type":["string","null"]})
}
