//! Console flow: status of the local backup history

use super::{Strings, select};
use crate::error::Result;
use crate::service::{self, Exit};
use std::path::Path;

/// Menu index -> (all, detailed) flags of `service::status::status`
fn status_view(index: usize) -> (bool, bool) {
    match index {
        0 => (false, false),
        1 => (false, true),
        _ => (true, true),
    }
}

/// Status flow: delegates to the shared `service::status` orchestration
pub(crate) async fn run(s: &'static Strings, _config_path: &Path) -> Result<Exit> {
    let Some(index) = select(s.status_prompt, &s.status_options, 0).await? else {
        return Ok(Exit::Success); // Esc / Ctrl-C: back to the menu
    };
    let (all, detailed) = status_view(index);
    Ok(service::status::status(all, detailed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_view_mapping() {
        assert_eq!(status_view(0), (false, false));
        assert_eq!(status_view(1), (false, true));
        assert_eq!(status_view(2), (true, true));
        assert_eq!(status_view(99), (true, true));
    }
}
