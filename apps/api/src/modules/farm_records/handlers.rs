//! HTTP edge for /v1/workspace/{workspace}/farm records: registration, then per record kind
//! (persons, households, farms, plots, crop cycles) list, get, correct (PUT) and verify;
//! harvest reports per cycle.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};

use crate::{
    AppState,
    error::{AppError, ValidationErrorBody},
    http::{
        farm::FarmAccess,
        json::ValidatedJson,
        list::{Page, Paginated, flag},
        response::{Data, Envelope, MessageBody},
    },
    modules::farm_records::{
        dto::{
            CropCycleResource, CycleCorrection, FarmCorrection, FarmResource, HarvestQuery,
            HarvestReportResource, HarvestRevision, HouseholdCorrection, HouseholdResource,
            ListQuery, PersonCorrection, PersonResource, PlotCorrection, PlotResource,
            RegistrationRequest, RegistrationResult,
        },
        repo::{self, Pick},
        service::{self, Kind, one, path_id, seen},
    },
};

/// POST /farm/registrations -> 201 new, 200 replayed.
#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/registrations", tag = "farm_records",
    operation_id = "farm_records_register", summary = "Register a farmer",
    description = "Field staff of the village. Person, household (new or joined), farm (new or \
        joined), plot and one crop cycle per crop, in one transaction: all or nothing. A retry with \
        the same `client_ref` returns the first result and writes nothing.",
    params(("workspace" = Uuid, Path, description = "Workspace id")),
    request_body = RegistrationRequest,
    responses(
        (status = 200, body = Envelope<RegistrationResult>, description = "Replayed"),
        (status = 201, body = Envelope<RegistrationResult>, description = "Registered"),
        (status = 403, body = MessageBody, description = "Caller is not field staff of this village"),
        (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role and is not a member"),
        (status = 409, body = MessageBody, description = "client_ref used by another registration"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn register(
    State(state): State<AppState>,
    access: FarmAccess,
    ValidatedJson(input): ValidatedJson<RegistrationRequest>,
) -> Result<(StatusCode, Data<RegistrationResult>), AppError> {
    let (result, created) = service::register(&state, &access, input).await?;
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Data(result)))
}

/// One record kind's list, get, correction and verification handlers.
macro_rules! record_handlers {
    (
        tag = $tag:literal, base = $base:literal, item = $item:literal, param = $param:literal,
        noun = $noun:literal,
        list = $list:ident / $list_id:literal,
        get = $get:ident / $get_id:literal,
        correct = $correct:ident / $correct_id:literal,
        verify = $verify:ident / $verify_id:literal,
        verify_path = $verify_path:literal,
        resource = $res:ty, correction = $fix:ty,
        read = $read:path, fix = $svc_fix:path, kind = $kind:expr
    ) => {
        #[doc = concat!("GET ", $base)]
        #[utoipa::path(
            get, path = $base, tag = $tag, operation_id = $list_id,
            summary = concat!("List ", $noun, "s"),
            description = "Staff see their villages; a farmer sees what belongs to them and their household.",
            params(("workspace" = Uuid, Path, description = "Workspace id"), ListQuery),
            responses(
                (status = 200, body = Paginated<$res>, description = "One page, oldest first"),
                (status = 404, body = MessageBody, description = "Unknown workspace, or caller has no farm role and is not a member"),
            ),
            security(("bearer" = []))
        )]
        pub async fn $list(
            State(state): State<AppState>,
            access: FarmAccess,
            Query(q): Query<ListQuery>,
        ) -> Result<Json<Paginated<$res>>, AppError> {
            let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
            let village = match q.village_id.as_deref().filter(|v| !v.is_empty()) {
                Some(v) => Some(path_id(v)?),
                None => None,
            };
            let scope = access.staff_scope();
            let pick = Pick { village, id: None, limit: page.size, offset: page.offset() };
            let (rows, total) = $read(&state.db, &seen(&access, &scope.villages), &pick).await?;
            Ok(Json(Paginated::new(rows.into_iter().map(Into::into).collect(), total, page)))
        }

        #[doc = concat!("GET ", $item)]
        #[utoipa::path(
            get, path = $item, tag = $tag, operation_id = $get_id,
            summary = concat!("Get a ", $noun),
            params(("workspace" = Uuid, Path, description = "Workspace id"), ($param = Uuid, Path, description = "Record id")),
            responses(
                (status = 200, body = Envelope<$res>, description = "The record"),
                (status = 404, body = MessageBody, description = "Unknown, deleted, or not visible to the caller"),
            ),
            security(("bearer" = []))
        )]
        pub async fn $get(
            State(state): State<AppState>,
            access: FarmAccess,
            Path((_, id)): Path<(String, String)>,
        ) -> Result<Data<$res>, AppError> {
            let scope = access.staff_scope();
            let rows = $read(&state.db, &seen(&access, &scope.villages), &Pick::one(path_id(&id)?)).await?;
            Ok(Data(one(rows)?.into()))
        }

        #[doc = concat!("PUT ", $item)]
        #[utoipa::path(
            put, path = $item, tag = $tag, operation_id = $correct_id,
            summary = concat!("Correct a ", $noun),
            description = "Field staff of the record's village. Send every editable field; other \
                fields are refused. Stamps the corrector as source and resets verification.",
            params(("workspace" = Uuid, Path, description = "Workspace id"), ($param = Uuid, Path, description = "Record id")),
            request_body = $fix,
            responses(
                (status = 200, body = Envelope<$res>, description = "Corrected"),
                (status = 403, body = MessageBody, description = "Caller is not field staff"),
                (status = 404, body = MessageBody, description = "Unknown, deleted, or outside the caller's villages"),
                (status = 422, body = ValidationErrorBody, description = "Invalid input or a field that may not be edited"),
            ),
            security(("bearer" = []))
        )]
        pub async fn $correct(
            State(state): State<AppState>,
            access: FarmAccess,
            Path((_, id)): Path<(String, String)>,
            ValidatedJson(fix): ValidatedJson<$fix>,
        ) -> Result<Data<$res>, AppError> {
            let id = $svc_fix(&state, &access, &id, fix).await?;
            let scope = access.staff_scope();
            let rows = $read(&state.db, &seen(&access, &scope.villages), &Pick::one(id)).await?;
            Ok(Data(one(rows)?.into()))
        }

        #[doc = concat!("POST ", $verify_path)]
        #[utoipa::path(
            post, path = $verify_path, tag = $tag, operation_id = $verify_id,
            summary = concat!("Verify a ", $noun),
            description = "Field staff of the record's village. One-way: a verified or disputed record is refused.",
            params(("workspace" = Uuid, Path, description = "Workspace id"), ($param = Uuid, Path, description = "Record id")),
            responses(
                (status = 200, body = Envelope<$res>, description = "Verified"),
                (status = 403, body = MessageBody, description = "Caller is not field staff"),
                (status = 404, body = MessageBody, description = "Unknown, deleted, or outside the caller's villages"),
                (status = 422, body = MessageBody, description = "Not awaiting verification, or the registrar verifying their own household"),
            ),
            security(("bearer" = []))
        )]
        pub async fn $verify(
            State(state): State<AppState>,
            access: FarmAccess,
            Path((_, id)): Path<(String, String)>,
        ) -> Result<Data<$res>, AppError> {
            let id = service::verify(&state, &access, $kind, &id).await?;
            let scope = access.staff_scope();
            let rows = $read(&state.db, &seen(&access, &scope.villages), &Pick::one(id)).await?;
            Ok(Data(one(rows)?.into()))
        }
    };
}

record_handlers!(
    tag = "farm_records",
    base = "/workspace/{workspace}/farm/persons",
    item = "/workspace/{workspace}/farm/persons/{person}",
    param = "person",
    noun = "person",
    list = list_persons / "farm_records_persons_list",
    get = get_person / "farm_records_persons_get",
    correct = correct_person / "farm_records_persons_correct",
    verify = verify_person / "farm_records_persons_verify",
    verify_path = "/workspace/{workspace}/farm/persons/{person}/verification",
    resource = PersonResource,
    correction = PersonCorrection,
    read = repo::persons,
    fix = service::correct_person,
    kind = Kind::Person
);
record_handlers!(
    tag = "farm_records",
    base = "/workspace/{workspace}/farm/households",
    item = "/workspace/{workspace}/farm/households/{household}",
    param = "household",
    noun = "household",
    list = list_households / "farm_records_households_list",
    get = get_household / "farm_records_households_get",
    correct = correct_household / "farm_records_households_correct",
    verify = verify_household / "farm_records_households_verify",
    verify_path = "/workspace/{workspace}/farm/households/{household}/verification",
    resource = HouseholdResource,
    correction = HouseholdCorrection,
    read = repo::households,
    fix = service::correct_household,
    kind = Kind::Household
);
record_handlers!(
    tag = "farm_records",
    base = "/workspace/{workspace}/farm/farms",
    item = "/workspace/{workspace}/farm/farms/{farm}",
    param = "farm",
    noun = "farm",
    list = list_farms / "farm_records_farms_list",
    get = get_farm / "farm_records_farms_get",
    correct = correct_farm / "farm_records_farms_correct",
    verify = verify_farm / "farm_records_farms_verify",
    verify_path = "/workspace/{workspace}/farm/farms/{farm}/verification",
    resource = FarmResource,
    correction = FarmCorrection,
    read = repo::farms,
    fix = service::correct_farm,
    kind = Kind::Farm
);
record_handlers!(
    tag = "farm_records",
    base = "/workspace/{workspace}/farm/plots",
    item = "/workspace/{workspace}/farm/plots/{plot}",
    param = "plot",
    noun = "plot",
    list = list_plots / "farm_records_plots_list",
    get = get_plot / "farm_records_plots_get",
    correct = correct_plot / "farm_records_plots_correct",
    verify = verify_plot / "farm_records_plots_verify",
    verify_path = "/workspace/{workspace}/farm/plots/{plot}/verification",
    resource = PlotResource,
    correction = PlotCorrection,
    read = repo::plots,
    fix = service::correct_plot,
    kind = Kind::Plot
);
record_handlers!(
    tag = "farm_records",
    base = "/workspace/{workspace}/farm/crop-cycles",
    item = "/workspace/{workspace}/farm/crop-cycles/{cycle}",
    param = "cycle",
    noun = "crop cycle",
    list = list_cycles / "farm_records_crop_cycles_list",
    get = get_cycle / "farm_records_crop_cycles_get",
    correct = correct_cycle / "farm_records_crop_cycles_correct",
    verify = verify_cycle / "farm_records_crop_cycles_verify",
    verify_path = "/workspace/{workspace}/farm/crop-cycles/{cycle}/verification",
    resource = CropCycleResource,
    correction = CycleCorrection,
    read = repo::cycles,
    fix = service::correct_cycle,
    kind = Kind::Cycle
);

/// GET /farm/crop-cycles/{cycle}/harvest-reports.
#[utoipa::path(
    get, path = "/workspace/{workspace}/farm/crop-cycles/{cycle}/harvest-reports", tag = "farm_records",
    operation_id = "farm_records_harvest_reports_list", summary = "List a cycle's harvest figures",
    description = "Newest first, old figures included unless `current=true`.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("cycle" = Uuid, Path, description = "Crop cycle id"),
        HarvestQuery,
    ),
    responses(
        (status = 200, body = Paginated<HarvestReportResource>, description = "One page"),
        (status = 404, body = MessageBody, description = "Unknown cycle, or not visible to the caller"),
    ),
    security(("bearer" = []))
)]
pub async fn list_harvests(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, cycle)): Path<(String, String)>,
    Query(q): Query<HarvestQuery>,
) -> Result<Json<Paginated<HarvestReportResource>>, AppError> {
    let page = Page::parse(q.page.as_deref(), q.page_size.as_deref());
    let current = q.current.as_deref().is_some_and(flag);
    let (rows, total) =
        service::harvests(&state, &access, &cycle, current, page.size, page.offset()).await?;
    Ok(Json(Paginated::new(
        rows.into_iter().map(Into::into).collect(),
        total,
        page,
    )))
}

/// POST /farm/crop-cycles/{cycle}/harvest-reports -> 201.
#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/crop-cycles/{cycle}/harvest-reports", tag = "farm_records",
    operation_id = "farm_records_harvest_reports_revise", summary = "Record a new harvest figure",
    description = "Field staff of the cycle's village. The new figure becomes the current one of \
        its kind; the previous one stays on record.",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("cycle" = Uuid, Path, description = "Crop cycle id"),
    ),
    request_body = HarvestRevision,
    responses(
        (status = 201, body = Envelope<HarvestReportResource>, description = "Recorded"),
        (status = 403, body = MessageBody, description = "Caller is not field staff"),
        (status = 404, body = MessageBody, description = "Unknown cycle, or outside the caller's villages"),
        (status = 422, body = ValidationErrorBody, description = "Invalid input"),
    ),
    security(("bearer" = []))
)]
pub async fn revise_harvest(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, cycle)): Path<(String, String)>,
    ValidatedJson(rev): ValidatedJson<HarvestRevision>,
) -> Result<(StatusCode, Data<HarvestReportResource>), AppError> {
    let row = service::revise_harvest(&state, &access, &cycle, rev).await?;
    Ok((StatusCode::CREATED, Data(row.into())))
}

/// POST /farm/harvest-reports/{report}/verification.
#[utoipa::path(
    post, path = "/workspace/{workspace}/farm/harvest-reports/{report}/verification", tag = "farm_records",
    operation_id = "farm_records_harvest_reports_verify", summary = "Verify a harvest figure",
    params(
        ("workspace" = Uuid, Path, description = "Workspace id"),
        ("report" = Uuid, Path, description = "Harvest report id"),
    ),
    responses(
        (status = 200, body = Envelope<HarvestReportResource>, description = "Verified"),
        (status = 403, body = MessageBody, description = "Caller is not field staff"),
        (status = 404, body = MessageBody, description = "Unknown, deleted, or outside the caller's villages"),
        (status = 422, body = MessageBody, description = "Not awaiting verification"),
    ),
    security(("bearer" = []))
)]
pub async fn verify_harvest(
    State(state): State<AppState>,
    access: FarmAccess,
    Path((_, id)): Path<(String, String)>,
) -> Result<Data<HarvestReportResource>, AppError> {
    let id = service::verify(&state, &access, Kind::Harvest, &id).await?;
    Ok(Data(service::harvest(&state, &access, id).await?.into()))
}
