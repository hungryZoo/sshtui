//! State for the update confirmation overlay.

use crate::update::{InstallMethod, UpdatePlan, latest_url};

/// An available update awaiting confirmation.
#[derive(Debug)]
pub struct UpdateState {
    pub(super) plan: UpdatePlan,
}

impl UpdateState {
    pub(super) fn new(plan: UpdatePlan) -> Self {
        Self { plan }
    }

    /// Returns the running version.
    pub fn current(&self) -> &str {
        &self.plan.current
    }

    /// Returns the available release version.
    pub fn latest(&self) -> &str {
        &self.plan.latest
    }

    /// Returns the label of the detected installation method.
    pub fn method_description(&self) -> String {
        self.plan.method.describe()
    }

    /// Returns the upgrade commands formatted for display.
    pub fn display_steps(&self) -> Vec<String> {
        self.plan.method.display_steps(self.latest())
    }

    /// Returns whether the installation method supports an upgrade command.
    pub fn can_run(&self) -> bool {
        self.plan.method != InstallMethod::Unknown
    }

    /// Returns the release page for a manual update.
    pub fn release_url(&self) -> String {
        latest_url()
    }
}
