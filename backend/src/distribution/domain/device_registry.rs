//! Device identity and lifecycle, independent of any sending protocol.
use crate::business::{Error, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
#[derive(Clone, Debug)]
pub enum Principal {
    Actor(Uuid),
    Device { id: Uuid, generation: i64 },
    Enrollment,
}
impl Principal {
    pub fn actor(&self) -> Result<Uuid> {
        if let Self::Actor(id) = self {
            Ok(*id)
        } else {
            Err(Error::Unauthorized)
        }
    }
    pub fn key(&self) -> String {
        match self {
            Self::Actor(id) => format!("actor:{id}"),
            Self::Device { id, generation } => format!("device:{id}:{generation}"),
            Self::Enrollment => "enrollment".into(),
        }
    }
}
#[derive(Clone, Debug)]
pub struct Scope {
    pub school: Option<Uuid>,
    pub id: Option<Uuid>,
}
#[derive(Clone, Copy)]
pub enum Query {
    Devices,
    Device,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    CreateDevice {
        name: String,
    },
    UpdateDevice {
        expected_revision: i64,
        enabled: bool,
        reason: String,
    },
    Enrollment {
        expected_revision: i64,
    },
    Revoke {
        expected_revision: i64,
        reason: String,
    },
    Enroll {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        device_id: Option<Uuid>,
        code: String,
        client_instance_id: String,
        protocol_version: u32,
    },
    Rotate {},
}
