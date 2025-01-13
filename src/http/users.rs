use super::auth::Claims;
use super::{auth, AppState, Error, Result};
use crate::config::Config;
use crate::http::errors::ResultExt;
use anyhow::Context;
use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash};
use axum::extract::State;
use axum::routing::{get, post};
use axum::{middleware, Extension, Json, Router};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub fn router(state: Arc<Config>) -> Router<AppState> {
    Router::new()
        .route("/api/users", post(create_user))
        .route("/api/users/login", post(login_user))
        .route(
            "/api/user",
            get(get_current_user)
                .put(update_user)
                .route_layer(middleware::from_fn_with_state(state, auth::auth)),
        )
}

#[derive(Serialize, Deserialize)]
struct UserBody<T> {
    user: T,
}

#[derive(Deserialize)]
struct NewUser {
    username: String,
    email: String,
    password: String,
}

#[derive(Deserialize)]
struct LoginUser {
    email: String,
    password: String,
}

#[derive(Deserialize, Default, PartialEq, Eq)]
#[serde(default)]
struct UpdateUser {
    email: Option<String>,
    username: Option<String>,
    password: Option<String>,
    bio: Option<String>,
    image: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct User {
    email: String,
    token: String,
    username: String,
    bio: String,
    image: Option<String>,
}

async fn create_user(
    state: State<AppState>,
    Json(req): Json<UserBody<NewUser>>,
) -> Result<Json<UserBody<User>>> {
    let password_hash = hash_password(req.user.password).await?;

    let user_id = sqlx::query_scalar!(
        // language=PostgreSQL
        "insert into \"user\" (username, email, password_hash) values ($1, $2, $3) returning user_id",
        req.user.username,
        req.user.email,
        password_hash
    )
    .fetch_one(&state.db)
    .await
    .on_constraint("user_username_key", |_| {
        Error::unprocessable_entity([("username", "username taken")])
    })
    .on_constraint("user_email_key", |_| {
        Error::unprocessable_entity([("email", "email taken")])
    })?;

    Ok(Json(UserBody {
        user: User {
            email: req.user.email,
            token: Claims::with_sub_to_jwt(user_id, &state),
            username: req.user.username,
            bio: "".to_string(),
            image: None,
        },
    }))
}

async fn login_user(
    state: State<AppState>,
    Json(req): Json<UserBody<LoginUser>>,
) -> Result<Json<UserBody<User>>> {
    let user = sqlx::query!(
        // language=PostgreSQL
        r#"
            select user_id, email, username, bio, image, password_hash
            from "user" where email = $1
        "#,
        req.user.email
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(Error::unprocessable_entity([("email", "does not exist")]))?;

    verify_password(req.user.password, user.password_hash).await?;

    Ok(Json(UserBody {
        user: User {
            email: user.email,
            token: Claims::with_sub_to_jwt(user.user_id, &state),
            username: user.username,
            bio: user.bio,
            image: None,
        },
    }))
}

async fn get_current_user(
    state: State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<UserBody<User>>> {
    let user = sqlx::query!(
        // language=PostgreSQL
        r#"
            select email, username, bio, image from "user" where user_id = $1
        "#,
        claims.sub

    )
        .fetch_one(&state.db)
        .await?;

    Ok(Json(UserBody {
        user: User {
            email: user.email,
            token: Claims::with_sub_to_jwt(claims.sub, &state),
            username: user.username,
            bio: user.bio,
            image: user.image,
        }
    }))
}

async fn update_user(
    state: State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<UserBody<UpdateUser>>,
) -> Result<Json<UserBody<User>>> {
    if req.user == UpdateUser::default() {
        return get_current_user(state, Extension(claims)).await;
    }

    let password_hash = if let Some(password) = req.user.password {
        Some(hash_password(password).await?)
    } else {
        None
    };

    let user = sqlx::query!(
        // language=PostgreSQL
        r#"
            update "user"
            set email = coalesce($1, "user".email),
                username = coalesce($2, "user".username),
                password_hash = coalesce($3, "user".password_hash),
                bio = coalesce($4, "user".bio),
                image = coalesce($5, "user".image)
            where user_id = $6
            returning email, username, bio, image
        "#,
        req.user.email,
        req.user.username,
        password_hash,
        req.user.bio,
        req.user.image,
        claims.sub
    )
        .fetch_one(&state.db)
        .await
        .on_constraint("user_username_key", |_| {
            Error::unprocessable_entity([("username", "username taken")])
        })
        .on_constraint("user_email_key", |_| {
            Error::unprocessable_entity([("email", "email taken")])
        })?;

    Ok(Json(UserBody {
        user: User {
            email: user.email,
            token: Claims::with_sub_to_jwt(claims.sub, &state),
            username: user.username,
            bio: user.bio,
            image: user.image,
        },
    }))
}

async fn hash_password(password: String) -> Result<String> {
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(rand::thread_rng());
        Ok(PasswordHash::generate(Argon2::default(), password, &salt)
            .map_err(|e| anyhow::anyhow!("failed to generate password hash: {}", e))?
            .to_string())
    })
    .await
    .context("panic in generating password hash")?
}

async fn verify_password(password: String, password_hash: String) -> Result<()> {
    tokio::task::spawn_blocking(move || -> Result<()> {
        let hash = PasswordHash::new(&password_hash)
            .map_err(|e| anyhow::anyhow!("invalid password hash: {}", e))?;

        hash.verify_password(&[&Argon2::default()], password)
            .map_err(|e| match e {
                argon2::password_hash::Error::Password => Error::Unauthorized,
                _ => anyhow::anyhow!("failed to verify password hash: {}", e).into(),
            })
    })
    .await
    .context("panic in verifying password hash")?
}
