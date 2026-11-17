//! Where a moved task lands in its column (`docs/API.md`, "Move rule").
//!
//! The position is a float between the neighbours: the midpoint of the two, half of the first at
//! the head, the last plus 1.0 at the tail and 1.0 in an empty column. When two neighbours have
//! come closer than [`MIN_GAP`] there is no room left and the column is renumbered first.

/// Below this distance between neighbours the column is renumbered (`docs/API.md`).
pub const MIN_GAP: f64 = 1e-9;

/// The `order` for a task that goes after `before` and before `after` (either may be missing:
/// head, tail or an empty column). `None` means "no room: renumber the column and ask again".
pub fn position_between(before: Option<f64>, after: Option<f64>) -> Option<f64> {
    match (before, after) {
        (None, None) => Some(1.0),
        (Some(last), None) => Some(last + 1.0),
        (None, Some(first)) => {
            if first < 2.0 * MIN_GAP {
                None
            } else {
                Some(first / 2.0)
            }
        }
        (Some(before), Some(after)) => {
            if after - before < MIN_GAP {
                None
            } else {
                Some(before + (after - before) / 2.0)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_column_starts_at_one() {
        assert_eq!(position_between(None, None), Some(1.0));
    }

    #[test]
    fn the_tail_is_the_last_plus_one() {
        assert_eq!(position_between(Some(4.0), None), Some(5.0));
        assert_eq!(position_between(Some(0.25), None), Some(1.25));
    }

    #[test]
    fn the_head_is_half_of_the_first() {
        assert_eq!(position_between(None, Some(1.0)), Some(0.5));
        assert_eq!(position_between(None, Some(0.5)), Some(0.25));
    }

    #[test]
    fn between_two_is_their_midpoint() {
        assert_eq!(position_between(Some(1.0), Some(2.0)), Some(1.5));
        assert_eq!(position_between(Some(1.0), Some(1.5)), Some(1.25));
    }

    #[test]
    fn no_room_asks_for_a_renumber() {
        // Neighbours closer than 1e-9, and equal neighbours.
        assert_eq!(position_between(Some(1.0), Some(1.0 + 5e-10)), None);
        assert_eq!(position_between(Some(1.0), Some(1.0)), None);
        // A head too close to zero.
        assert_eq!(position_between(None, Some(1e-9)), None);
        // Just enough room is still room.
        assert!(position_between(Some(1.0), Some(1.0 + 2e-9)).is_some());
    }

    #[test]
    fn repeated_inserts_at_the_head_eventually_ask_for_a_renumber() {
        let mut first = 1.0;
        let mut steps = 0;
        while let Some(order) = position_between(None, Some(first)) {
            first = order;
            steps += 1;
            assert!(steps < 100, "the head never ran out of room");
        }
        assert!((25..40).contains(&steps), "ran out after {steps} steps");
    }

    #[test]
    fn the_result_is_strictly_between_its_neighbours() {
        for (a, b) in [(1.0, 2.0), (0.001, 0.002), (10.0, 10.5), (1.0, 1.0 + 1e-6)] {
            let order = position_between(Some(a), Some(b)).unwrap();
            assert!(a < order && order < b, "{order} is not between {a} and {b}");
        }
    }
}
