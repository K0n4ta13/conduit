use super::{Article, ArticleFromQuery, Claims, Result};
use crate::http::AppState;
use axum::extract::{Query, State};
use axum::{Extension, Json};
use futures::TryStreamExt;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct ListArticlesQuery {
    tag: Option<String>,
    author: Option<String>,
    favorited: Option<String>,
    cursor: Option<OffsetDateTime>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
pub struct FeedArticlesQuery {
    cursor: Option<OffsetDateTime>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MultipleArticlesBody {
    articles: Vec<Article>,
    articles_count: usize,
}

pub(super) async fn list_articles(
    state: State<AppState>,
    Extension(maybe_claims): Extension<Option<Claims>>,
    query: Query<ListArticlesQuery>,
) -> Result<Json<MultipleArticlesBody>> {
    let articles: Vec<_> = sqlx::query_as!(
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
            where (
                $2::timestamptz is NULL or $2 > article.created_at
            )
            and (
                $3::text is null or tag_list @> array[$3]
            )
            and (
                $4::text is null or author.username = $4
            )
            and (
                $5::text is null or exists(
                    select 1
                    from "user"
                    inner join article_favorite using (user_id)
                    where username = $5
                )
            )
            order by article.created_at desc
            limit 20;
        "#,
        maybe_claims.as_ref().map(|claims| claims.sub),
        query.cursor,
        query.tag,
        query.author,
        query.favorited
    )
    .fetch(&state.db)
    .map_ok(ArticleFromQuery::into_article)
    .try_collect()
    .await?;

    Ok(Json(MultipleArticlesBody {
        articles_count: articles.len(),
        articles,
    }))
}

pub(super) async fn feed_articles(
    state: State<AppState>,
    Extension(claims): Extension<Claims>,
    query: Query<FeedArticlesQuery>,
) -> Result<Json<MultipleArticlesBody>> {
    let articles: Vec<_> = sqlx::query_as!(
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
                true "following_author!"
            from follow
            inner join article on followed_user_id = article.user_id
            inner join "user" author using (user_id)
            where (
                following_user_id = $1
            ) and (
                $2::timestamptz is NULL or $2 > article.created_at
            )
            limit 20
        "#,
        claims.sub,
        query.cursor
    )
    .fetch(&state.db)
    .map_ok(ArticleFromQuery::into_article)
    .try_collect()
    .await?;

    Ok(Json(MultipleArticlesBody {
        articles_count: articles.len(),
        articles,
    }))
}
