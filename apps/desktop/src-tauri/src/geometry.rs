//! Window geometry rules.
//!
//! Kept as pure functions with unit tests because multi-monitor placement is
//! exactly the kind of thing that silently breaks when a display is unplugged
//! or a scale factor changes.

use serde::{Deserialize, Serialize};

/// Physical pixels, top-left origin (Quartz / Tauri convention).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Px {
    pub x: i32,
    pub y: i32,
}

impl Px {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn contains(&self, p: Px) -> bool {
        p.x >= self.x && p.x < self.x + self.w && p.y >= self.y && p.y < self.y + self.h
    }

    /// Area of overlap with another rect (0 when disjoint).
    pub fn overlap_area(&self, other: &Rect) -> i64 {
        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = (self.x + self.w).min(other.x + other.w);
        let y2 = (self.y + self.h).min(other.y + other.h);
        if x2 <= x1 || y2 <= y1 {
            0
        } else {
            (x2 - x1) as i64 * (y2 - y1) as i64
        }
    }
}

impl From<Px> for Rect {
    fn from(p: Px) -> Self {
        Rect {
            x: p.x,
            y: p.y,
            w: 1,
            h: 1,
        }
    }
}

/// Restore policy result, so callers can report *why* a position was changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum RestoreOutcome {
    /// The saved position is usable as-is.
    Kept,
    /// The saved position was off-screen; moved to the primary monitor centre.
    RecoveredOffscreen,
    /// The saved position was on-screen but partly clipped; nudged back in.
    Clamped,
    /// No saved position existed.
    Defaulted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub pos: Px,
    pub outcome: RestoreOutcome,
}

/// Decide where the light point should appear.
///
/// Rules:
/// 1. A saved position counts as usable when the window's centre is on some
///    monitor **and** at least half of it is on-screen.
/// 2. Otherwise fall back to the primary monitor's centre — never to (0,0),
///    which is usually behind the menu bar.
pub fn place(
    saved: Option<Px>,
    size: (i32, i32),
    monitors: &[Rect],
    primary: Option<Rect>,
) -> Placement {
    let Some(primary) = primary else {
        // No monitor info at all: keep the saved value, or a safe default.
        return Placement {
            pos: saved.unwrap_or(Px::new(100, 100)),
            outcome: if saved.is_some() {
                RestoreOutcome::Kept
            } else {
                RestoreOutcome::Defaulted
            },
        };
    };

    let centre = |p: Px| Px::new(p.x + size.0 / 2, p.y + size.1 / 2);

    let Some(saved) = saved else {
        return Placement {
            pos: centre_of(primary, size),
            outcome: RestoreOutcome::Defaulted,
        };
    };

    let win = Rect {
        x: saved.x,
        y: saved.y,
        w: size.0,
        h: size.1,
    };

    // Rule 1: the window must be meaningfully on *some* monitor.
    let best = monitors
        .iter()
        .map(|m| (m, m.overlap_area(&win)))
        .max_by_key(|(_, area)| *area);

    let centre_pos = centre(saved);
    let centre_on_screen = monitors.iter().any(|m| m.contains(centre_pos));
    let half_visible = best.map(|(_, a)| a * 2 >= (size.0 as i64 * size.1 as i64)).unwrap_or(false);

    if !centre_on_screen || !half_visible {
        return Placement {
            pos: centre_of(primary, size),
            outcome: RestoreOutcome::RecoveredOffscreen,
        };
    }

    // Rule 2: nudge fully inside the monitor it overlaps most.
    let Some((monitor, _)) = best else {
        return Placement {
            pos: centre_of(primary, size),
            outcome: RestoreOutcome::RecoveredOffscreen,
        };
    };

    let mut pos = saved;
    if pos.x < monitor.x {
        pos.x = monitor.x;
    }
    if pos.y < monitor.y {
        pos.y = monitor.y;
    }
    if pos.x + size.0 > monitor.x + monitor.w {
        pos.x = monitor.x + monitor.w - size.0;
    }
    if pos.y + size.1 > monitor.y + monitor.h {
        pos.y = monitor.y + monitor.h - size.1;
    }

    Placement {
        pos,
        outcome: if pos == saved {
            RestoreOutcome::Kept
        } else {
            RestoreOutcome::Clamped
        },
    }
}

pub fn centre_of(monitor: Rect, size: (i32, i32)) -> Px {
    Px::new(
        monitor.x + (monitor.w - size.0) / 2,
        monitor.y + (monitor.h - size.1) / 2,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: (i32, i32) = (132, 132);

    fn primary() -> Rect {
        Rect {
            x: 0,
            y: 0,
            w: 3024,
            h: 1964,
        }
    }

    fn secondary_left() -> Rect {
        Rect {
            x: -1920,
            y: 0,
            w: 1920,
            h: 1080,
        }
    }

    #[test]
    fn no_saved_position_centres_on_primary() {
        let p = place(None, SIZE, &[primary()], Some(primary()));
        assert_eq!(p.outcome, RestoreOutcome::Defaulted);
        assert_eq!(p.pos, Px::new((3024 - 132) / 2, (1964 - 132) / 2));
        // and definitely not (0,0), which is under the menu bar
        assert_ne!(p.pos, Px::new(0, 0));
    }

    #[test]
    fn on_screen_position_is_kept_verbatim() {
        let saved = Px::new(900, 700);
        let p = place(Some(saved), SIZE, &[primary()], Some(primary()));
        assert_eq!(p.outcome, RestoreOutcome::Kept);
        assert_eq!(p.pos, saved);
    }

    #[test]
    fn off_screen_position_is_recovered() {
        // Display unplugged: the saved spot is far outside every monitor.
        let saved = Px::new(9000, 9000);
        let p = place(Some(saved), SIZE, &[primary()], Some(primary()));
        assert_eq!(p.outcome, RestoreOutcome::RecoveredOffscreen);
        assert!(primary().contains(Px::new(p.pos.x + SIZE.0 / 2, p.pos.y + SIZE.1 / 2)));
    }

    #[test]
    fn mostly_offscreen_is_recovered() {
        // Only a sliver visible: treat as lost rather than leaving it unreachable.
        let saved = Px::new(3024 - 8, 500);
        let p = place(Some(saved), SIZE, &[primary()], Some(primary()));
        assert_eq!(p.outcome, RestoreOutcome::RecoveredOffscreen);
    }

    #[test]
    fn slightly_clipped_is_clamped_not_discarded() {
        let saved = Px::new(3024 - 100, 500); // 32px hanging off the right edge
        let p = place(Some(saved), SIZE, &[primary()], Some(primary()));
        assert_eq!(p.outcome, RestoreOutcome::Clamped);
        assert_eq!(p.pos.x, 3024 - 132);
        assert_eq!(p.pos.y, 500, "the other axis is untouched");
    }

    #[test]
    fn negative_origin_secondary_display_works() {
        let saved = Px::new(-1800, 200); // on the left-hand display
        let p = place(
            Some(saved),
            SIZE,
            &[primary(), secondary_left()],
            Some(primary()),
        );
        assert_eq!(p.outcome, RestoreOutcome::Kept);
        assert_eq!(p.pos, saved);
    }

    #[test]
    fn removed_secondary_display_falls_back_to_primary() {
        let saved = Px::new(-1500, 300); // was on the now-unplugged display
        let p = place(Some(saved), SIZE, &[primary()], Some(primary()));
        assert_eq!(p.outcome, RestoreOutcome::RecoveredOffscreen);
    }

    #[test]
    fn no_monitor_information_keeps_saved_value() {
        let p = place(Some(Px::new(10, 20)), SIZE, &[], None);
        assert_eq!(p.outcome, RestoreOutcome::Kept);
        assert_eq!(p.pos, Px::new(10, 20));
    }

    #[test]
    fn clamp_uses_the_display_it_mostly_lives_on() {
        // Straddling two displays: clamp into the one with more overlap.
        let left = Rect { x: -800, y: 0, w: 800, h: 600 };
        let right = Rect { x: 0, y: 0, w: 1200, h: 600 };
        let saved = Px::new(60, 100); // 72px on the right, 60px on the left
        let p = place(Some(saved), SIZE, &[left, right], Some(right));
        assert_eq!(p.outcome, RestoreOutcome::Kept);
    }

    #[test]
    fn overlap_area_math() {
        let a = Rect { x: 0, y: 0, w: 10, h: 10 };
        let b = Rect { x: 5, y: 5, w: 10, h: 10 };
        assert_eq!(a.overlap_area(&b), 25);
        assert_eq!(a.overlap_area(&Rect { x: 20, y: 20, w: 5, h: 5 }), 0);
    }
}
