use std::sync::Arc;
use super::{Error, Profile, Result, auth};
use crate::http::auth::Claims;
use crate::http::AppState;
use axum::extract::{Path, State};
use axum::{middleware, Extension, Json, Router};
use axum::routing::{delete, get, post};
use futures::TryStreamExt;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use crate::config::Config;

pub fn router(state: Arc<Config>) -> Router<AppState> {
    Router::new()
        .route(
            "/api/articles/{slug}/comments",
            get(get_article_comments)
                .route_layer(middleware::from_fn_with_state(state.clone(), auth::maybe_auth))
        )
        .route(
            "/api/articles/{slug}/comments",
            post(add_comment)
                .route_layer(middleware::from_fn_with_state(state.clone(), auth::auth)),
        )
        .route(
            "/api/articles/{slug}/comments/{comment_id}",
            delete(delete_comment)
                .route_layer(middleware::from_fn_with_state(state, auth::auth)),

        )
}

#[derive(Deserialize, Serialize)]
struct CommentBody<T = Comment> {
    comment: T,
}

#[derive(Serialize)]
struct MultipleCommentsBody {
    comments: Vec<Comment>,
}

#[derive(Deserialize)]
struct AddComment {
    body: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Comment {
    id: i64,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
    body: String,
    author: Profile,
}

struct CommentFromQuery {
    comment_id: i64,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
    body: String,
    author_username: String,
    author_bio: String,
    author_image: Option<String>,
    following_author: bool,
}

impl CommentFromQuery {
    fn into_comment(self) -> Comment {
        Comment {
            id: self.comment_id,
            created_at: self.created_at,
            updated_at: self.updated_at,
            body: self.body,
            author: Profile {
                username: self.author_username,
                bio: self.author_bio,
                image: self.author_image,
                following: self.following_author,
            },
        }
    }
}

async fn get_article_comments(
    state: State<AppState>,
    Extension(maybe_claims): Extension<Option<Claims>>,
    Path(slug): Path<String>,
) -> Result<Json<MultipleCommentsBody>> {
    let article_id = sqlx::query_scalar!("select article_id from article where slug = $1", slug)
        .fetch_optional(&state.db)
        .await?
        .ok_or(Error::NotFound)?;

    let comments = sqlx::query_as!(
        CommentFromQuery,
        // language=PostgreSQL
        r#"
            select
                comment_id,
                comment.created_at,
                comment.updated_at,
                comment.body,
                author.username author_username,
                author.bio author_bio,
                author.image author_image,
                exists(select 1 from follow where followed_user_id = author.user_id and following_user_id = $1) "following_author!"
            from article_comment comment
            inner join "user" author using (user_id)
            where article_id = $2
            order by created_at
        "#,
        maybe_claims.as_ref().map(|claims| claims.sub),
        article_id
    )
    .fetch(&state.db)
    .map_ok(CommentFromQuery::into_comment)
    .try_collect()
    .await?;

    Ok(Json(MultipleCommentsBody { comments }))
}

async fn add_comment(
    state: State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(slug): Path<String>,
    Json(req): Json<CommentBody<AddComment>>,
) -> Result<Json<CommentBody>> {
    let comment = sqlx::query_as!(
        CommentFromQuery,
        // language=PostgreSQL
        r#"
            with inserted_comment as (
                insert into article_comment(article_id, user_id, body)
                select article_id, $1, $2
                from article
                where slug = $3
                returning comment_id, created_at, updated_at, body
            )
            select
                comment_id,
                comment.created_at,
                comment.updated_at,
                body,
                author.username author_username,
                author.bio author_bio,
                author.image author_image,
                false "following_author!"
            from inserted_comment comment
            inner join "user" author on user_id = $1
        "#,
        claims.sub,
        req.comment.body,
        slug
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(Error::NotFound)?
    .into_comment();

    Ok(Json(CommentBody { comment }))
}

async fn delete_comment(
    state: State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((slug, comment_id)): Path<(String, i64)>,
) -> Result<()> {
    let result = sqlx::query!(
        // language=PostgreSQL
        r#"
            with deleted_comment as (
                delete from article_comment
                where 
                    comment_id = $1
                    and article_id = (select article_id from article where slug = $2)
                    and user_id = $3
                returning 1 
            )
            select 
                exists(
                    select 1 from article_comment
                    inner join article using (article_id)
                    where comment_id = $1 and slug = $2
                ) "existed!",
                exists(select 1 from deleted_comment) "deleted!"
        "#,
        comment_id,
        slug,
        claims.sub
    )
    .fetch_one(&state.db)
    .await?;

    if result.deleted {
        Ok(())
    } else if result.existed {
        Err(Error::Forbidden)
    } else {
        Err(Error::NotFound)
    }
}
