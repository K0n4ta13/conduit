mod comments;

use super::profiles::Profile;
use super::{auth, AppState, Error, Result};
use crate::config::Config;
use crate::http::auth::Claims;
use crate::http::errors::ResultExt;
use axum::extract::{Path, State};
use axum::routing::{get, post, put};
use axum::{middleware, Extension, Json, Router};
use serde::{Deserialize, Serialize};
use sqlx::{Executor, Postgres};
use std::sync::Arc;
use time::OffsetDateTime;
use uuid::Uuid;

pub fn router(state: Arc<Config>) -> Router<AppState> {
    Router::new()
        .route(
            "/api/articles",
            post(create_article)
                .route_layer(middleware::from_fn_with_state(state.clone(), auth::auth)),
        )
        .route(
            "/api/articles/:slug",
            get(get_article)
                .route_layer(middleware::from_fn_with_state(state.clone(), auth::maybe_auth))
        )
        .route(
            "/api/articles/:slug",
            put(update_article).delete(delete_article)
                .route_layer(middleware::from_fn_with_state(state.clone(), auth::auth))
        )
        .route(
            "/api/articles/:slug/favorite",
            post(favorite_article).delete(unfavorite_article)
                .route_layer(middleware::from_fn_with_state(state.clone(), auth::auth)),
        )
        .route("/api/tags", get(get_tags))
        .merge(comments::router(state))
}

#[derive(Serialize, Deserialize)]
struct ArticleBody<T = Article> {
    article: T,
}

#[derive(Serialize)]
struct TagsBody {
    tags: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateArticle {
    title: String,
    description: String,
    body: String,
    tag_list: Vec<String>,
}

#[derive(Deserialize)]
struct UpdateArticle {
    title: Option<String>,
    description: Option<String>,
    body: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Article {
    slug: String,
    title: String,
    description: String,
    body: String,
    tag_list: Vec<String>,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
    favorited: bool,
    favorites_count: i64,
    author: Profile,
}

struct ArticleFromQuery {
    slug: String,
    title: String,
    description: String,
    body: String,
    tag_list: Vec<String>,
    created_at: OffsetDateTime,
    updated_at: OffsetDateTime,
    favorited: bool,
    favorites_count: i64,
    author_username: String,
    author_bio: String,
    author_image: Option<String>,
    following_author: bool,
}

impl ArticleFromQuery {
    fn into_article(self) -> Article {
        Article {
            slug: self.slug,
            title: self.title,
            description: self.description,
            body: self.body,
            tag_list: self.tag_list,
            created_at: self.created_at,
            updated_at: self.updated_at,
            favorited: self.favorited,
            favorites_count: self.favorites_count,
            author: Profile {
                username: self.author_username,
                bio: self.author_bio,
                image: self.author_image,
                following: self.following_author,
            },
        }
    }
}

async fn create_article(
    state: State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(mut req): Json<ArticleBody<CreateArticle>>,
) -> Result<Json<ArticleBody>> {
    let slug = slugify(&req.article.title);

    req.article.tag_list.sort();

    let article = sqlx::query_as!(
        ArticleFromQuery,
        // language=PostgreSQL
        r#"
            with inserted_article as (
                insert into article (user_id, slug, title, description, body, tag_list)
                values ($1, $2, $3, $4, $5, $6)
                returning
                    slug,
                    title,
                    description,
                    body,
                    tag_list,
                    created_at,
                    updated_at
            )
            select
                inserted_article.*,
                false "favorited!",
                0::int8 "favorites_count!",
                username author_username,
                bio author_bio,
                image author_image,
                false "following_author!"
            from inserted_article
            inner join "user" on user_id = $1
        "#,
        claims.sub,
        slug,
        req.article.title,
        req.article.description,
        req.article.body,
        &req.article.tag_list[..]
    )
    .fetch_one(&state.db)
    .await
    .on_constraint("article_slug_key", |_| {
        Error::unprocessable_entity([("slug", format!("duplicate article slug: {}", slug))])
    })?;

    Ok(Json(ArticleBody {
        article: article.into_article(),
    }))
}

async fn update_article(
    state: State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(slug): Path<String>,
    Json(req): Json<ArticleBody<UpdateArticle>>,
) -> Result<Json<ArticleBody>> {
    let new_slug = req.article.title.as_deref().map(slugify);

    let article = sqlx::query_as!(
        ArticleFromQuery,
        // language=PostgreSQL
        r#"
            with permission_check as (
                select article_id from article
                where slug = $1 and user_id = $2
            ),
            updated_article as (
                update article
                    set
                        slug = coalesce($3, slug),
                        title = coalesce($4, title),
                        description = coalesce($5, description),
                        body = coalesce($6, body)
                    where slug = $1 and exists(select 1 from permission_check)
                    returning
                        slug,
                        title,
                        description,
                        body,
                        tag_list,
                        article.created_at,
                        article.updated_at
            )
            select
                updated_article.*,
                exists(select 1 from article_favorite where user_id = $2) "favorited!",
                (select count(*) from article_favorite fav where fav.article_id = (select article_id from permission_check)) "favorites_count!",
                author.username "author_username",
                author.bio "author_bio",
                author.image "author_image",
                false "following_author!"
            from updated_article
                     inner join "user" author on author.user_id = $2
        "#,
        slug,
        claims.sub,
        new_slug,
        req.article.title,
        req.article.description,
        req.article.body
    )
    .fetch_one(&state.db)
    .await
    .on_constraint("article_slug_key", |_| {
        Error::unprocessable_entity([
            ("slug", format!("duplicate article slug: {}", new_slug.unwrap()))
        ])
    })
    .map_err(|e| match e {
        Error::UnprocessableEntity{ .. } => e,
        _ => Error::Forbidden
    })?
    .into_article();

    Ok(Json(ArticleBody { article }))
}

async fn delete_article(
    state: State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(slug): Path<String>,
) -> Result<()> {
    let result = sqlx::query!(
        //language=PostgreSQL
        r#"
            with deleted_article as (
                delete from article 
                where slug = $1 and user_id = $2
                returning 1
            )
            select
                exists(select 1 from article where slug = $1) "existed!",
                exists(select 1 from deleted_article) "deleted!"
        "#,
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

async fn get_article(
    state: State<AppState>,
    Extension(maybe_claims): Extension<Option<Claims>>,
    Path(slug): Path<String>,
) -> Result<Json<ArticleBody>> {
    let article = sqlx::query_as!(
        ArticleFromQuery,
        // language=PostgreSQL
        r#"
            select
                slug,
                title,
                description,
                body,
                tag_list,
                article.created_at,
                article.updated_at,
                exists(select 1 from article_favorite where article_id = article.article_id and user_id = $1) "favorited!",
                (select count(*) from article_favorite fav where fav.article_id = article.article_id) "favorites_count!",
                author.username author_username,
                author.bio author_bio,
                author.image author_image,
                exists(select 1 from follow where followed_user_id = author.user_id and following_user_id = $1) "following_author!"
            from article
            inner join "user" author using (user_id)
            where slug = $2
        "#,
        maybe_claims.as_ref().map(|claims| claims.sub),
        slug
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(Error::NotFound)?
    .into_article();

    Ok(Json(ArticleBody { article }))
}

async fn favorite_article(
    state: State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(slug): Path<String>,
) -> Result<Json<ArticleBody>> {
    let article_id = sqlx::query_scalar!(
        // language=PostgreSQL
        r#"
            with selected_article as (
                select article_id from article where slug = $1
            ),
            inserted_favorite as (
                insert into article_favorite(article_id, user_id)
                select article_id, $2
                from selected_article
                on conflict do nothing
            )
            select article_id from selected_article
        "#,
        slug,
        claims.sub
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(Error::NotFound)?;

    Ok(Json(ArticleBody {
        article: article_by_id(&state.db, claims.sub, article_id).await?,
    }))
}

async fn unfavorite_article(
    state: State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(slug): Path<String>,
) -> Result<Json<ArticleBody>> {
    let article_id = sqlx::query_scalar!(
        // language=PostgreSQL
        r#"
            with selected_article as (
                select article_id from article where slug = $1
            ),
            inserted_favorite as (
                delete from article_favorite
                where article_id = (select article_id from selected_article)
                and user_id = $2
            )
            select article_id from selected_article
        "#,
        slug,
        claims.sub
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(Error::NotFound)?;

    Ok(Json(ArticleBody {
        article: article_by_id(&state.db, claims.sub, article_id).await?,
    }))
}

async fn get_tags(state: State<AppState>) -> Result<Json<TagsBody>> {
        let tags = sqlx::query_scalar!(
        // language=PostgreSQL
        r#"
            select distinct tag "tag!"
            from article, unnest (article.tag_list) tags(tag)
            order by tag;
        "#
    )
    .fetch_all(&state.db)
    .await?;

    Ok(Json(TagsBody { tags }))
}

async fn article_by_id(
    e: impl Executor<'_, Database = Postgres>,
    user_id: Uuid,
    article_id: Uuid,
) -> Result<Article> {
    let article = sqlx::query_as!(
        ArticleFromQuery,
        // language=PostgreSQL
        r#"
            select
                slug,
                title,
                description,
                body,
                tag_list,
                article.created_at,
                article.updated_at,
                exists(select 1 from article_favorite where article_id = article.article_id and user_id = $1) "favorited!",
                (select count(*) from article_favorite fav where fav.article_id = article.article_id) "favorites_count!",
                author.username author_username,
                author.bio author_bio,
                author.image author_image,
                exists(select 1 from follow where followed_user_id = author.user_id and following_user_id = $1) "following_author!"
            from article
            inner join "user" author using (user_id)
            where article_id = $2
        "#,
        user_id,
        article_id
    )
    .fetch_optional(e)
    .await?
    .ok_or(Error::NotFound)?
    .into_article();

    Ok(article)
}

fn slugify(title: &str) -> String {
    title
        .to_ascii_lowercase()
        .chars()
        .map(|c| match c {
            'a'..='z' | '0'..='9' => c,
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            '\'' | '\\' => '\0',
            _ => ' ',
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}
