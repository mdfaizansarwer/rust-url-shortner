use actix_web::{HttpResponse, web};
use chrono::Utc;
use sqlx::PgPool;

use crate::configuration::Settings;

#[derive(serde::Deserialize, Debug)]
pub struct GenerateShortUrlRequest {
    url: String,
}

pub async fn generate_short_url(
    body: web::Json<GenerateShortUrlRequest>,
    connection_pool: web::Data<PgPool>,
    settings: web::Data<Settings>,
) -> HttpResponse {
    println!("Received URL to shorten: {}", body.url);
    if body.url.is_empty() {
        return HttpResponse::BadRequest().body("URL cannot be empty");
    }
    if !body.url.starts_with("http://") && !body.url.starts_with("https://") {
        return HttpResponse::BadRequest().body("URL must start with http:// or https://");
    }

    let is_url_present = sqlx::query!(
        r#"
        SELECT short_code FROM short_urls WHERE original_url = $1
        "#,
        &body.url
    )
    .fetch_one(connection_pool.get_ref())
    .await;

    if let Ok(record) = is_url_present {
        return HttpResponse::Ok().json(
            serde_json::json!({ "short_code": format!("{}/{}", settings.domain, record.short_code) }),
        );
    }

    // insert the URL into the database and generate a short code
    let short_code = generate_short_code(&connection_pool).await;
    match sqlx::query!(
        r#"
        INSERT INTO short_urls (original_url, short_code, created_at) 
        VALUES ($1, $2, $3)
        "#,
        body.url,
        short_code,
        Utc::now()
    )
    .execute(connection_pool.get_ref())
    .await
    {
        Ok(_) => HttpResponse::Ok().json(
            serde_json::json!({ "short_url": format!("{}/{}", settings.domain, short_code) }),
        ),
        Err(e) => {
            eprintln!("Failed to execute query: {}", e);
            HttpResponse::InternalServerError().finish()
        }
    }
}

async fn generate_short_code(connection_pool: &web::Data<PgPool>) -> String {
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
            String::from_utf8(short_code).unwrap_or_else(|_| "a".to_string())
        }
        Err(_) => "a".to_string(), // start from 'a' if no records exist
    }
}
