//! Provider-agnostic glue (Verzo WF-01): connections to customer systems, pairs, id mappings,
//! poll cursors, correlation links and connection probes. Provider adapters live in `providers`.

use serde::Deserialize;
use utoipa::IntoParams;

use crate::outbound::Outbound;

pub mod capability;
pub mod connections;
pub mod cursors;
pub mod links;
pub mod mappings;
pub mod pairs;
pub mod probe;
pub mod providers;

/// What provider clients share: the guarded HTTP client and the provider mode.
#[derive(Clone)]
pub struct Runtime {
    /// Customer hosts are never trusted: every name passes the address guard.
    pub outbound: Outbound,
    pub mode: providers::Mode,
}

impl Runtime {
    /// Runtime with `mode` (from `INTEGRATIONS_PROVIDERS`, see `providers::select`).
    pub fn new(mode: providers::Mode) -> Result<Self, String> {
        Ok(Self {
            outbound: Outbound::new(&[])?,
            mode,
        })
    }

    /// The in-process fake (tests, local dev).
    pub fn fake(fake: providers::fake::Fake) -> Result<Self, String> {
        Self::new(providers::Mode::Fake(fake))
    }
}

/// `page` / `page_size` of a list.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct PageQuery {
    #[param(value_type = Option<i64>)]
    pub page: Option<String>,
    #[param(value_type = Option<i64>)]
    pub page_size: Option<String>,
}

/// All integration routes.
pub fn routes() -> utoipa_axum::router::OpenApiRouter<crate::AppState> {
    connections::routes::private()
        .merge(pairs::routes::private())
        .merge(mappings::routes::private())
}
