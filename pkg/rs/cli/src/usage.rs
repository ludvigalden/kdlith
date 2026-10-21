//! The argument-error stratum: every command-line rejection exits 2
//! under its registered code, with clap's statement of the fault kept
//! as the message in the `error[<CODE>]:` line shape.

use std::error::Error as StdError;
use std::fmt;

use clap::error::{Error as ClapError, ErrorKind};
use kdlith_diagnostic::ExitStatus;
use kdlith_diagnostic::registry::{lookup, render};

/// A rejected argument or option value, or any other argument-form
/// fault: a missing required argument or subcommand, an unexpected
/// argument, an unrecognized subcommand.
const INVALID_ARGUMENT: &str = "KDLITH-CLI001";
/// Two options the grammar declares mutually exclusive, both given.
const CONFLICTING_OPTIONS: &str = "KDLITH-CLI002";
/// `export --format` names an unsupported encoding.
const UNKNOWN_FORMAT: &str = "KDLITH-CLI003";
/// `inspect --view` names a view the inspected target does not support.
const INAPPLICABLE_VIEW: &str = "KDLITH-CLI004";

/// The fault a format option's value parser reports: the named format
/// is not one of the supported ones. It renders the registered
/// KDLITH-CLI003 template, and its type is how the stratum tells this
/// rejection from every other value rejection.
#[derive(Debug)]
pub(crate) struct UnknownFormat {
    format: String,
    supported: &'static str,
}

impl UnknownFormat {
    pub(crate) fn new(format: &str, supported: &'static str) -> Self {
        Self {
            format: format.to_owned(),
            supported,
        }
    }
}

impl fmt::Display for UnknownFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let template = lookup(UNKNOWN_FORMAT).map_or(
            "unknown format <format>; expected one of <values>",
            |definition| definition.message_template,
        );
        f.write_str(&render(template, &[&self.format, self.supported]))
    }
}

impl StdError for UnknownFormat {}

/// Report a clap parse outcome and decide the exit. `--help` and
/// `--version` print to stdout and exit 0; every rejection goes to
/// stderr under its registered code and exits 2.
pub(crate) fn report_parse_error(error: &ClapError) -> ExitStatus {
    if error.exit_code() == 0 {
        let _ = error.print();
        return ExitStatus::Success;
    }
    let rendered = error.render().to_string();
    let code = code_of(error);
    let text = match rendered.strip_prefix("error: ") {
        Some(statement) => format!("error[{code}]: {statement}"),
        // clap printed help because the command line named no
        // subcommand; the fault line follows the help text.
        None => format!(
            "{}\nerror[{code}]: a subcommand is required but one was not provided\n",
            rendered.trim_end()
        ),
    };
    eprint!("{text}");
    ExitStatus::Usage
}

/// Report an argument-form fault the parser cannot see, such as a
/// `help` path naming no command.
pub(crate) fn report_invalid_argument(statement: &str) -> ExitStatus {
    eprintln!("error[{INVALID_ARGUMENT}]: {statement}");
    ExitStatus::Usage
}

/// Report a view and target the parser accepts one by one but the
/// inspect view matrix rejects together.
pub(crate) fn report_inapplicable_view(view: &str, target: &str) -> ExitStatus {
    let template = lookup(INAPPLICABLE_VIEW).map_or(
        "the <view> view does not apply to a <target> target",
        |definition| definition.message_template,
    );
    eprintln!(
        "error[{INAPPLICABLE_VIEW}]: {}",
        render(template, &[view, target])
    );
    ExitStatus::Usage
}

fn code_of(error: &ClapError) -> &'static str {
    if error.kind() == ErrorKind::ArgumentConflict {
        return CONFLICTING_OPTIONS;
    }
    let unknown_format = error
        .source()
        .is_some_and(|source| source.downcast_ref::<UnknownFormat>().is_some());
    if unknown_format {
        UNKNOWN_FORMAT
    } else {
        INVALID_ARGUMENT
    }
}
