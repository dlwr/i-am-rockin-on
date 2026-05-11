import { defineConfig } from "@playwright/test";
import { execSync } from "node:child_process";
import { existsSync, unlinkSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = resolve(__dirname, "../..");
const DB_ABS = `${REPO_ROOT}/tests/visual/visual-test.db`;
const SEED_PATH = `${__dirname}/seed.sql`;

const PORT = 3030;
const BASE_URL = `http://127.0.0.1:${PORT}`;

// Prepare DB *before* defineConfig returns — Playwright's globalSetup runs in
// parallel with webServer, so seeding inside globalSetup races the binary
// startup. Running here guarantees the DB is migrated + seeded before any
// process touches it.
for (const suffix of ["", "-wal", "-shm", "-journal"]) {
  const p = DB_ABS + suffix;
  if (existsSync(p)) unlinkSync(p);
}
execSync("sqlx database create && sqlx migrate run", {
  cwd: REPO_ROOT,
  env: { ...process.env, DATABASE_URL: `sqlite:${DB_ABS}` },
  stdio: "inherit",
});
execSync(`sqlite3 "${DB_ABS}" < "${SEED_PATH}"`, { stdio: "inherit" });

export default defineConfig({
  testDir: ".",
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: process.env.CI ? "github" : "list",
  use: {
    baseURL: BASE_URL,
    trace: "retain-on-failure",
  },
  webServer: {
    // Run the already-built binary directly so cargo-leptos isn't in the loop.
    command: `${REPO_ROOT}/target/debug/i-am-rockin-on`,
    cwd: REPO_ROOT,
    url: BASE_URL,
    reuseExistingServer: !process.env.CI,
    timeout: 600_000,
    stdout: "pipe",
    stderr: "pipe",
    env: {
      ...(process.env as Record<string, string>),
      DATABASE_URL: `sqlite:${DB_ABS}`,
      LEPTOS_SITE_ADDR: `127.0.0.1:${PORT}`,
      LEPTOS_SITE_ROOT: "target/site",
      LEPTOS_SITE_PKG_DIR: "pkg",
      LEPTOS_OUTPUT_NAME: "i-am-rockin-on",
      SQLX_OFFLINE: "true",
      DISABLE_SCRAPE: "1",
      RUST_LOG: "error",
      // Spotify creds are required by Config::from_env but never used here
      // because DISABLE_SCRAPE=1 keeps the resolver dormant.
      SPOTIFY_CLIENT_ID: "visual-test",
      SPOTIFY_CLIENT_SECRET: "visual-test",
    },
  },
});
