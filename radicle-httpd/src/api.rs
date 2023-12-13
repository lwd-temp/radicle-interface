pub mod auth;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::Method;
use axum::response::{IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::RwLock;
use tower_http::cors::{self, CorsLayer};

use radicle::cob::issue;
use radicle::cob::patch;
use radicle::identity::{DocAt, Id};
use radicle::node::policy::Scope;
use radicle::node::routing::Store;
use radicle::node::{Handle, NodeId};
use radicle::storage::{ReadRepository, ReadStorage};
use radicle::{Node, Profile};

mod error;
mod json;
mod v1;

use crate::api::error::Error;
use crate::cache::Cache;
use crate::Options;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Identifier for sessions
type SessionId = String;

#[derive(Clone)]
pub struct Context {
    profile: Arc<Profile>,
    sessions: Arc<RwLock<HashMap<SessionId, auth::Session>>>,
    cache: Option<Cache>,
}

impl Context {
    pub fn new(profile: Arc<Profile>, options: &Options) -> Self {
        Self {
            profile,
            sessions: Default::default(),
            cache: options.cache.map(Cache::new),
        }
    }

    pub fn project_info(&self, id: Id) -> Result<project::Info, error::Error> {
        let storage = &self.profile.storage;
        let repo = storage.repository(id)?;
        let (_, head) = repo.head()?;
        let DocAt { doc, .. } = repo.identity_doc()?;

        let payload = doc.project()?;
        let delegates = doc.delegates;
        let issues = issue::Issues::open(&repo)?.counts()?;
        let patches = patch::Patches::open(&repo)?.counts()?;
        let db = &self.profile.database()?;
        let seeding = db.count(&id).unwrap_or_default();

        Ok(project::Info {
            payload,
            delegates,
            visibility: doc.visibility,
            head,
            issues,
            patches,
            id,
            seeding,
        })
    }

    #[cfg(test)]
    pub fn profile(&self) -> &Arc<Profile> {
        &self.profile
    }

    #[cfg(test)]
    pub fn sessions(&self) -> &Arc<RwLock<HashMap<SessionId, auth::Session>>> {
        &self.sessions
    }
}

pub fn router(ctx: Context) -> Router {
    Router::new()
        .route("/", get(root_handler))
        .merge(v1::router(ctx))
        .layer(
            CorsLayer::new()
                .max_age(Duration::from_secs(86400))
                .allow_origin(cors::Any)
                .allow_methods([
                    Method::GET,
                    Method::POST,
                    Method::PATCH,
                    Method::PUT,
                    Method::DELETE,
                ])
                .allow_headers([CONTENT_TYPE, AUTHORIZATION]),
        )
}

async fn root_handler() -> impl IntoResponse {
    let response = json!({
        "path": "/api",
        "links": [
            {
                "href": "/v1",
                "rel": "v1",
                "type": "GET"
            }
        ]
    });

    Json(response)
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PaginationQuery {
    pub page: Option<usize>,
    pub per_page: Option<usize>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RawQuery {
    pub mime: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CobsQuery<T> {
    pub page: Option<usize>,
    pub per_page: Option<usize>,
    pub state: Option<T>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PoliciesQuery {
    /// The NID from which to fetch from after tracking a repo.
    pub from: Option<NodeId>,
    pub scope: Option<Scope>,
}

#[derive(Default, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub enum IssueState {
    Closed,
    #[default]
    Open,
}

impl IssueState {
    pub fn matches(&self, issue: &issue::State) -> bool {
        match self {
            Self::Open => matches!(issue, issue::State::Open),
            Self::Closed => matches!(issue, issue::State::Closed { .. }),
        }
    }
}

#[derive(Default, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub enum PatchState {
    #[default]
    Open,
    Draft,
    Archived,
    Merged,
}

impl PatchState {
    pub fn matches(&self, patch: &patch::State) -> bool {
        match self {
            Self::Open => matches!(patch, patch::State::Open { .. }),
            Self::Draft => matches!(patch, patch::State::Draft),
            Self::Archived => matches!(patch, patch::State::Archived),
            Self::Merged => matches!(patch, patch::State::Merged { .. }),
        }
    }
}

mod project {
    use nonempty::NonEmpty;
    use serde::Serialize;

    use radicle::cob;
    use radicle::git::Oid;
    use radicle::identity::project::Project;
    use radicle::identity::{Id, Visibility};
    use radicle::prelude::Did;

    /// Project info.
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Info {
        /// Project metadata.
        #[serde(flatten)]
        pub payload: Project,
        pub delegates: NonEmpty<Did>,
        pub visibility: Visibility,
        pub head: Oid,
        pub patches: cob::patch::PatchCounts,
        pub issues: cob::issue::IssueCounts,
        pub id: Id,
        pub seeding: usize,
    }
}

/// Announce refs to the network for the given RID.
pub fn announce_refs(mut node: Node, rid: Id) -> Result<(), Error> {
    match node.announce_refs(rid) {
        Ok(_) => Ok(()),
        Err(e) if e.is_connection_err() => Ok(()),
        Err(e) => Err(e.into()),
    }
}
