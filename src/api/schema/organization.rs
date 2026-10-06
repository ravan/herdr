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
