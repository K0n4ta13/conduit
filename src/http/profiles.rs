use super::{auth, AppState, Error, Result};
use crate::config::Config;
use crate::http::auth::Claims;
use crate::http::errors::ResultExt;
use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::{middleware, Extension, Json, Router};
use serde::Serialize;
use std::sync::Arc;

pub fn router(state: Arc<Config>) -> Router<AppState> {
    Router::new()
        .route(
            "/api/profiles/{username}",
            get(get_user_profile).route_layer(middleware::from_fn_with_state(
                state.clone(),
                auth::maybe_auth,
            )),
        )
        .route(
            "/api/profiles/{username}/follow",
            post(follow_user)
                .delete(unfollow_user)
                .route_layer(middleware::from_fn_with_state(state, auth::auth)),
        )
}

#[derive(Serialize)]
struct ProfileBody {
    profile: Profile,
}

#[derive(Serialize)]
pub struct Profile {
    pub username: String,
    pub bio: String,
    pub image: Option<String>,
    pub following: bool,
}

async fn get_user_profile(
    state: State<AppState>,
    Extension(maybe_claims): Extension<Option<Claims>>,
    Path(username): Path<String>,
) -> Result<Json<ProfileBody>> {
    let profile = sqlx::query_as!(
        Profile,
        // language=PostgreSQL
        r#"
            select
                username,
                bio,
                image,
                exists(
                    select 1 from follow
                    where followed_user_id = "user".user_id and following_user_id = $2
                ) "following!"
            from "user"
            where username = $1
        "#,
        username,
        maybe_claims.as_ref().map(|claims| claims.sub)
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(Error::NotFound)?;

    Ok(Json(ProfileBody { profile }))
}

async fn follow_user(
    state: State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(username): Path<String>,
) -> Result<Json<ProfileBody>> {
    let profile = sqlx::query_as!(
        Profile,
        // language=PostgreSQL
        r#"
            with selected_user as (
                select user_id, username, bio, image
                from "user" where username = $1
            ),
            insert_follow as (
                insert into follow (following_user_id, followed_user_id)
                    select $2, user_id
                    from selected_user
                    on conflict do nothing
            )
            select su.username, su.bio, su.image, true "following!"
            from selected_user su;
        "#,
        username,
        claims.sub
    )
    .fetch_one(&state.db)
    .await
    .on_constraint("user_cannot_follow_self", |_| Error::Forbidden)?;

    Ok(Json(ProfileBody { profile }))
}

async fn unfollow_user(
    state: State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(username): Path<String>,
) -> Result<Json<ProfileBody>> {
    let profile = sqlx::query_as!(
        Profile,
        // language=PostgreSQL
        r#"
            with selected_user as (
                select user_id, username, bio, image
                from "user" where username = $1
            ),
            delet_follow as (
                delete from follow where following_user_id = $2
                    and followed_user_id = (SELECT user_id FROM selected_user)
            )
            select su.username, su.bio, su.image, false "following!"
            from selected_user su;
        "#,
        username,
        claims.sub
    )
        .fetch_one(&state.db)
        .await?;

    Ok(Json(ProfileBody { profile }))
}