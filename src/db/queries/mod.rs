//! Database query functions, organized by domain.
//!
//! Submodules are flat-re-exported so callers use `db::queries::<fn>` regardless
//! of which domain a function lives in.

mod channels;
mod friends;
mod messaging;
mod settings;

pub use channels::*;
pub use friends::*;
pub use messaging::*;
pub use settings::*;
