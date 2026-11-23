// Typed DTOs for responses we consume (serde structs).
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct LoginResp {
    pub token: Option<String>,
    pub kind: Option<String>, // "user" | "admin" | "superadmin"
    pub email: Option<String>,
    #[serde(default)]
    pub force_password_change: bool,
}

impl LoginResp {
    pub fn is_admin(&self) -> bool {
        matches!(self.kind.as_deref(), Some("admin" | "superadmin"))
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct MeResp {
    pub email: Option<String>,
    pub kind: Option<String>,
    #[serde(default)]
    pub force_password_change: bool,
}
