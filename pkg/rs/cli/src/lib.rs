//! Commands, arguments, output conventions, and orchestration.
mod read;
mod report;
mod vet;

pub use read::read;
pub use vet::{VET_USAGE, VetTarget, vet, vet_command};
