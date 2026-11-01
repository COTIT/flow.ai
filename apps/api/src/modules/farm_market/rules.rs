//! The opportunity status machine (RUAHA `opportunity_guard`), as pure functions.

pub const STATUSES: [&str; 5] = ["proposed", "shared", "accepted", "declined", "lapsed"];
/// RUAHA's words.
pub const WRITE_ONLY: &str = "only project ops or admin may write opportunities";
pub const MUST_START: &str = "an opportunity must start as proposed";
pub const CROP_MISMATCH: &str = "opportunity must match the demand crop and project";
pub const ATTACH_ONLY: &str = "only project ops or admin may attach supply";
pub const BAD_HARVEST: &str = "supply must reference a current, undeleted expected harvest report";
pub const NOT_LIVE: &str = "supply requires a live opportunity";
pub const MISMATCH: &str = "supply must match the opportunity village, crop and harvest cycle";

/// A live opportunity holds the kg promised on it.
pub fn live(status: &str) -> bool {
    matches!(status, "proposed" | "shared" | "accepted")
}

/// May an opportunity move `from` -> `to`? Keeping its status is always allowed (RUAHA only
/// checks a change).
pub fn legal(from: &str, to: &str) -> bool {
    from == to
        || matches!(
            (from, to),
            ("proposed", "shared" | "declined" | "lapsed")
                | ("shared", "accepted" | "declined" | "lapsed")
                | ("accepted", "declined" | "lapsed")
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_ruahas_moves_are_legal() {
        let moves = [
            ("proposed", "shared"),
            ("proposed", "declined"),
            ("proposed", "lapsed"),
            ("shared", "accepted"),
            ("shared", "declined"),
            ("shared", "lapsed"),
            ("accepted", "declined"),
            ("accepted", "lapsed"),
        ];
        for from in STATUSES {
            for to in STATUSES {
                let ok = from == to || moves.contains(&(from, to));
                assert_eq!(legal(from, to), ok, "{from} -> {to}");
            }
        }
    }

    #[test]
    fn declined_and_lapsed_are_terminal() {
        for to in ["proposed", "shared", "accepted"] {
            assert!(!legal("declined", to));
            assert!(!legal("lapsed", to));
        }
    }
}
