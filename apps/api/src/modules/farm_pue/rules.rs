//! The PUE status machine (RUAHA `pue_request_guard`), as pure functions.

/// Statuses a request may be created in.
pub const CREATABLE: [&str; 2] = ["draft", "submitted"];
/// RUAHA's words.
pub const CREATE_REFUSED: &str = "a request may only be created as draft or submitted";
pub const REVIEW_ONLY: &str = "only ops or admin may review a request";

/// Transition actions and the status each one moves to.
pub const ACTIONS: [(&str, &str); 5] = [
    ("submit", "submitted"),
    ("withdraw", "withdrawn"),
    ("start_review", "under_review"),
    ("approve", "approved"),
    ("reject", "rejected"),
];

/// Why a move is refused.
#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// `illegal transition {from} -> {to}`.
    Illegal,
    /// Review states need ops or admin.
    NotReviewer,
}

/// The status `action` moves to.
pub fn target(action: &str) -> Option<&'static str> {
    ACTIONS.iter().find(|(a, _)| *a == action).map(|(_, s)| *s)
}

/// May a request move `from` -> `to`? Legality first, then the reviewer check, as RUAHA.
pub fn check(from: &str, to: &str, reviewer: bool) -> Result<(), Refusal> {
    let legal = matches!(
        (from, to),
        ("draft", "submitted" | "withdrawn")
            | ("submitted", "under_review" | "withdrawn")
            | ("under_review", "approved" | "rejected")
    );
    if !legal {
        return Err(Refusal::Illegal);
    }
    if matches!(to, "under_review" | "approved" | "rejected") && !reviewer {
        return Err(Refusal::NotReviewer);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [&str; 6] = [
        "draft",
        "submitted",
        "under_review",
        "approved",
        "rejected",
        "withdrawn",
    ];

    #[test]
    fn only_ruahas_moves_are_legal() {
        let legal = [
            ("draft", "submitted"),
            ("draft", "withdrawn"),
            ("submitted", "under_review"),
            ("submitted", "withdrawn"),
            ("under_review", "approved"),
            ("under_review", "rejected"),
        ];
        for from in ALL {
            for to in ALL {
                let ok = check(from, to, true).is_ok();
                assert_eq!(ok, legal.contains(&(from, to)), "{from} -> {to}");
            }
        }
    }

    #[test]
    fn review_states_need_a_reviewer() {
        assert_eq!(
            check("submitted", "under_review", false),
            Err(Refusal::NotReviewer)
        );
        assert_eq!(
            check("under_review", "approved", false),
            Err(Refusal::NotReviewer)
        );
        assert_eq!(check("draft", "submitted", false), Ok(()));
        assert_eq!(check("submitted", "withdrawn", false), Ok(()));
        // An illegal move is reported as illegal even to a non-reviewer.
        assert_eq!(check("submitted", "approved", false), Err(Refusal::Illegal));
    }

    #[test]
    fn actions_name_their_status() {
        assert_eq!(target("start_review"), Some("under_review"));
        assert_eq!(target("cancel"), None);
    }
}
