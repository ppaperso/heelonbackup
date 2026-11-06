//! Console flow stub (implemented in its own phase 3 commit)

use super::Strings;
use crate::error::Result;
use crate::service::Exit;
use std::path::Path;

/// Stub: no business logic yet, back to the menu
pub(crate) async fn run(s: &'static Strings, _config_path: &Path) -> Result<Exit> {
    println!("{}", s.stub);
    Ok(Exit::Success)
}
