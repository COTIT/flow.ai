//! Quick search bodies: a few rows per entity kind for the office top bar.

use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

/// GET /quick-search query.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct QuickQuery {
    /// 3 to 100 characters after trimming; contains-match, any case, `%` and `_` literal.
    #[param(required = true, example = "honey")]
    pub q: Option<String>,
    /// Rows per group: default 5, at most 10; junk falls back to 5.
    #[param(value_type = Option<i64>)]
    pub limit: Option<String>,
}

/// Each group is null when the caller may not list that kind, else at most `limit` rows,
/// newest first.
#[derive(Serialize, ToSchema)]
pub struct QuickResults {
    #[schema(required = true)]
    pub products: Option<Vec<QuickProduct>>,
    #[schema(required = true)]
    pub orders: Option<Vec<QuickOrder>>,
    #[schema(required = true)]
    pub invoices: Option<Vec<QuickInvoice>>,
    #[schema(required = true)]
    pub lots: Option<Vec<QuickLot>>,
    #[schema(required = true)]
    pub relations: Option<Vec<QuickRelation>>,
}

/// A product hit: name, product number, EAN-13 or barcode.
#[derive(Serialize, ToSchema)]
pub struct QuickProduct {
    pub id: Uuid,
    pub name: String,
    #[schema(example = "P-00042")]
    pub product_nr: String,
    #[schema(required = true)]
    pub ean13: Option<String>,
}

/// An order hit: number or reference.
#[derive(Serialize, ToSchema)]
pub struct QuickOrder {
    pub id: Uuid,
    #[schema(example = "ORD-0001")]
    pub number: String,
    #[schema(required = true)]
    pub reference: Option<String>,
    pub status: String,
    /// Minor units of `currency`, incl VAT.
    pub total: i64,
    #[schema(example = "EUR")]
    pub currency: String,
    pub decimals: i16,
}

/// A numbered invoice hit: the digits of `q` equal its number.
#[derive(Serialize, ToSchema)]
pub struct QuickInvoice {
    pub id: Uuid,
    #[schema(example = "INV-1000")]
    pub number: String,
    pub status: String,
    /// Company name, else alias, else first contact's full name.
    #[schema(required = true)]
    pub customer_name: Option<String>,
}

/// A lot hit: code or public (QR) code.
#[derive(Serialize, ToSchema)]
pub struct QuickLot {
    pub id: Uuid,
    pub code: String,
    pub public_code: String,
    pub product_name: String,
}

/// A relation hit: company name, alias or a contact's first or last name.
#[derive(Serialize, ToSchema)]
pub struct QuickRelation {
    pub id: Uuid,
    /// Company name, else alias, else first contact's full name.
    #[schema(required = true)]
    pub name: Option<String>,
}
