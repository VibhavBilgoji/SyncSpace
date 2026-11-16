//! Workspaces and their members (`docs/API.md` section 2). Who may do what is decided in
//! [`crate::rbac`]; every change to membership is audited through [`crate::audit`].

pub mod handlers;
pub mod validation;
