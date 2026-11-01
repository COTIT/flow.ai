//! Pure voucher rules: code normalising and why a staff member may not redeem now.

use chrono::{DateTime, FixedOffset, Offset, Utc};
use uuid::Uuid;

/// Tanzania keeps UTC+3 all year.
const EAT_SECONDS: i32 = 3 * 3600;

fn tz() -> FixedOffset {
    // In range by construction (|offset| < 24 h), so the fallback never runs.
    FixedOffset::east_opt(EAT_SECONDS).unwrap_or_else(|| Utc.fix())
}

/// A typed or scanned code -> stored form: any `PREFIX:` dropped, upper case, separators gone,
/// O read as 0 and I/L as 1 (Crockford).
pub fn normalize(code: &str) -> String {
    let code = code.rsplit(':').next().unwrap_or_default();
    code.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| match c.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            c => c,
        })
        .collect()
}

/// A voucher as the redeem rule sees it.
pub struct VoucherFacts<'a> {
    pub status: &'a str,
    pub redeemed_at: Option<DateTime<Utc>>,
    pub redeemer_name: Option<&'a str>,
    pub void_reason: Option<&'a str>,
    pub expires_at: DateTime<Utc>,
    pub audit_required: bool,
    pub household_captured_by: Option<Uuid>,
    pub household_verified_by: Option<Uuid>,
}

/// The staff member asking.
pub struct Redeemer {
    pub id: Uuid,
    /// Their own person is a member of the voucher's household.
    pub in_household: bool,
    /// ops or admin.
    pub manages: bool,
}

/// Why a staff member may not hand cash to a household (vouchers and payouts alike).
#[derive(Debug, PartialEq, Eq)]
pub enum Conflict {
    /// They registered the household.
    Registered,
    /// They verified the household.
    Verified,
    /// Their own person belongs to it.
    OwnHousehold,
}

/// Four eyes on cash to a household: whoever registered or verified it, or belongs to it,
/// must leave the hand-over to someone else.
pub fn household_conflict(
    captured_by: Option<Uuid>,
    verified_by: Option<Uuid>,
    me: Uuid,
    in_household: bool,
) -> Option<Conflict> {
    if captured_by == Some(me) {
        Some(Conflict::Registered)
    } else if verified_by == Some(me) {
        Some(Conflict::Verified)
    } else if in_household {
        Some(Conflict::OwnHousehold)
    } else {
        None
    }
}

/// RUAHA `voucher_block_reason`: the first reason that applies, None = may redeem.
pub fn block_reason(v: &VoucherFacts<'_>, me: &Redeemer, now: DateTime<Utc>) -> Option<String> {
    if v.status == "redeemed" {
        let at = v
            .redeemed_at
            .map(|t| t.with_timezone(&tz()).format("%d %b %Y %H:%M").to_string());
        return Some(format!(
            "voucher already redeemed on {} by {}",
            at.unwrap_or_default(),
            v.redeemer_name.unwrap_or("another staff member")
        ));
    }
    if v.status == "void" {
        return Some(format!(
            "this voucher was voided: {}",
            v.void_reason.unwrap_or_default()
        ));
    }
    if v.expires_at <= now {
        let on = v.expires_at.with_timezone(&tz()).format("%d %b %Y");
        return Some(format!("this voucher expired on {on}"));
    }
    match household_conflict(
        v.household_captured_by,
        v.household_verified_by,
        me.id,
        me.in_household,
    ) {
        Some(Conflict::Registered) => {
            return Some(
                "you registered this household, so another staff member must redeem this voucher"
                    .into(),
            );
        }
        Some(Conflict::Verified) => {
            return Some(
                "you verified this household, so another staff member must redeem this voucher"
                    .into(),
            );
        }
        Some(Conflict::OwnHousehold) => {
            return Some("you cannot redeem a voucher for your own household".into());
        }
        None => {}
    }
    if v.audit_required && !me.manages {
        return Some(
            "this voucher is held for an audit: ops or admin must redeem it in person".into(),
        );
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};

    #[test]
    fn codes_normalise_like_crockford() {
        assert_eq!(normalize("ab12-cd34 ef"), "AB12CD34EF");
        assert_eq!(normalize("R360V:k7oil2qx9z"), "K70112QX9Z");
        assert_eq!(normalize(" o0Ii Ll "), "001111");
    }

    #[test]
    fn three_staff_audit_hold_and_states() {
        let (registrar, verifier, third) = (Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7());
        let now = Utc
            .with_ymd_and_hms(2026, 10, 31, 9, 0, 0)
            .single()
            .unwrap();
        let v = |f: fn(&mut VoucherFacts<'static>)| {
            let mut x = VoucherFacts {
                status: "issued",
                redeemed_at: None,
                redeemer_name: None,
                void_reason: None,
                expires_at: now + Duration::days(10),
                audit_required: false,
                household_captured_by: Some(registrar),
                household_verified_by: Some(verifier),
            };
            f(&mut x);
            x
        };
        let me = |id, in_household, manages| Redeemer {
            id,
            in_household,
            manages,
        };
        assert_eq!(
            block_reason(&v(|_| {}), &me(third, false, false), now),
            None
        );
        assert_eq!(
            block_reason(&v(|_| {}), &me(registrar, false, true), now).as_deref(),
            Some("you registered this household, so another staff member must redeem this voucher")
        );
        assert_eq!(
            block_reason(&v(|_| {}), &me(verifier, false, true), now).as_deref(),
            Some("you verified this household, so another staff member must redeem this voucher")
        );
        assert_eq!(
            block_reason(&v(|_| {}), &me(third, true, false), now).as_deref(),
            Some("you cannot redeem a voucher for your own household")
        );
        let held = v(|x| x.audit_required = true);
        assert_eq!(
            block_reason(&held, &me(third, false, false), now).as_deref(),
            Some("this voucher is held for an audit: ops or admin must redeem it in person")
        );
        assert_eq!(block_reason(&held, &me(third, false, true), now), None);
        let redeemed = VoucherFacts {
            status: "redeemed",
            redeemed_at: Some(now - Duration::hours(4)),
            redeemer_name: Some("Juma Ally"),
            ..v(|_| {})
        };
        assert_eq!(
            block_reason(&redeemed, &me(third, false, false), now).as_deref(),
            Some("voucher already redeemed on 31 Oct 2026 08:00 by Juma Ally"),
            "times in Tanzanian time"
        );
        let expired = v(|x| x.expires_at -= Duration::days(20));
        assert_eq!(
            block_reason(&expired, &me(third, false, false), now).as_deref(),
            Some("this voucher expired on 21 Oct 2026")
        );
        let void = VoucherFacts {
            status: "void",
            void_reason: Some("duplicate"),
            ..v(|_| {})
        };
        assert_eq!(
            block_reason(&void, &me(third, false, false), now).as_deref(),
            Some("this voucher was voided: duplicate")
        );
    }
}
