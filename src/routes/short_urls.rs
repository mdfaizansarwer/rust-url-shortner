use actix_web::{
    HttpResponse, http,
    web::{self},
};
use chrono::Utc;
use sqlx::PgPool;

#[derive(serde::Deserialize, Debug)]
pub struct GenerateShortUrlRequest {
    url: String,
}

#[tracing::instrument(name = "Navigate to long URL", skip(short_code, connection_pool))]
pub async fn navigate_to_long_url(
    short_code: web::Path<String>,
    connection_pool: web::Data<PgPool>,
) -> HttpResponse {
    match fetch_long_url(short_code.into_inner(), connection_pool).await {
        Some(original_url) => {
            tracing::info!("Redirecting to original URL: {}", original_url);
            // Navigate to the original URL with 301 redirect
            HttpResponse::Found()
                .append_header(("Location", original_url))
                .status(http::StatusCode::PERMANENT_REDIRECT)
                .finish()
        }
        None => HttpResponse::NotFound().body("Short URL not found."),
    }
}

#[tracing::instrument(name = "Generate short URL", skip(body, connection_pool))]
pub async fn generate_short_url(
    body: web::Json<GenerateShortUrlRequest>,
    connection_pool: web::Data<PgPool>,
) -> HttpResponse {
    if is_valid(&body) == false {
        tracing::error!("Invalid URL format: {}", body.url);
        return HttpResponse::BadRequest().body("Invalid URL format.");
    }
    // Fetch long URL if it already exists
    if let Some(existing_short_code) =
        fetch_short_code(body.url.clone(), connection_pool.clone()).await
    {
        tracing::info!(
            "URL already exists. Returning existing short code: {}.",
            existing_short_code
        );
        return HttpResponse::Ok()
            .json(serde_json::json!({ "short_url": format!("/{}" ,existing_short_code) }));
    }
    let short_code = generate_short_code(&connection_pool).await;
    if short_code.is_err() {
        tracing::error!("Failed to generate short code: {:?}", short_code.err());
        return HttpResponse::InternalServerError().finish();
    }
    // Insert new URL and generate short code
    match insert_new_url(
        body.url.clone(),
        short_code.as_ref().unwrap(),
        &connection_pool,
    )
    .await
    {
        Ok(_) => HttpResponse::Ok()
            .json(serde_json::json!({ "short_url": format!("/{}", short_code.unwrap()) })),
        Err(e) => {
            eprintln!("Failed to execute query: {}", e);
            HttpResponse::InternalServerError().finish()
        }
    }
}
#[tracing::instrument(
    name = "Generate a short code for the given URL",
    skip(connection_pool)
)]
async fn generate_short_code(connection_pool: &web::Data<PgPool>) -> Result<String, sqlx::Error> {
    match sqlx::query!(
        r#"
        SELECT id FROM short_urls ORDER BY created_at DESC LIMIT 1
        "#
    )
    .fetch_one(connection_pool.get_ref())
    .await
    {
        Ok(record) => {
            let id = record.id + 1;
            let allowed_chars = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
            let base = allowed_chars.len() as u64;
            let mut num = id as u64;
            let mut short_code = Vec::new();
            while num > 0 {
                let rem = (num % base) as usize;
                short_code.push(allowed_chars[rem]);
                num /= base;
            }
            Ok(String::from_utf8(short_code).unwrap_or_else(|_| "a".to_string()))
        }
        Err(e) => {
            if e.to_string().contains("no rows returned") {
                tracing::info!("No existing records found, starting with ID 1.");
                return Ok("a".to_string());
            }
            Err(e)
        }
    }
}

#[tracing::instrument(
    name = "Fetch fetch_long_url for the given short code",
    skip(connection_pool)
)]
pub async fn fetch_long_url(
    short_code: String,
    connection_pool: web::Data<PgPool>,
) -> Option<String> {
    let result = sqlx::query!(
        r#"
        SELECT original_url FROM short_urls WHERE short_code = $1
        "#,
        short_code
    )
    .fetch_one(connection_pool.get_ref())
    .await;

    match result {
        Ok(record) => Some(record.original_url),
        Err(_) => None,
    }
}

#[tracing::instrument(
    name = "Fetch short code for the given long URL",
    skip(connection_pool)
)]
pub async fn fetch_short_code(
    long_url: String,
    connection_pool: web::Data<PgPool>,
) -> Option<String> {
    let is_url_present = sqlx::query!(
        r#"
        SELECT short_code FROM short_urls WHERE original_url = $1
        "#,
        long_url
    )
    .fetch_one(connection_pool.get_ref())
    .await;
    match is_url_present {
        Ok(record) => Some(record.short_code),
        Err(_) => None,
    }
}

#[tracing::instrument(name = "Insert new URL and generate short code", skip(connection_pool))]
async fn insert_new_url(
    long_url: String,
    short_code: &String,
    connection_pool: &web::Data<PgPool>,
) -> Result<(), sqlx::Error> {
    // insert the URL into the database

    match sqlx::query!(
        r#"
        INSERT INTO short_urls (original_url, short_code, created_at) 
        VALUES ($1, $2, $3)
        "#,
        long_url,
        short_code,
        Utc::now()
    )
    .execute(connection_pool.get_ref())
    .await
    {
        Ok(_) => Ok(()),
        Err(e) => Err(e),
    }
}

fn is_valid(body: &GenerateShortUrlRequest) -> bool {
    return !body.url.is_empty()
        && (body.url.starts_with("http://") || body.url.starts_with("https://"));
}
