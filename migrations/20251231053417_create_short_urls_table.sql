-- Add migration script here
CREATE TABLE short_urls (
    id BIGSERIAL PRIMARY KEY,
    original_url TEXT NOT NULL UNIQUE,
    short_code VARCHAR(10) NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);