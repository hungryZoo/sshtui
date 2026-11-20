//! The Windows common item dialog.

use std::path::Path;

use super::command::{PICKER_TITLE, existing_start_dir};
use super::{DialogError, FilePicker, PickOutcome};

/// Picker backed by the Windows common item dialog through `rfd`.
#[derive(Debug, Clone)]
pub struct RfdPicker {
    unavailable: Option<String>,
}

impl RfdPicker {
    /// Creates a picker with any session availability restriction.
    pub fn new(unavailable: Option<String>) -> Self {
        Self { unavailable }
    }
}

impl FilePicker for RfdPicker {
    fn unavailable_reason(&self) -> Option<String> {
        self.unavailable.clone()
    }

    fn hidden_files_hint(&self) -> Option<String> {
        None
    }

    fn pick_file(&self, start_dir: &Path) -> Result<PickOutcome, DialogError> {
        if let Some(reason) = self.unavailable_reason() {
            return Err(DialogError::Unavailable(reason));
        }

        let path = rfd::FileDialog::new()
            .set_title(PICKER_TITLE)
            .set_directory(existing_start_dir(start_dir))
            .set_show_hidden_files(true)
            .pick_file();
        Ok(path.map_or(PickOutcome::Cancelled, PickOutcome::Picked))
    }
}
