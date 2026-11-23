//! State for the update confirmation overlay.

use crate::i18n::Lang;
use crate::update::{InstallMethod, UpdatePlan, latest_url};

/// An available update awaiting confirmation.
#[derive(Debug)]
pub struct UpdateState {
    pub(super) plan: UpdatePlan,
    lang: Lang,
}

impl UpdateState {
    pub(super) fn new(plan: UpdatePlan, lang: Lang) -> Self {
        Self { plan, lang }
    }

    /// Returns the language of the update overlay.
    pub(crate) fn lang(&self) -> Lang {
        self.lang
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
        self.lang.install_method(&self.plan.method)
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
