use super::Result;
use crate::config::Config;
use crate::http::{AppState, Error};
use axum::extract::{Request, State};
use axum::http::header;
use axum::middleware::Next;
use axum::response::Response;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use time::OffsetDateTime;
use uuid::Uuid;

const DEFAULT_SESSION_LENGTH: time::Duration = time::Duration::weeks(2);

const SCHEME_PREFIX: &str = "Bearer ";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Claims {
    pub(crate) sub: Uuid,
    iat: usize,
    exp: usize,
}

impl Claims {
    pub(crate) fn with_sub_to_jwt(sub: Uuid, state: &AppState) -> String {
        let now = OffsetDateTime::now_utc();
        let iat = now.unix_timestamp() as usize;
        let exp = (now + DEFAULT_SESSION_LENGTH).unix_timestamp() as usize;

        let claims = Self { sub, iat, exp };

        let jwt = encode(
            &Header::new(Algorithm::RS256),
            &claims,
            &EncodingKey::from_rsa_pem(&state.config.rsa_private_key.as_ref()).unwrap(),
        )
        .unwrap();

        format!("{SCHEME_PREFIX}{jwt}")
    }

    fn from_jwt(jwt: &str, state: Arc<Config>) -> Result<Self> {
        Ok(decode(
            jwt,
            &DecodingKey::from_rsa_pem(state.rsa_public_key.as_ref()).unwrap(),
            &Validation::new(Algorithm::RS256),
        )
        .map_err(|_| Error::Unauthorized)?
        .claims)
    }
}

pub async fn auth(
    State(state): State<Arc<Config>>,
    mut request: Request,
    next: Next,
) -> Result<Response> {
    let jwt = request
        .headers()
        .get(header::AUTHORIZATION)
        .ok_or(Error::Unauthorized)?
        .to_str()
        .map_err(|_| Error::Unauthorized)?
        .strip_prefix(SCHEME_PREFIX)
        .ok_or(Error::Unauthorized)?;
    let claims = Claims::from_jwt(jwt, state)?;

    request.extensions_mut().insert(claims);
    Ok(next.run(request).await)
}

pub async fn maybe_auth(
    State(state): State<Arc<Config>>,
    mut request: Request,
    next: Next,
) -> Result<Response> {
    let maybe_claims = request
        .headers()
        .get(header::AUTHORIZATION)
        .ok_or(Error::Unauthorized)
        .and_then(|header| {
            Ok(header.to_str().ok().and_then(|header| {
                let jwt = header.strip_prefix(SCHEME_PREFIX)?;
                Claims::from_jwt(jwt, state).ok()
            }))
        })?;

    request.extensions_mut().insert(maybe_claims);
    Ok(next.run(request).await)
}
