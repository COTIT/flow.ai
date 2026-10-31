//! Who may answer a survey (RUAHA `survey_block_reason`). One pure function, used by the
//! farmer's list and by the submit, so they can never disagree. The first reason that applies
//! is the answer; None = may answer.

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// The survey as eligibility sees it.
pub struct SurveyFacts {
    pub status: String,
    pub closes_at: Option<DateTime<Utc>>,
    pub village_id: Option<Uuid>,
    pub published_at: Option<DateTime<Utc>>,
    pub max_households: Option<i32>,
    /// Households that answered so far.
    pub answered: i64,
}

/// One household the caller's person belongs to.
pub struct HouseholdFacts {
    pub id: Uuid,
    pub village_id: Uuid,
    pub verification: String,
    pub captured_by: Option<Uuid>,
    pub verified_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    /// This household already answered this survey.
    pub answered_this: bool,
}

/// The caller as eligibility sees them.
pub struct CallerFacts<'a> {
    pub linked: bool,
    pub must_change_password: bool,
    pub households: &'a [HouseholdFacts],
}

pub fn block_reason(
    s: &SurveyFacts,
    c: &CallerFacts<'_>,
    now: DateTime<Utc>,
) -> Option<&'static str> {
    if s.status == "draft" {
        return Some("survey not found");
    }
    if s.status != "live" || s.closes_at.is_some_and(|t| t <= now) {
        return Some("this survey is closed");
    }
    if !c.linked {
        return Some("your account is not linked to a registered farmer");
    }
    if c.must_change_password {
        return Some("set your own password before answering surveys");
    }
    let [h] = c.households else {
        return Some("your household record needs checking by an officer");
    };
    if s.village_id.is_some_and(|v| v != h.village_id) {
        return Some("this survey is not open in your village");
    }
    if h.answered_this {
        return Some("your household has already answered this survey");
    }
    if h.verification != "verified" {
        return Some("your household has not been verified yet");
    }
    if h.verified_by.is_none() || h.verified_by == h.captured_by {
        return Some("your household must be verified by a second staff member");
    }
    // Registering a household to catch an open survey does not work.
    if s.published_at.is_none_or(|p| h.created_at >= p) {
        return Some("this survey opened before your household was registered");
    }
    if s.max_households.is_some_and(|m| s.answered >= i64::from(m)) {
        return Some("this survey has reached its limit");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn ids() -> (Uuid, Uuid, Uuid) {
        (Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7())
    }

    #[test]
    fn the_first_reason_that_applies_is_the_answer() {
        let now = Utc::now();
        let (village, officer, ops) = ids();
        let survey = || SurveyFacts {
            status: "live".into(),
            closes_at: None,
            village_id: Some(village),
            published_at: Some(now - Duration::days(1)),
            max_households: Some(2),
            answered: 1,
        };
        let house = || HouseholdFacts {
            id: Uuid::now_v7(),
            village_id: village,
            verification: "verified".into(),
            captured_by: Some(officer),
            verified_by: Some(ops),
            created_at: now - Duration::days(30),
            answered_this: false,
        };
        let one = [house()];
        let caller = CallerFacts {
            linked: true,
            must_change_password: false,
            households: &one,
        };
        assert_eq!(block_reason(&survey(), &caller, now), None);

        let check = |s: SurveyFacts, h: Vec<HouseholdFacts>, linked: bool, temp: bool| {
            let c = CallerFacts {
                linked,
                must_change_password: temp,
                households: &h,
            };
            block_reason(&s, &c, now)
        };
        let s = |f: fn(&mut SurveyFacts)| {
            let mut x = survey();
            f(&mut x);
            x
        };
        let h = |f: fn(&mut HouseholdFacts)| {
            let mut x = house();
            f(&mut x);
            vec![x]
        };
        assert_eq!(
            check(s(|x| x.status = "draft".into()), h(|_| {}), true, false),
            Some("survey not found")
        );
        assert_eq!(
            check(s(|x| x.status = "closed".into()), h(|_| {}), true, false),
            Some("this survey is closed")
        );
        assert_eq!(
            check(
                s(|x| x.closes_at = Some(Utc::now() - Duration::minutes(1))),
                h(|_| {}),
                true,
                false
            ),
            Some("this survey is closed")
        );
        assert_eq!(
            check(survey(), h(|_| {}), false, false),
            Some("your account is not linked to a registered farmer")
        );
        assert_eq!(
            check(survey(), h(|_| {}), true, true),
            Some("set your own password before answering surveys")
        );
        assert_eq!(
            check(survey(), vec![], true, false),
            Some("your household record needs checking by an officer")
        );
        assert_eq!(
            check(survey(), vec![house(), house()], true, false),
            Some("your household record needs checking by an officer")
        );
        assert_eq!(
            check(
                s(|x| x.village_id = Some(Uuid::now_v7())),
                h(|_| {}),
                true,
                false
            ),
            Some("this survey is not open in your village")
        );
        assert_eq!(
            check(survey(), h(|x| x.answered_this = true), true, false),
            Some("your household has already answered this survey")
        );
        assert_eq!(
            check(
                survey(),
                h(|x| x.verification = "unverified".into()),
                true,
                false
            ),
            Some("your household has not been verified yet")
        );
        assert_eq!(
            check(survey(), h(|x| x.verified_by = x.captured_by), true, false),
            Some("your household must be verified by a second staff member")
        );
        assert_eq!(
            check(survey(), h(|x| x.created_at = Utc::now()), true, false),
            Some("this survey opened before your household was registered")
        );
        assert_eq!(
            check(s(|x| x.answered = 2), h(|_| {}), true, false),
            Some("this survey has reached its limit")
        );
        assert_eq!(
            check(s(|x| x.village_id = None), h(|_| {}), true, false),
            None,
            "a survey for every village"
        );
    }
}
