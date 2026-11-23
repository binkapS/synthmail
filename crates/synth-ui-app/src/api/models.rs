// Typed DTOs for responses we consume (serde structs).
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct LoginResp {
    pub token: Option<String>,
    pub kind: Option<String>, // "user" | "admin" | "superadmin"
    #[allow(dead_code)] // read in later phases (e.g. account screen)
    pub email: Option<String>,
    #[allow(dead_code)]
    pub force_password_change: bool,
}

impl LoginResp {
    pub fn is_admin(&self) -> bool {
        matches!(self.kind.as_deref(), Some("admin" | "superadmin"))
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct MeResp {
    #[allow(dead_code)] // read in later phases (e.g. account screen)
    pub email: Option<String>,
    pub kind: Option<String>,
    #[allow(dead_code)]
    pub force_password_change: bool,
}
