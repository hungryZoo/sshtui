//! Pointer hover transitions driven by caller-supplied time.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crossterm::event::KeyCode;

use crate::app::{ClickTarget, FormField};

/// Identifies a hoverable element regardless of the cell within it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoverKey {
    /// Row index in the filtered host list.
    HostRow(usize),
    /// The row that opens a new connection form.
    NewRow,
    /// The entire field, including its label and input cells.
    FormField(FormField),
    /// Row index in the key picker.
    KeyRow(usize),
    /// A key hint or another region that dispatches the same key.
    Key(KeyCode),
    /// The update notice in the header.
    UpdateNotice,
}

impl ClickTarget {
    /// Returns the element's hover identity, if this target can be highlighted.
    pub fn hover_key(&self) -> Option<HoverKey> {
        Some(match *self {
            Self::HostRow(index) => HoverKey::HostRow(index),
            Self::NewRow => HoverKey::NewRow,
            Self::FormField { field, .. } => HoverKey::FormField(field),
            Self::KeyRow(index) => HoverKey::KeyRow(index),
            Self::Key(code) => HoverKey::Key(code),
            Self::UpdateNotice => HoverKey::UpdateNotice,
            Self::Inert | Self::Outside => return None,
        })
    }
}

#[derive(Debug)]
struct Fade {
    key: HoverKey,
    started: Instant,
    strength: f32,
}

impl Fade {
    fn entering(&self, now: Instant) -> f32 {
        let elapsed = now.saturating_duration_since(self.started);
        let remaining = 1.0 - elapsed.as_secs_f32() / Hover::DURATION.as_secs_f32();
        self.strength + (1.0 - self.strength) * (1.0 - remaining.max(0.0).powi(3))
    }

    fn leaving(&self, now: Instant) -> f32 {
        let elapsed = now.saturating_duration_since(self.started);
        let duration = Hover::DURATION.mul_f32(self.strength);
        if elapsed >= duration {
            return 0.0;
        }
        self.strength * (1.0 - elapsed.as_secs_f32() / duration.as_secs_f32())
    }
}

/// Tracks which element the pointer is over and the fades in progress. The caller supplies time.
#[derive(Debug, Default)]
pub struct Hover {
    current: Option<Fade>,
    leaving: VecDeque<Fade>,
}

impl Hover {
    /// Fade duration in each direction at full strength.
    pub const DURATION: Duration = Duration::from_millis(150);
    const MAX_TRACKED: usize = 16;

    /// Sets the element under the pointer and returns whether it changed.
    pub fn set(&mut self, key: Option<HoverKey>, now: Instant) -> bool {
        if self.current_key() == key {
            return false;
        }
        let mut strength = 0.0;
        self.leaving.retain(|fade| {
            let intensity = fade.leaving(now);
            if Some(fade.key) == key {
                strength = intensity;
                false
            } else {
                intensity > 0.0
            }
        });
        if let Some(previous) = self.current.take() {
            let strength = previous.entering(now);
            if strength > 0.0 {
                self.leaving.push_back(Fade {
                    key: previous.key,
                    started: now,
                    strength,
                });
            }
        }
        self.current = key.map(|key| Fade {
            key,
            started: now,
            strength,
        });
        if self.leaving.len() + usize::from(self.current.is_some()) > Self::MAX_TRACKED {
            self.leaving.pop_front();
        }
        true
    }

    /// Returns the element currently under the pointer.
    pub fn current_key(&self) -> Option<HoverKey> {
        self.current.as_ref().map(|fade| fade.key)
    }

    /// Returns the element's hover strength at `now`, from 0.0 to 1.0.
    pub fn intensity(&self, key: &HoverKey, now: Instant) -> f32 {
        if let Some(fade) = self.current.as_ref().filter(|fade| fade.key == *key) {
            return fade.entering(now);
        }
        self.leaving
            .iter()
            .find(|fade| fade.key == *key)
            .map_or(0.0, |fade| fade.leaving(now))
    }

    /// Returns whether any fade is still in progress at `now`.
    pub fn is_animating(&self, now: Instant) -> bool {
        self.current.as_ref().is_some_and(|fade| {
            fade.strength < 1.0 && now.saturating_duration_since(fade.started) < Self::DURATION
        }) || self.leaving.iter().any(|fade| fade.leaving(now) > 0.0)
    }

    /// Forgets all hover state when the pointer is known to be gone.
    pub fn clear(&mut self) {
        self.current = None;
        self.leaving.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: HoverKey = HoverKey::HostRow(0);
    const B: HoverKey = HoverKey::HostRow(1);

    #[test]
    fn entering_eases_out_over_the_full_duration() {
        let mut hover = Hover::default();
        let before = Instant::now();
        let start = before + Duration::from_millis(1);
        hover.set(Some(A), start);
        assert_eq!(hover.intensity(&A, before), 0.0);
        assert_eq!(hover.intensity(&A, start), 0.0);
        let middle = start + Hover::DURATION / 2;
        let strength = hover.intensity(&A, middle);
        assert!(strength > 0.5 && strength < 1.0);
        assert!(hover.is_animating(middle));
        let end = start + Hover::DURATION;
        assert_eq!(hover.intensity(&A, end), 1.0);
        assert!(!hover.is_animating(end));
    }

    #[test]
    fn leaving_at_full_strength_fades_out_over_the_full_duration() {
        let mut hover = Hover::default();
        let start = Instant::now();
        let leave = start + Hover::DURATION;
        hover.set(Some(A), start);
        hover.set(None, leave);
        assert_eq!(hover.intensity(&A, leave), 1.0);
        let middle = leave + Hover::DURATION / 2;
        let strength = hover.intensity(&A, middle);
        assert!(strength > 0.0 && strength < 1.0);
        assert!(hover.is_animating(middle));
        let end = leave + Hover::DURATION;
        assert_eq!(hover.intensity(&A, end), 0.0);
        assert!(!hover.is_animating(end));
    }

    #[test]
    fn leaving_at_partial_strength_shortens_the_fade_proportionally() {
        let mut hover = Hover::default();
        let start = Instant::now();
        let leave = start + Duration::from_millis(31);
        hover.set(Some(A), start);
        let strength = hover.intensity(&A, leave);
        assert!((strength - 0.5).abs() < 0.01);
        hover.set(None, leave);
        let end = leave + Hover::DURATION.mul_f32(strength);
        assert!(hover.intensity(&A, end - Duration::from_millis(1)) > 0.0);
        assert!(end < leave + Hover::DURATION);
        assert_eq!(hover.intensity(&A, end), 0.0);
        assert!(!hover.is_animating(end));
    }

    #[test]
    fn reentering_resumes_from_the_current_strength() {
        let mut hover = Hover::default();
        let start = Instant::now();
        let leave = start + Hover::DURATION;
        hover.set(Some(A), start);
        hover.set(None, leave);
        let reenter = leave + Hover::DURATION / 2;
        let strength = hover.intensity(&A, reenter);
        assert!(strength > 0.0 && strength < 1.0);
        hover.set(Some(A), reenter);
        assert!((hover.intensity(&A, reenter) - strength).abs() <= 0.05);
        assert!(hover.intensity(&A, reenter + Hover::DURATION / 2) > strength);
        assert_eq!(hover.intensity(&A, reenter + Hover::DURATION), 1.0);
    }

    #[test]
    fn moving_to_another_element_animates_both_elements() {
        let mut hover = Hover::default();
        let start = Instant::now();
        let moved = start + Hover::DURATION;
        hover.set(Some(A), start);
        hover.set(Some(B), moved);
        assert_eq!(hover.intensity(&A, moved), 1.0);
        assert_eq!(hover.intensity(&B, moved), 0.0);
        let middle = moved + Hover::DURATION / 2;
        assert!(hover.intensity(&A, middle) < hover.intensity(&A, moved));
        assert!(hover.intensity(&B, middle) > hover.intensity(&B, moved));
        assert!(hover.intensity(&A, middle) > 0.0);
        assert!(hover.intensity(&B, middle) < 1.0);
    }

    #[test]
    fn only_changes_to_the_hover_key_restart_the_transition() {
        let mut hover = Hover::default();
        let start = Instant::now();
        let middle = start + Hover::DURATION / 2;
        assert!(!hover.set(None, start));
        assert!(hover.set(Some(A), start));
        assert!(!hover.set(Some(A), middle));
        assert_eq!(hover.intensity(&A, start + Hover::DURATION), 1.0);
        assert!(hover.set(Some(B), start + Hover::DURATION));
        assert!(hover.set(None, start + Hover::DURATION));
        assert!(!hover.set(None, start + Hover::DURATION));
    }

    #[test]
    fn rapid_transitions_keep_at_most_sixteen_elements_and_drop_the_oldest() {
        let mut hover = Hover::default();
        let mut now = Instant::now();
        let mut step = Duration::from_millis(30);
        for index in 0..17 {
            hover.set(Some(HoverKey::HostRow(index)), now);
            assert_eq!(
                hover.leaving.len() + usize::from(hover.current.is_some()),
                (index + 1).min(16)
            );
            if index < 16 {
                // Shorter intervals keep all previous fades active until the cap is reached.
                now += step;
                step = step * 2 / 3;
            }
        }
        assert_eq!(hover.intensity(&A, now), 0.0);
        assert!(hover.intensity(&B, now) > 0.0);
        hover.set(Some(A), now + Hover::DURATION);
        assert_eq!(hover.leaving.len(), 1);
        assert_eq!(hover.leaving[0].key, HoverKey::HostRow(16));
    }

    #[test]
    fn form_fields_share_a_hover_key_across_columns_and_inert_regions_have_no_key() {
        for column in [0, 12, u16::MAX] {
            let field = FormField::Address;
            assert_eq!(
                ClickTarget::FormField { field, column }.hover_key(),
                Some(HoverKey::FormField(field))
            );
        }
        assert_eq!(ClickTarget::Inert.hover_key(), None);
        assert_eq!(ClickTarget::Outside.hover_key(), None);
    }

    #[test]
    fn clear_forgets_current_and_departing_elements() {
        let mut hover = Hover::default();
        let start = Instant::now();
        let moved = start + Hover::DURATION;
        hover.set(Some(A), start);
        hover.set(Some(B), moved);
        assert!(hover.is_animating(moved));
        hover.clear();
        assert_eq!(hover.current_key(), None);
        assert_eq!(hover.intensity(&A, moved), 0.0);
        assert!(!hover.is_animating(moved));
    }
}
