//! The fixed event list. Next on the list (not emitted yet): `order.created`, `lot.created`,
//! `harvest.reported`, `passport.published`, `sale.attributed`.

/// Stock level of a product changed somewhere (`stock::repo::record`, `soft_delete`).
pub const STOCK_CHANGED: &str = "stock.changed";
/// A product or one of its variants was created, changed or deleted.
pub const PRODUCT_CHANGED: &str = "product.changed";

/// Events an endpoint may subscribe to.
pub const EVENTS: &[&str] = &[PRODUCT_CHANGED, STOCK_CHANGED];
