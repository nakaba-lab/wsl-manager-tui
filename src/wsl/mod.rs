//! WSL backend: a `wsl.exe` wrapper abstracted behind a trait, UTF-16LE output
//! decoding, output parsing, and locale-independent state detection. No UI.

pub mod backend;
pub mod collect;
pub mod decode;
pub mod model;
pub mod parse;

pub use backend::{RealWslBackend, WslBackend};
pub use collect::{collect_distros, refresh};
pub use decode::{decode_utf8, decode_wsl_output};
pub use model::{Distro, DistroState};
pub use parse::{parse_list_verbose, RawDistroRow};
