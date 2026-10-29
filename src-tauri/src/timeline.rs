// A clip that still overruns its slot after the line is rewritten may be sped up only this far.
// Beyond that, the tail is cut. Short clips stay at natural speed.
pub(crate) const MAX_SPEED: f64 = 1.08;
pub(crate) const TOLERANCE: f64 = 1.5;
const MIN_ROOM: f64 = 0.05;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Slot {
    pub start: f64,
    pub end: f64,
    pub wav_secs: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Placement {
    pub start: f64,
    pub speed: f64,
    pub keep: Option<f64>,
    pub played: f64,
}

fn limit_end(slots: &[Slot], index: usize, total_secs: f64) -> f64 {
    let slot = slots[index];
    match slots.get(index + 1) {
        Some(next) => (slot.end + TOLERANCE).min(next.start.max(slot.end)),
        None => total_secs.max(slot.end),
    }
}

pub(crate) fn room(slots: &[Slot], index: usize, total_secs: f64) -> f64 {
    (limit_end(slots, index, total_secs) - slots[index].start.max(0.0)).max(MIN_ROOM)
}

/// Rewrite only when the existing gentle speed adjustment cannot fit the line.
pub(crate) fn too_long(wav_secs: f64, room_secs: f64) -> bool {
    wav_secs.is_finite() && wav_secs > room_secs * MAX_SPEED
}

pub(crate) fn place_all(slots: &[Slot], total_secs: f64) -> Vec<Placement> {
    let mut cursor = 0.0_f64;
    let mut placed = Vec::with_capacity(slots.len());
    for (index, slot) in slots.iter().enumerate() {
        let start = slot.start.max(cursor).max(0.0);
        let wav = if slot.wav_secs.is_finite() {
            slot.wav_secs.max(0.0)
        } else {
            0.0
        };
        let available = (limit_end(slots, index, total_secs) - start).max(MIN_ROOM);
        let speed = if wav <= available {
            1.0
        } else {
            (wav / available).min(MAX_SPEED)
        };
        let mut played = wav / speed;
        let mut keep = None;
        if played > available + 0.02 {
            keep = Some(available);
            played = available;
        }
        cursor = start + played;
        placed.push(Placement {
            start,
            speed,
            keep,
            played,
        });
    }
    placed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(start: f64, end: f64, wav_secs: f64) -> Slot {
        Slot {
            start,
            end,
            wav_secs,
        }
    }

    #[test]
    fn short_clip_keeps_speed_and_start() {
        let placed = place_all(&[slot(0.0, 2.0, 1.95)], 10.0);
        assert_eq!(placed[0].start, 0.0);
        assert_eq!(placed[0].speed, 1.0);
        assert_eq!(placed[0].keep, None);
    }

    #[test]
    fn short_clip_in_a_long_slot_keeps_natural_speed() {
        let placed = place_all(&[slot(0.0, 8.0, 2.0), slot(8.0, 10.0, 1.9)], 10.0);
        assert_eq!(placed[0].speed, 1.0);
        assert!((placed[0].played - 2.0).abs() < 1e-9);
        let near = place_all(&[slot(0.0, 2.0, 1.6)], 10.0);
        assert_eq!(near[0].speed, 1.0);
        assert!((near[0].played - 1.6).abs() < 1e-9);
    }

    #[test]
    fn small_overrun_speeds_up_only_up_to_the_cap() {
        let placed = place_all(&[slot(0.0, 2.0, 2.1), slot(2.0, 4.0, 1.0)], 10.0);
        assert!((placed[0].speed - 1.05).abs() < 1e-9);
        assert!(placed[0].keep.is_none());
        let over = place_all(&[slot(0.0, 2.0, 3.0), slot(2.0, 4.0, 1.0)], 10.0);
        assert_eq!(over[0].speed, MAX_SPEED);
        assert_eq!(over[0].keep, Some(2.0));
    }

    #[test]
    fn clip_borrows_the_gap_before_speeding_up() {
        let placed = place_all(&[slot(0.0, 2.0, 2.6), slot(4.0, 6.0, 1.0)], 10.0);
        assert_eq!(placed[0].speed, 1.0);
        assert_eq!(placed[0].keep, None);
        assert_eq!(placed[1].start, 4.0);
    }

    #[test]
    fn clip_that_cannot_fit_is_sped_up_then_cut() {
        let placed = place_all(&[slot(0.0, 1.0, 3.0), slot(1.0, 5.0, 1.0)], 10.0);
        assert_eq!(placed[0].speed, MAX_SPEED);
        assert_eq!(placed[0].keep, Some(1.0));
        assert_eq!(placed[0].played, 1.0);
        assert_eq!(placed[1].start, 1.0);
    }

    #[test]
    fn clips_never_overlap() {
        let slots: Vec<Slot> = (0..20)
            .map(|i| slot(i as f64, i as f64 + 1.0, 1.0 + (i % 4) as f64))
            .collect();
        let placed = place_all(&slots, 30.0);
        for pair in placed.windows(2) {
            assert!(pair[1].start + 1e-9 >= pair[0].start + pair[0].played);
        }
    }

    #[test]
    fn room_counts_gap_up_to_tolerance() {
        let slots = [slot(0.0, 2.0, 1.0), slot(4.0, 6.0, 1.0)];
        assert!((room(&slots, 0, 10.0) - 3.5).abs() < 1e-9);
        assert!((room(&slots, 1, 10.0) - 6.0).abs() < 1e-9);
    }

    #[test]
    fn too_long_means_gentle_speed_adjustment_cannot_fit() {
        assert!(too_long(1.2, 1.0));
        assert!(!too_long(1.0, 1.0));
        assert!(!too_long(0.95, 1.0));
        assert!(!too_long(1.05, 1.0));
        assert!(!too_long(1.08, 1.0));
        assert!(!too_long(f64::NAN, 1.0));
    }
}
