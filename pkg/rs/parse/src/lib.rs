//! Streaming KDL v2 parser (pull events, coordinates, fail-closed
//! limits) and writer. The parser derives from just-kdl and kdlite
//! (MIT OR Apache-2.0); attribution in src/NOTICE.md.
mod chars;
mod coords;
mod error;
mod event;
mod limits;
mod number;
mod parse;
mod scan;
mod write;

pub use coords::{Locator, locate};
pub use error::{Construct, ErrorKind, Limit, ParseError};
pub use event::{Entry, Event, Number, NumberKind, Radix, Spanned, Value};
pub use limits::Limits;
pub use parse::Parser;
pub use write::{
    WriteDocument, WriteNode, WriteValue, is_bare_string, write_name, write_string, write_value,
};
