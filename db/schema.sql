-- Raw fetched sources (dedup by URL)
CREATE TABLE IF NOT EXISTS news_sources (
  id           INTEGER PRIMARY KEY,
  fetched_at   TEXT NOT NULL DEFAULT (datetime('now')),
  url          TEXT NOT NULL UNIQUE,
  title        TEXT,
  outlet       TEXT,
  author       TEXT,
  published_at TEXT,
  text         TEXT NOT NULL,          -- cleaned article text
  sha256       BLOB NOT NULL           -- 32-byte hash of 'text' for change detect
);

-- One row per day for your published brief
CREATE TABLE IF NOT EXISTS news_digests (
  id           INTEGER PRIMARY KEY,
  digest_date  TEXT NOT NULL UNIQUE,   -- 'YYYY-MM-DD'
  html         TEXT NOT NULL,          -- rendered HTML we’ll serve
  json         TEXT NOT NULL,          -- exact JSON (bullets, links, ai_view, etc.)
  item_count   INTEGER NOT NULL,
  created_at   TEXT NOT NULL DEFAULT (datetime('now')),
  deleted      INTEGER NOT NULL DEFAULT 0 -- soft delete (0=false, 1=true)
);

-- Daily node metrics (your “Bitvia metric of the day” comes from here)
CREATE TABLE IF NOT EXISTS metrics (
  id                         INTEGER PRIMARY KEY,
  metric_date                TEXT NOT NULL UNIQUE, -- 'YYYY-MM-DD'
  mempool_tx                 INTEGER,
  mempool_bytes              INTEGER,
  avg_block_interval_sec     REAL,
  median_fee_sat_per_vb      REAL,
  created_at                 TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Helpful indexes
CREATE INDEX IF NOT EXISTS idx_sources_fetched_at ON news_sources(fetched_at);
CREATE INDEX IF NOT EXISTS idx_sources_published_at ON news_sources(published_at);
CREATE INDEX IF NOT EXISTS idx_digests_created_at ON news_digests(created_at);
