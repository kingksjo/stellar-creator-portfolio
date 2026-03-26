use actix_cors::Cors;
use actix_web::{http::StatusCode, middleware, web, App, HttpResponse, HttpServer, ResponseError};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use utoipa::{OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;

#[derive(Clone, Serialize, Deserialize, Debug, ToSchema)]
pub struct BountyRequest {
    /// Stellar address of the bounty creator
    pub creator: String,
    pub title: String,
    pub description: String,
    /// Budget in stroops (1 XLM = 10_000_000 stroops)
    pub budget: i128,
    /// Unix timestamp for the deadline
    pub deadline: u64,
}

#[derive(Clone, Serialize, Deserialize, Debug, ToSchema)]
pub struct BountyApplication {
    pub bounty_id: u64,
    /// Stellar address of the applicant
    pub freelancer: String,
    pub proposal: String,
    pub proposed_budget: i128,
    /// Estimated timeline in days
    pub timeline: u64,
}

#[derive(Clone, Serialize, Deserialize, Debug, ToSchema)]
pub struct FreelancerRegistration {
    pub name: String,
    pub discipline: String,
    pub bio: String,
}

#[derive(Clone, Serialize, Deserialize, Debug, ToSchema)]
pub struct ApiResponse<T: ToSchema> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
    pub message: Option<String>,
}

impl<T: ToSchema> ApiResponse<T> {
    fn ok(data: T, message: Option<String>) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            message,
        }
    }

    #[allow(dead_code)]
    fn err(error: String) -> Self
    where
        T: Default,
    {
        Self {
            success: false,
            data: None,
            error: Some(error),
            message: None,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, ToSchema)]
struct BountyRecord {
    id: u64,
    creator: String,
    title: String,
    description: String,
    budget: i128,
    deadline: u64,
    status: String,
    application_count: u64,
}

#[derive(Clone, Serialize, Deserialize, Debug, ToSchema)]
struct ApplicationRecord {
    id: u64,
    bounty_id: u64,
    freelancer: String,
    proposal: String,
    proposed_budget: i128,
    timeline: u64,
    status: String,
}

#[derive(Clone, Serialize, Deserialize, Debug, ToSchema)]
struct FreelancerRecord {
    address: String,
    name: String,
    discipline: String,
    bio: String,
    verified: bool,
}

#[derive(Debug)]
enum ApiError {
    BadRequest(String),
    Conflict(String),
    NotFound(String),
    Internal(String),
}

impl ApiError {
    fn message(&self) -> &str {
        match self {
            Self::BadRequest(message)
            | Self::Conflict(message)
            | Self::NotFound(message)
            | Self::Internal(message) => message,
        }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

impl ResponseError for ApiError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_response(&self) -> HttpResponse {
        let response: ApiResponse<serde_json::Value> = ApiResponse::err(self.message().to_string());
        HttpResponse::build(self.status_code()).json(response)
    }
}

#[derive(Clone, Debug, Default)]
struct InMemoryDb {
    next_bounty_id: u64,
    next_application_id: u64,
    bounties: BTreeMap<u64, BountyRecord>,
    creator_bounties: BTreeMap<String, BTreeSet<u64>>,
    applications: BTreeMap<u64, ApplicationRecord>,
    bounty_applications: BTreeMap<u64, BTreeSet<u64>>,
    freelancers: BTreeMap<String, FreelancerRecord>,
    discipline_index: BTreeMap<String, BTreeSet<String>>,
}

#[derive(Clone, Debug, Default)]
struct FailureConfig {
    fail_after_write: Option<usize>,
}

#[derive(Clone, Debug, Default)]
struct Store {
    db: Arc<Mutex<InMemoryDb>>,
    failure: Arc<Mutex<FailureConfig>>,
}

#[derive(Debug, Default)]
struct TransactionContext {
    writes: usize,
    fail_after_write: Option<usize>,
}

impl TransactionContext {
    fn record_write(&mut self) -> Result<(), ApiError> {
        self.writes += 1;
        if self.fail_after_write == Some(self.writes) {
            return Err(ApiError::Internal(
                "transaction aborted before commit".to_string(),
            ));
        }
        Ok(())
    }
}

impl Store {
    fn create_bounty(&self, request: BountyRequest) -> Result<BountyRecord, ApiError> {
        self.transaction(|db, tx| {
            db.next_bounty_id += 1;
            let bounty = BountyRecord {
                id: db.next_bounty_id,
                creator: request.creator.clone(),
                title: request.title.clone(),
                description: request.description.clone(),
                budget: request.budget,
                deadline: request.deadline,
                status: "open".to_string(),
                application_count: 0,
            };

            db.bounties.insert(bounty.id, bounty.clone());
            tx.record_write()?;

            db.creator_bounties
                .entry(bounty.creator.clone())
                .or_default()
                .insert(bounty.id);
            tx.record_write()?;

            Ok(bounty)
        })
    }

    fn list_bounties(&self) -> Vec<BountyRecord> {
        self.db
            .lock()
            .expect("db poisoned")
            .bounties
            .values()
            .cloned()
            .collect()
    }

    fn get_bounty(&self, bounty_id: u64) -> Option<BountyRecord> {
        self.db
            .lock()
            .expect("db poisoned")
            .bounties
            .get(&bounty_id)
            .cloned()
    }

    fn apply_for_bounty(&self, request: BountyApplication) -> Result<ApplicationRecord, ApiError> {
        self.transaction(|db, tx| {
            if !db.bounties.contains_key(&request.bounty_id) {
                return Err(ApiError::NotFound("Bounty not found".to_string()));
            }

            db.next_application_id += 1;
            let application = ApplicationRecord {
                id: db.next_application_id,
                bounty_id: request.bounty_id,
                freelancer: request.freelancer.clone(),
                proposal: request.proposal.clone(),
                proposed_budget: request.proposed_budget,
                timeline: request.timeline,
                status: "pending".to_string(),
            };

            db.applications.insert(application.id, application.clone());
            tx.record_write()?;

            db.bounty_applications
                .entry(request.bounty_id)
                .or_default()
                .insert(application.id);
            tx.record_write()?;

            let bounty = db.bounties.get_mut(&request.bounty_id).ok_or_else(|| {
                ApiError::Internal("Bounty disappeared during transaction".to_string())
            })?;
            bounty.application_count += 1;
            tx.record_write()?;

            Ok(application)
        })
    }

    fn register_freelancer(
        &self,
        registration: FreelancerRegistration,
    ) -> Result<FreelancerRecord, ApiError> {
        let address = registration.name.trim().to_lowercase().replace(' ', "-");
        self.transaction(|db, tx| {
            if db.freelancers.contains_key(&address) {
                return Err(ApiError::Conflict("Address already registered".to_string()));
            }

            let freelancer = FreelancerRecord {
                address: address.clone(),
                name: registration.name.clone(),
                discipline: registration.discipline.clone(),
                bio: registration.bio.clone(),
                verified: false,
            };

            db.freelancers.insert(address.clone(), freelancer.clone());
            tx.record_write()?;

            db.discipline_index
                .entry(freelancer.discipline.clone())
                .or_default()
                .insert(address.clone());
            tx.record_write()?;

            Ok(freelancer)
        })
    }

    fn list_freelancers(&self, discipline: Option<&str>) -> Vec<FreelancerRecord> {
        let db = self.db.lock().expect("db poisoned");
        match discipline.filter(|value| !value.is_empty()) {
            Some(discipline) => db
                .discipline_index
                .get(discipline)
                .into_iter()
                .flat_map(|addresses| addresses.iter())
                .filter_map(|address| db.freelancers.get(address))
                .cloned()
                .collect(),
            None => db.freelancers.values().cloned().collect(),
        }
    }

    fn get_freelancer(&self, address: &str) -> Option<FreelancerRecord> {
        self.db
            .lock()
            .expect("db poisoned")
            .freelancers
            .get(address)
            .cloned()
    }

    fn transaction<T>(
        &self,
        operation: impl FnOnce(&mut InMemoryDb, &mut TransactionContext) -> Result<T, ApiError>,
    ) -> Result<T, ApiError> {
        let failure = self
            .failure
            .lock()
            .expect("failure config poisoned")
            .clone();
        let mut db = self.db.lock().expect("db poisoned");

        // We stage every dependent write on a cloned snapshot and only swap it into the
        // shared store after all steps succeed. Any error leaves the original state untouched.
        let mut staged = db.clone();
        let mut tx = TransactionContext {
            writes: 0,
            fail_after_write: failure.fail_after_write,
        };
        let result = operation(&mut staged, &mut tx)?;
        *db = staged;
        Ok(result)
    }

    #[cfg(test)]
    fn set_fail_after_write(&self, fail_after_write: Option<usize>) {
        self.failure
            .lock()
            .expect("failure config poisoned")
            .fail_after_write = fail_after_write;
    }

    #[cfg(test)]
    fn seed_bounty(&self, bounty: BountyRecord) {
        let mut db = self.db.lock().expect("db poisoned");
        db.next_bounty_id = db.next_bounty_id.max(bounty.id);
        db.creator_bounties
            .entry(bounty.creator.clone())
            .or_default()
            .insert(bounty.id);
        db.bounties.insert(bounty.id, bounty);
    }
}

#[derive(Clone, Default)]
struct AppState {
    store: Store,
}

/// Health check
#[utoipa::path(
    get, path = "/health",
    responses((status = 200, description = "Service is healthy"))
)]
async fn health() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({
        "status": "healthy",
        "service": "stellar-api",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

/// Create a new bounty
#[utoipa::path(
    post, path = "/api/bounties",
    request_body = BountyRequest,
    responses(
        (status = 201, description = "Bounty created"),
        (status = 400, description = "Invalid request body"),
    )
)]
async fn create_bounty(
    state: web::Data<AppState>,
    body: web::Json<BountyRequest>,
) -> Result<HttpResponse, ApiError> {
    tracing::info!("Creating bounty: {:?}", body.title);
    let bounty = state.store.create_bounty(body.into_inner())?;
    let response: ApiResponse<serde_json::Value> = ApiResponse::ok(
        serde_json::json!({
            "bounty_id": bounty.id,
            "creator": bounty.creator,
            "title": bounty.title,
            "budget": bounty.budget,
            "status": bounty.status
        }),
        Some("Bounty created successfully".to_string()),
    );
    Ok(HttpResponse::Created().json(response))
}

/// List bounties (paginated)
#[utoipa::path(
    get, path = "/api/bounties",
    params(
        ("page" = Option<u32>, Query, description = "Page number (default 1)"),
        ("limit" = Option<u32>, Query, description = "Items per page (default 10)"),
        ("status" = Option<String>, Query, description = "Filter by status: open | in-progress | completed"),
    ),
    responses((status = 200, description = "Paginated list of bounties"))
)]
async fn list_bounties(state: web::Data<AppState>) -> HttpResponse {
    let bounties = state.store.list_bounties();
    let total = bounties.len();
    let response: ApiResponse<serde_json::Value> = ApiResponse::ok(
        serde_json::json!({
            "bounties": bounties,
            "total": total,
            "page": 1,
            "limit": 10
        }),
        None,
    );
    HttpResponse::Ok().json(response)
}

/// Get a single bounty by ID
#[utoipa::path(
    get, path = "/api/bounties/{id}",
    params(("id" = u64, Path, description = "Bounty ID")),
    responses(
        (status = 200, description = "Bounty details"),
        (status = 404, description = "Bounty not found"),
    )
)]
async fn get_bounty(
    state: web::Data<AppState>,
    path: web::Path<u64>,
) -> Result<HttpResponse, ApiError> {
    let bounty_id = path.into_inner();
    let bounty = state
        .store
        .get_bounty(bounty_id)
        .ok_or_else(|| ApiError::NotFound("Bounty not found".to_string()))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(bounty, None)))
}

/// Apply for a bounty
#[utoipa::path(
    post, path = "/api/bounties/{id}/apply",
    params(("id" = u64, Path, description = "Bounty ID")),
    request_body = BountyApplication,
    responses(
        (status = 201, description = "Application submitted"),
        (status = 400, description = "Invalid request body"),
        (status = 404, description = "Bounty not found"),
    )
)]
async fn apply_for_bounty(
    state: web::Data<AppState>,
    path: web::Path<u64>,
    body: web::Json<BountyApplication>,
) -> Result<HttpResponse, ApiError> {
    let bounty_id = path.into_inner();
    let application = body.into_inner();
    if application.bounty_id != bounty_id {
        return Err(ApiError::BadRequest(
            "Bounty ID in path must match request body".to_string(),
        ));
    }

    let created = state.store.apply_for_bounty(application)?;
    let response: ApiResponse<serde_json::Value> = ApiResponse::ok(
        serde_json::json!({
            "application_id": created.id,
            "bounty_id": created.bounty_id,
            "freelancer": created.freelancer,
            "status": created.status
        }),
        Some("Application submitted successfully".to_string()),
    );
    Ok(HttpResponse::Created().json(response))
}

/// Register a freelancer profile
#[utoipa::path(
    post, path = "/api/freelancers/register",
    request_body = FreelancerRegistration,
    responses(
        (status = 201, description = "Freelancer registered"),
        (status = 409, description = "Address already registered"),
    )
)]
async fn register_freelancer(
    state: web::Data<AppState>,
    body: web::Json<FreelancerRegistration>,
) -> Result<HttpResponse, ApiError> {
    let freelancer = state.store.register_freelancer(body.into_inner())?;
    let response: ApiResponse<serde_json::Value> = ApiResponse::ok(
        serde_json::json!({
            "name": freelancer.name,
            "discipline": freelancer.discipline,
            "verified": freelancer.verified
        }),
        Some("Freelancer registered successfully".to_string()),
    );
    Ok(HttpResponse::Created().json(response))
}

/// List freelancers
#[utoipa::path(
    get, path = "/api/freelancers",
    params(
        ("discipline" = Option<String>, Query, description = "Filter by discipline"),
        ("page" = Option<u32>, Query, description = "Page number"),
        ("limit" = Option<u32>, Query, description = "Items per page"),
    ),
    responses((status = 200, description = "Paginated list of freelancers"))
)]
async fn list_freelancers(
    state: web::Data<AppState>,
    query: web::Query<HashMap<String, String>>,
) -> HttpResponse {
    let discipline = query.get("discipline").map(String::as_str);
    let freelancers = state.store.list_freelancers(discipline);
    let total = freelancers.len();
    let response: ApiResponse<serde_json::Value> = ApiResponse::ok(
        serde_json::json!({
            "freelancers": freelancers,
            "total": total,
            "filters": { "discipline": discipline.unwrap_or_default() }
        }),
        None,
    );
    HttpResponse::Ok().json(response)
}

/// Get a freelancer by Stellar address
#[utoipa::path(
    get, path = "/api/freelancers/{address}",
    params(("address" = String, Path, description = "Stellar address")),
    responses(
        (status = 200, description = "Freelancer profile"),
        (status = 404, description = "Freelancer not found"),
    )
)]
async fn get_freelancer(
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, ApiError> {
    let address = path.into_inner();
    let freelancer = state
        .store
        .get_freelancer(&address)
        .ok_or_else(|| ApiError::NotFound("Freelancer not found".to_string()))?;
    Ok(HttpResponse::Ok().json(ApiResponse::ok(freelancer, None)))
}

/// Get escrow details
#[utoipa::path(
    get, path = "/api/escrow/{id}",
    params(("id" = u64, Path, description = "Escrow ID")),
    responses(
        (status = 200, description = "Escrow details"),
        (status = 404, description = "Escrow not found"),
    )
)]
async fn get_escrow(path: web::Path<u64>) -> HttpResponse {
    let escrow_id = path.into_inner();
    let response: ApiResponse<serde_json::Value> = ApiResponse::ok(
        serde_json::json!({ "id": escrow_id, "status": "active", "amount": 0 }),
        None,
    );
    HttpResponse::Ok().json(response)
}

/// Release escrowed funds
#[utoipa::path(
    post, path = "/api/escrow/{id}/release",
    params(("id" = u64, Path, description = "Escrow ID")),
    responses(
        (status = 200, description = "Funds released"),
        (status = 403, description = "Not authorised to release"),
        (status = 404, description = "Escrow not found"),
    )
)]
async fn release_escrow(path: web::Path<u64>) -> HttpResponse {
    let escrow_id = path.into_inner();
    let response: ApiResponse<serde_json::Value> = ApiResponse::ok(
        serde_json::json!({ "id": escrow_id, "status": "released" }),
        Some("Funds released successfully".to_string()),
    );
    HttpResponse::Ok().json(response)
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Stellar Creator Platform API",
        version = "0.1.0",
        description = "REST API for the Stellar Creator Portfolio & Bounty Marketplace",
        contact(name = "Stellar Team", email = "support@stellar.dev"),
        license(name = "MIT"),
    ),
    paths(
        health,
        create_bounty,
        list_bounties,
        get_bounty,
        apply_for_bounty,
        register_freelancer,
        list_freelancers,
        get_freelancer,
        get_escrow,
        release_escrow,
    ),
    components(schemas(
        BountyRequest,
        BountyApplication,
        FreelancerRegistration,
        BountyRecord,
        ApplicationRecord,
        FreelancerRecord
    )),
    tags(
        (name = "bounties", description = "Bounty management"),
        (name = "freelancers", description = "Freelancer registry"),
        (name = "escrow", description = "Payment escrow"),
    )
)]
pub struct ApiDoc;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info,stellar_api=debug".to_string()),
        )
        .init();

    let port = std::env::var("API_PORT")
        .unwrap_or_else(|_| "3001".to_string())
        .parse::<u16>()
        .expect("API_PORT must be a valid port number");

    let host = std::env::var("API_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let openapi = ApiDoc::openapi();
    let state = AppState::default();

    tracing::info!("Starting Stellar API on {}:{}", host, port);
    tracing::info!(
        "Swagger UI available at http://{}:{}/swagger-ui/",
        host,
        port
    );

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(state.clone()))
            .wrap(Cors::permissive())
            .wrap(middleware::Logger::default())
            .wrap(middleware::NormalizePath::trim())
            .service(
                SwaggerUi::new("/swagger-ui/{_:.*}").url("/api-docs/openapi.json", openapi.clone()),
            )
            .route("/health", web::get().to(health))
            .route("/api/bounties", web::post().to(create_bounty))
            .route("/api/bounties", web::get().to(list_bounties))
            .route("/api/bounties/{id}", web::get().to(get_bounty))
            .route("/api/bounties/{id}/apply", web::post().to(apply_for_bounty))
            .route(
                "/api/freelancers/register",
                web::post().to(register_freelancer),
            )
            .route("/api/freelancers", web::get().to(list_freelancers))
            .route("/api/freelancers/{address}", web::get().to(get_freelancer))
            .route("/api/escrow/{id}", web::get().to(get_escrow))
            .route("/api/escrow/{id}/release", web::post().to(release_escrow))
    })
    .bind((host.as_str(), port))?
    .run()
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::{body::to_bytes, http::StatusCode, test};

    #[test]
    fn test_api_response_ok() {
        let response: ApiResponse<String> = ApiResponse::ok("test".to_string(), None);
        assert!(response.success);
        assert_eq!(response.data, Some("test".to_string()));
    }

    #[test]
    fn test_api_response_err() {
        let response: ApiResponse<String> = ApiResponse::err("error".to_string());
        assert!(!response.success);
        assert_eq!(response.error, Some("error".to_string()));
    }

    #[test]
    fn test_openapi_spec_is_valid() {
        let spec = ApiDoc::openapi();
        let paths = &spec.paths.paths;
        assert!(paths.contains_key("/health"));
        assert!(paths.contains_key("/api/bounties"));
        assert!(paths.contains_key("/api/freelancers"));
        assert!(paths.contains_key("/api/escrow/{id}"));
    }

    #[test]
    fn create_bounty_commits_all_related_writes() {
        let store = Store::default();

        let created = store
            .create_bounty(BountyRequest {
                creator: "GCREATOR".to_string(),
                title: "Design a logo".to_string(),
                description: "Need a logo".to_string(),
                budget: 100,
                deadline: 12345,
            })
            .expect("bounty should be created");

        let db = store.db.lock().expect("db poisoned");
        assert_eq!(
            db.bounties.get(&created.id).map(|b| b.title.as_str()),
            Some("Design a logo")
        );
        assert!(db
            .creator_bounties
            .get("GCREATOR")
            .is_some_and(|bounties| bounties.contains(&created.id)));
    }

    #[test]
    fn create_bounty_rolls_back_when_transaction_fails_midway() {
        let store = Store::default();
        store.set_fail_after_write(Some(1));

        let result = store.create_bounty(BountyRequest {
            creator: "GCREATOR".to_string(),
            title: "Broken create".to_string(),
            description: "Should rollback".to_string(),
            budget: 100,
            deadline: 12345,
        });

        assert!(matches!(result, Err(ApiError::Internal(_))));
        let db = store.db.lock().expect("db poisoned");
        assert!(db.bounties.is_empty());
        assert!(db.creator_bounties.is_empty());
    }

    #[test]
    fn apply_for_bounty_rolls_back_all_writes_on_failure() {
        let store = Store::default();
        store.seed_bounty(BountyRecord {
            id: 1,
            creator: "GCREATOR".to_string(),
            title: "Design a logo".to_string(),
            description: "Need a logo".to_string(),
            budget: 100,
            deadline: 12345,
            status: "open".to_string(),
            application_count: 0,
        });
        store.set_fail_after_write(Some(2));

        let result = store.apply_for_bounty(BountyApplication {
            bounty_id: 1,
            freelancer: "GFREELANCER".to_string(),
            proposal: "I can do it".to_string(),
            proposed_budget: 90,
            timeline: 7,
        });

        assert!(matches!(result, Err(ApiError::Internal(_))));
        let db = store.db.lock().expect("db poisoned");
        assert!(db.applications.is_empty());
        assert!(db.bounty_applications.is_empty());
        assert_eq!(db.bounties.get(&1).map(|b| b.application_count), Some(0));
    }

    #[test]
    fn duplicate_freelancer_registration_does_not_partially_persist() {
        let store = Store::default();

        store
            .register_freelancer(FreelancerRegistration {
                name: "Alice Doe".to_string(),
                discipline: "Design".to_string(),
                bio: "Bio".to_string(),
            })
            .expect("initial registration should succeed");

        let duplicate = store.register_freelancer(FreelancerRegistration {
            name: "Alice Doe".to_string(),
            discipline: "Design".to_string(),
            bio: "Updated bio".to_string(),
        });

        assert!(matches!(duplicate, Err(ApiError::Conflict(_))));
        let db = store.db.lock().expect("db poisoned");
        assert_eq!(db.freelancers.len(), 1);
        assert_eq!(
            db.discipline_index.get("Design").map(BTreeSet::len),
            Some(1)
        );
    }

    #[actix_web::test]
    async fn apply_handler_rejects_path_body_mismatch_without_writes() {
        let state = AppState::default();
        state.store.seed_bounty(BountyRecord {
            id: 1,
            creator: "GCREATOR".to_string(),
            title: "Design a logo".to_string(),
            description: "Need a logo".to_string(),
            budget: 100,
            deadline: 12345,
            status: "open".to_string(),
            application_count: 0,
        });

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(state.clone()))
                .route("/api/bounties/{id}/apply", web::post().to(apply_for_bounty)),
        )
        .await;

        let request = test::TestRequest::post()
            .uri("/api/bounties/1/apply")
            .set_json(BountyApplication {
                bounty_id: 2,
                freelancer: "GFREELANCER".to_string(),
                proposal: "I can do it".to_string(),
                proposed_budget: 90,
                timeline: 7,
            })
            .to_request();

        let response = test::call_service(&app, request).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = to_bytes(response.into_body())
            .await
            .expect("body should be readable");
        let payload: ApiResponse<serde_json::Value> =
            serde_json::from_slice(&body).expect("response should deserialize");
        assert!(!payload.success);

        let db = state.store.db.lock().expect("db poisoned");
        assert!(db.applications.is_empty());
        assert!(db.bounty_applications.is_empty());
        assert_eq!(db.bounties.get(&1).map(|b| b.application_count), Some(0));
    }
}
