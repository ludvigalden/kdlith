use std::process::ExitCode;

use crate::envelope::Envelope;
use crate::record::Severity;
use crate::registry;

/// The process exit strata: success, general failure, usage error. An
/// uncaught panic exits 101 through the runtime default and never
/// passes through this type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExitStatus {
    Success,
    Failure,
    Usage,
}

impl From<ExitStatus> for ExitCode {
    fn from(status: ExitStatus) -> Self {
        ExitCode::from(status.code())
    }
}

impl ExitStatus {
    /// The literal process exit code of this stratum.
    pub fn code(self) -> u8 {
        match self {
            ExitStatus::Success => 0,
            ExitStatus::Failure => 1,
            ExitStatus::Usage => 2,
        }
    }

    /// Resolve the exit status of a completed command from its
    /// envelope: an error diagnostic of exit class 2 decides usage
    /// failure; other errors, and warnings under `warnings_as_errors`,
    /// decide general failure; warnings and notes alone never fail. The
    /// coverage record never moves the exit by itself.
    pub fn resolve(envelope: &Envelope, warnings_as_errors: bool) -> Self {
        Self::from_diagnostics(
            envelope
                .diagnostics
                .iter()
                .map(|d| (d.code.as_str(), d.severity)),
            warnings_as_errors,
        )
    }

    /// [`ExitStatus::resolve`] over bare `(code, severity)` pairs, for
    /// commands whose diagnostics travel outside an envelope (exported
    /// data on stdout, diagnostics on stderr).
    pub fn from_diagnostics<'a>(
        diagnostics: impl IntoIterator<Item = (&'a str, Severity)>,
        warnings_as_errors: bool,
    ) -> Self {
        let mut failure = false;
        for (code, severity) in diagnostics {
            match severity {
                Severity::Error => match registry::exit_class(code, Severity::Error) {
                    2 => return Self::Usage,
                    _ => failure = true,
                },
                Severity::Warning if warnings_as_errors => failure = true,
                Severity::Warning | Severity::Note => {}
            }
        }
        if failure {
            Self::Failure
        } else {
            Self::Success
        }
    }
}
