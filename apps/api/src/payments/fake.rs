//! In-process test provider: no HTTP, state in memory, scriptable from tests (set a status,
//! fail the next call, count creates per idempotency key). Selected only by
//! `PAYMENTS_PROVIDER=fake` in debug builds; clones share one state.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
};

use uuid::Uuid;

use super::{Error, Intent, NewIntent, NewRefund, Status};

#[derive(Default)]
struct State {
    /// Payments and refunds by provider id, each with its workspace.
    intents: HashMap<String, (Uuid, Intent)>,
    /// Refund ids per payment id.
    refunds: HashMap<String, Vec<String>>,
    /// Idempotency key → provider id.
    by_key: HashMap<String, String>,
    /// Create calls per idempotency key (retries included).
    creates: HashMap<String, u32>,
    fail_next: Option<Error>,
    next: u64,
}

/// The fake provider; clones share state.
#[derive(Clone, Default)]
pub struct Fake(Arc<Mutex<State>>);

impl std::fmt::Debug for Fake {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Fake")
    }
}

impl Fake {
    fn state(&self) -> MutexGuard<'_, State> {
        // A test that panicked while holding the lock must not hide later results.
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn take_failure(s: &mut State) -> Result<(), Error> {
        s.fail_next.take().map_or(Ok(()), Err)
    }

    pub(super) fn create(&self, p: &NewIntent<'_>) -> Result<Intent, Error> {
        let mut s = self.state();
        *s.creates.entry(p.idempotency_key.to_owned()).or_default() += 1;
        Self::take_failure(&mut s)?;
        if let Some(id) = s.by_key.get(p.idempotency_key).cloned() {
            return Ok(s.intents[&id].1.clone());
        }
        s.next += 1;
        let id = format!("fk_{}", s.next);
        let intent = Intent {
            id: id.clone(),
            status: Status::Open,
            amount: p.amount,
            currency: p.currency.to_owned(),
            checkout_url: Some(format!("https://pay.fake.test/{id}")),
            metadata: p.metadata.clone(),
        };
        s.by_key.insert(p.idempotency_key.to_owned(), id.clone());
        s.intents.insert(id, (p.workspace_id, intent.clone()));
        Ok(intent)
    }

    pub(super) fn fetch(&self, workspace_id: Uuid, id: &str) -> Result<Intent, Error> {
        let mut s = self.state();
        Self::take_failure(&mut s)?;
        match s.intents.get(id) {
            Some((ws, i)) if *ws == workspace_id => Ok(i.clone()),
            _ => Err(Error::NotFound),
        }
    }

    pub(super) fn refund(&self, r: &NewRefund<'_>) -> Result<Intent, Error> {
        let mut s = self.state();
        *s.creates.entry(r.idempotency_key.to_owned()).or_default() += 1;
        Self::take_failure(&mut s)?;
        if let Some(id) = s.by_key.get(r.idempotency_key).cloned() {
            return Ok(s.intents[&id].1.clone());
        }
        match s.intents.get(r.payment_id) {
            Some((ws, p)) if *ws == r.workspace_id && p.status == Status::Paid => {}
            Some((ws, _)) if *ws == r.workspace_id => {
                return Err(Error::Refused("payment not paid".into()));
            }
            _ => return Err(Error::NotFound),
        }
        s.next += 1;
        let id = format!("fr_{}", s.next);
        let intent = Intent {
            id: id.clone(),
            status: Status::Open,
            amount: r.amount,
            currency: r.currency.to_owned(),
            checkout_url: None,
            metadata: serde_json::json!({ "payment_id": r.payment_id }),
        };
        s.by_key.insert(r.idempotency_key.to_owned(), id.clone());
        s.refunds
            .entry(r.payment_id.to_owned())
            .or_default()
            .push(id.clone());
        s.intents.insert(id, (r.workspace_id, intent.clone()));
        Ok(intent)
    }

    pub(super) fn fetch_refund(
        &self,
        workspace_id: Uuid,
        payment_id: &str,
        refund_id: &str,
    ) -> Result<Intent, Error> {
        let mut s = self.state();
        Self::take_failure(&mut s)?;
        let of_payment = s
            .refunds
            .get(payment_id)
            .is_some_and(|r| r.iter().any(|r| r == refund_id));
        match s.intents.get(refund_id) {
            Some((ws, i)) if *ws == workspace_id && of_payment => Ok(i.clone()),
            _ => Err(Error::NotFound),
        }
    }

    /// Test script: the provider now reports `id` as `status`.
    pub fn set_status(&self, id: &str, status: Status) {
        if let Some((_, i)) = self.state().intents.get_mut(id) {
            i.status = status;
        }
    }

    /// Test script: the provider now reports `id`'s amount as `amount` (a mismatch).
    pub fn set_amount(&self, id: &str, amount: i64) {
        if let Some((_, i)) = self.state().intents.get_mut(id) {
            i.amount = amount;
        }
    }

    /// Test script: the next call (any kind) fails with `error`.
    pub fn fail_next(&self, error: Error) {
        self.state().fail_next = Some(error);
    }

    /// Create calls seen under `key` (failed ones included).
    pub fn creates(&self, key: &str) -> u32 {
        self.state().creates.get(key).copied().unwrap_or(0)
    }

    /// Every payment and refund the fake holds.
    pub fn intents(&self) -> Vec<Intent> {
        let mut all: Vec<Intent> = self
            .state()
            .intents
            .values()
            .map(|(_, i)| i.clone())
            .collect();
        all.sort_by(|a, b| a.id.cmp(&b.id));
        all
    }
}

/// The fake's webhook body: `id=<fk_… or fr_…>` (form encoded, as most providers send).
pub(super) fn webhook_id(body: &[u8]) -> Option<String> {
    let id = std::str::from_utf8(body).ok()?.trim().strip_prefix("id=")?;
    let ok =
        (1..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
    ok.then(|| id.to_owned())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn new<'a>(ws: Uuid, key: &'a str) -> NewIntent<'a> {
        NewIntent {
            workspace_id: ws,
            amount: 1250,
            currency: "EUR",
            decimals: 2,
            description: "Order ORD-0001",
            redirect_url: "https://shop.example.com/thanks",
            webhook_url: "https://api.example.com/v1/payments/webhooks/fake/x",
            metadata: json!({ "order_id": "o" }),
            idempotency_key: key,
        }
    }

    #[test]
    fn webhook_bodies_name_one_plain_id() {
        assert_eq!(webhook_id(b"id=fk_12"), Some("fk_12".into()));
        for bad in [&b"fk_12"[..], b"id=", b"id=fk 1", b"id=fk_1&x=2", b"\xff"] {
            assert_eq!(webhook_id(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn same_key_same_payment_and_creates_are_counted() {
        let (f, ws) = (Fake::default(), Uuid::from_u128(1));
        let a = f.create(&new(ws, "k1")).unwrap();
        let b = f.create(&new(ws, "k1")).unwrap();
        assert_eq!(a, b);
        assert_eq!(
            (a.status, a.amount, a.currency.as_str()),
            (Status::Open, 1250, "EUR")
        );
        assert!(a.checkout_url.is_some());
        assert_eq!(f.creates("k1"), 2);
        assert_ne!(f.create(&new(ws, "k2")).unwrap().id, a.id);
        assert_eq!(f.intents().len(), 2);
    }

    #[test]
    fn fetch_sees_scripted_status_and_only_its_workspace() {
        let (f, ws) = (Fake::default(), Uuid::from_u128(1));
        let a = f.create(&new(ws, "k")).unwrap();
        f.set_status(&a.id, Status::Paid);
        assert_eq!(f.fetch(ws, &a.id).unwrap().status, Status::Paid);
        assert_eq!(f.fetch(Uuid::from_u128(2), &a.id), Err(Error::NotFound));
        assert_eq!(f.fetch(ws, "fk_404"), Err(Error::NotFound));
    }

    #[test]
    fn fail_next_fails_one_call_and_still_counts_it() {
        let (f, ws) = (Fake::default(), Uuid::from_u128(1));
        f.fail_next(Error::Unavailable("down".into()));
        assert_eq!(
            f.create(&new(ws, "k")),
            Err(Error::Unavailable("down".into()))
        );
        assert_eq!(f.creates("k"), 1);
        assert!(f.create(&new(ws, "k")).is_ok());
    }

    #[test]
    fn refunds_need_a_paid_payment_and_dedupe_on_key() {
        let (f, ws) = (Fake::default(), Uuid::from_u128(1));
        let p = f.create(&new(ws, "k")).unwrap();
        let refund = |key| NewRefund {
            workspace_id: ws,
            payment_id: &p.id,
            amount: 500,
            currency: "EUR",
            decimals: 2,
            idempotency_key: key,
        };
        assert!(matches!(f.refund(&refund("r1")), Err(Error::Refused(_))));
        f.set_status(&p.id, Status::Paid);
        let r = f.refund(&refund("r2")).unwrap();
        assert_eq!(f.refund(&refund("r2")).unwrap(), r);
        assert_eq!((r.status, r.amount), (Status::Open, 500));
        f.set_status(&r.id, Status::Refunded);
        assert_eq!(
            f.fetch_refund(ws, &p.id, &r.id).unwrap().status,
            Status::Refunded
        );
        assert_eq!(f.fetch_refund(ws, "fk_x", &r.id), Err(Error::NotFound));
    }
}
