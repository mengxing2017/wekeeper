//! Forward-only retirement: settlement and governance remain available, never new UI work.
use super::*;

pub const LEGACY_MAINTENANCE_RETIRED: &str = "legacy_mobile_maintenance_retired";
impl Command {
    /// Called after principal/scope authorization, before idempotency replay or writes.
    pub fn require_not_retired(&self) -> Result<()> {
        let retired = match self {
            Self::Dispatch { input } => match input.as_ref() {
                DispatchCommand::CancelValidation { .. }
                | DispatchCommand::ValidationEvent { .. }
                | DispatchCommand::RevokeFlow { .. }
                | DispatchCommand::SetEnabled { enabled: false, .. }
                | DispatchCommand::Failure { .. }
                | DispatchCommand::Diagnostic { .. }
                | DispatchCommand::Heartbeat { .. }
                | DispatchCommand::Authorization { .. } => false,
                DispatchCommand::ControlBatch { command, .. } => {
                    !matches!(command.as_str(), "stop" | "pause")
                }
                _ => true,
            },
            Self::Automation { input } => !matches!(
                input.as_ref(),
                AutomationCommand::RecordAuthorization { .. }
                    | AutomationCommand::RejectValidation { .. }
                    | AutomationCommand::CancelValidation { .. }
                    | AutomationCommand::ReportValidation { .. }
                    | AutomationCommand::RevokeRelease { .. }
            ),
            Self::CreateConfiguration { .. }
            | Self::PublishConfiguration { .. }
            | Self::PreviewRunRevision { .. }
            | Self::ReviseRun { .. }
            | Self::CreateStrategy { .. }
            | Self::UpdateStrategy { .. }
            | Self::RunStrategy { .. }
            | Self::Draft { .. }
            | Self::Preview { .. }
            | Self::Confirm { .. }
            | Self::Resend { .. }
            | Self::Claim { .. }
            | Self::Renew { .. }
            | Self::Prepare { .. }
            | Self::Permit { .. } => true,
            Self::Settings { enabled, .. } => *enabled,
            _ => false,
        };
        if retired {
            return Err(Error::Policy(LEGACY_MAINTENANCE_RETIRED));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn dispatch(input: DispatchCommand) -> Command {
        Command::Dispatch {
            input: Box::new(input),
        }
    }
    #[test]
    fn retired_writes_cannot_reactivate_but_settlement_survives() {
        for command in [
            dispatch(DispatchCommand::SetEnabled {
                enabled: true,
                reviewed_history: true,
            }),
            dispatch(DispatchCommand::AcceptValidation {
                expected_revision: 1,
                generation: Uuid::nil(),
                authorization_id: Uuid::nil(),
            }),
            dispatch(DispatchCommand::CreatePacing {
                name: "old".into(),
                school_id: Uuid::nil(),
                validation_ids: vec![],
            }),
            Command::Claim {
                binding_revision: 1,
            },
            Command::Permit {
                fence: 1,
                snapshot_digest: "x".into(),
                preparation_id: Uuid::nil(),
            },
        ] {
            assert!(matches!(
                command.require_not_retired(),
                Err(Error::Policy(LEGACY_MAINTENANCE_RETIRED))
            ));
        }
        for command in [
            dispatch(DispatchCommand::CancelValidation {
                expected_revision: 1,
            }),
            dispatch(DispatchCommand::SetEnabled {
                enabled: false,
                reviewed_history: false,
            }),
            dispatch(DispatchCommand::ControlBatch {
                expected_revision: 1,
                command: "stop".into(),
            }),
            Command::Reconcile {
                expected_revision: 1,
                decision: "verified_not_sent".into(),
                evidence_ref: "fixed".into(),
                reason: "review".into(),
            },
        ] {
            command.require_not_retired().unwrap();
        }
        assert!(
            dispatch(DispatchCommand::ControlBatch {
                expected_revision: 1,
                command: "resume".into()
            })
            .require_not_retired()
            .is_err()
        );
    }
}
