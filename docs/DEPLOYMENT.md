# Deployment guide

Everything runs on free tiers: the web app on Vercel (Hobby), the Rust server on Render (free web service, Docker image), the database on MongoDB Atlas (free cluster). GitHub Actions builds and tests everything, then deploys.

Free-tier limits change. Check them before your demo:
- Render free web services sleep after 15 minutes without traffic and take about a minute to wake; WebSocket messages on an open connection count as traffic; 750 free instance hours per workspace per month (enough for one service running all month); bandwidth and build minutes are capped. https://render.com/docs/free
- Atlas free cluster: 0.5 GB storage, 500 connections, 100 operations per second, one free cluster per project, paused after 30 days with no connections. https://www.mongodb.com/docs/atlas/reference/free-shared-limitations/
- Vercel Hobby is for personal, non-commercial use, which covers a college project.

## 1. Topology

```
Browser ──https──> Vercel (Next.js)  ──rewrite /api/*──>  Render (Rust server) ──> MongoDB Atlas
   └──────────────── wss://<api>.onrender.com/ws/v1/... (ticket) ──────────┘
```

REST goes through the Vercel rewrite so the login cookie is first-party. WebSockets go straight to Render with a 30-second ticket (ADR-006).

## 2. Accounts and one-time setup

1. GitHub repository `syncspace` (private or public). Add the three team members.
2. MongoDB Atlas account, Render account, Vercel account. Sign up to Render and Vercel with GitHub.
3. Pick regions close to India: Atlas on AWS Mumbai (`ap-south-1`) if offered for free clusters, otherwise Singapore; Render Singapore.

## 3. MongoDB Atlas

1. Create a project `syncspace`, then a **Free** cluster (`syncspace-cluster`).
2. **Database Access:** add user `syncspace_app` with a generated password and role "Read and write to any database" (or a custom role limited to the `syncspace` database).
3. **Network Access:** Render free services do not have fixed outbound IPs, so add `0.0.0.0/0`. This is why the strong generated password matters. Never commit the URI.
4. **Connect > Drivers:** copy the `mongodb+srv://` URI. Add the database name as the `MONGODB_DB` env var (`syncspace`) instead of in the URI.
5. Keep the connection pool small: `maxPoolSize=10` in the URI.
6. The server creates indexes at start-up. Check them in Atlas under Collections > Indexes after the first deploy.
7. Optional: create a second free project and cluster for staging, or use a separate database name (`syncspace_staging`) in the same cluster.

**MongoDB version: 8.0 everywhere.** Atlas documents that free clusters run MongoDB 8.0 ("Atlas Free Cluster Limits"), that you cannot upgrade a free cluster yourself, and that Atlas moves free clusters to the newest version only after several patch releases of it. So local development, the CI service container (`mongo:8.0`) and `infrastructure/docker-compose.yml` (`mongo:8.0`) all use 8.0, and `mongodb_major_version_matches_production` in `crates/server/tests/db_indexes.rs` fails when a test run uses another major version. The Rust driver we pin (`mongodb` 3.10) is documented as fully compatible with 7.0, 8.0 and 9.0, so the driver is not what limits the choice. **When Atlas moves your cluster to a newer major version, bump all of these in one change**: `EXPECTED_MONGODB_MAJOR` in `crates/server/tests/common/mod.rs`, the CI image, the compose image, the local install, this paragraph and the version record in `docs/PROGRESS.md`.

## 4. CI/CD with GitHub Actions

Three workflows in `.github/workflows/`, plus `nightly-proptest.yml` (a scheduled, non-blocking run of the property tests that opens an issue on failure; see `docs/TESTING.md` section 4).

### ci.yml (every push and PR)
The block below is the **target** workflow once every phase has landed. The file in `.github/workflows/ci.yml` is the source of truth for what runs today; it grows gate by gate (see "Current state" under the block).
```yaml
name: ci
on: [push, pull_request]
jobs:
  rust:
    runs-on: ubuntu-latest
    services:
      mongo:
        image: mongo:8.0
        ports: ["27017:27017"]
    env:
      MONGODB_URI: mongodb://localhost:27017
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with: { components: "rustfmt, clippy", targets: "wasm32-unknown-unknown" }
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --all --check
      - run: cargo clippy --workspace --all-targets -- -D warnings
      - run: cargo test --workspace
      - run: cargo install wasm-pack --locked   # or cache a binary
      - run: wasm-pack test --node crates/sync-crdt
      - run: cargo install cargo-llvm-cov --locked
      - run: cargo llvm-cov -p sync-crdt --fail-under-lines 90
      - run: cargo llvm-cov -p server --fail-under-lines 75
  web:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with: { targets: "wasm32-unknown-unknown" }
      - uses: pnpm/action-setup@v4
      - uses: actions/setup-node@v4
        with: { node-version: 22, cache: pnpm, cache-dependency-path: apps/web/pnpm-lock.yaml }
      - run: cargo install wasm-pack --locked
      - run: pnpm --dir apps/web install --frozen-lockfile
      - run: pnpm --dir apps/web wasm
      - run: pnpm --dir apps/web lint && pnpm --dir apps/web typecheck && pnpm --dir apps/web test --coverage
      - run: cargo run -p server --bin gen-types && git diff --exit-code apps/web/src/types/generated
  e2e:
    needs: [rust, web]
    runs-on: ubuntu-latest
    services:
      mongo: { image: "mongo:8.0", ports: ["27017:27017"] }
    steps:
      - uses: actions/checkout@v4
      # build server and web, start both in the background, wait for /healthz, then:
      - run: pnpm --dir e2e install --frozen-lockfile && pnpm --dir e2e exec playwright install --with-deps chromium firefox
      - run: pnpm --dir e2e test
```
Pin action versions to the latest major at implementation time.

**Current state (tasks 0.2 and 0.3).** Actions are pinned to `actions/checkout@v7`, `actions/setup-node@v7`, `pnpm/action-setup@v6`, `Swatinem/rust-cache@v2` and `actions/upload-artifact@v7`. There is no root `package.json`, so `pnpm/action-setup` is given `package_json_file: apps/web/package.json` and takes the pnpm version from `packageManager`. `push` runs only on `main` and `pull_request` covers every PR, so a PR is not tested twice. wasm-pack (`0.15.1`) and the Rust toolchain (`1.100.0`, also set in `rust-toolchain.toml`) are pinned; bump them together on purpose. The workflow sets `permissions: contents: read`, cancels superseded runs per ref and has a timeout on every job. Running today: fmt, clippy, `cargo test` (failing tests are annotated on the run), the hostile-input tests of `sync-crdt` again in release mode, `wasm-pack test --node`, the server integration tests against a `mongo:8.0` service container, and the line-coverage gates (`cargo llvm-cov`: `sync-crdt` 90%, `server` 75%) (the Wasm half of the parity test), web lint, typecheck and test after `pnpm --dir apps/web wasm`, e2e typecheck and the Playwright tests. wasm-pack comes from `taiki-e/install-action@v2`. The `web` and `e2e` jobs install the Rust toolchain with the wasm target because the app imports the generated package. The `e2e` job starts the throwaway spike relay and `next dev` through the Playwright `webServer` list. Left as `# TODO` lines in the file until their task lands: web `--coverage`, the generated-types diff, and the MongoDB service with the API start-up for `e2e` (Phase 1).

`.gitattributes` forces LF line endings so `prettier --check` gives the same result on Windows and on the runner.

### deploy-api.yml (after CI passes on `main`; a pull request that touches the image only builds it)
1. Build the Docker image (below) with `docker/build-push-action`, using GitHub Actions cache.
2. Push to GitHub Container Registry: `ghcr.io/<owner>/syncspace-api:<git-sha>` and `:latest`.
3. Trigger Render: `curl -fsS -X POST "$RENDER_DEPLOY_HOOK?imgURL=ghcr.io/<owner>/syncspace-api:<git-sha>"` (store the hook URL as the secret `RENDER_DEPLOY_HOOK`).
4. Poll `https://<api>.onrender.com/healthz` until it returns 200 (allow 3 minutes).

Building in Actions avoids Render's free build limits; Rust builds are heavy.

### deploy-web.yml (after CI passes on `main`)
Vercel's build machines have no Rust toolchain, so build the Wasm package in Actions and deploy a prebuilt output:
```yaml
      - run: pnpm --dir apps/web wasm
      - run: npm i -g vercel
      - run: vercel pull --yes --environment=production --token=${{ secrets.VERCEL_TOKEN }}
        working-directory: apps/web
      - run: vercel build --prod --token=${{ secrets.VERCEL_TOKEN }}
        working-directory: apps/web
      - run: vercel deploy --prebuilt --prod --token=${{ secrets.VERCEL_TOKEN }}
        working-directory: apps/web
```
Secrets: `VERCEL_TOKEN`, `VERCEL_ORG_ID`, `VERCEL_PROJECT_ID` (set the last two as env for the job).

## 5. Environment variables

### Server (Render)
| Name | Example | Notes |
|---|---|---|
| `PORT` | `10000` | Render sets it; the server must bind `0.0.0.0:$PORT` |
| `MONGODB_URI` | `mongodb+srv://syncspace_app:...@.../?retryWrites=true&w=majority&maxPoolSize=10` | secret |
| `MONGODB_DB` | `syncspace` | |
| `JWT_SECRET` | 32+ random bytes, base64 | secret; `openssl rand -base64 32` |
| `TICKET_SECRET` | 32+ random bytes, base64 | secret, different from `JWT_SECRET` |
| `ALLOWED_ORIGINS` | `https://syncspace.vercel.app` | comma separated; used for CORS and the WebSocket Origin check |
| `COOKIE_SECURE` | `true` | `false` only for local http |
| `SNAPSHOT_INTERVAL_SECS` | `5` | |
| `METRICS_TOKEN` | unset | Bearer token (16+ characters) for `GET /metrics`. Unset: the endpoint answers 404. Set it on the API service and give the same value to whatever scrapes it |
| `GITHUB_CLIENT_ID` | `Ov23li...` | optional: sign in with GitHub (ADR-015). Set it together with the next two or none of them |
| `GITHUB_CLIENT_SECRET` | | secret, from the GitHub OAuth App |
| `PUBLIC_WEB_URL` | `https://syncspace-amber.vercel.app` | the exact web origin; the callback URL registered with GitHub is `<PUBLIC_WEB_URL>/api/v1/auth/github/callback` |
| `GITHUB_OAUTH_URL`, `GITHUB_API_URL` | `https://github.com`, `https://api.github.com` | only changed by tests (a fake GitHub) |
| `VERSION_INTERVAL_SECS` | `600` | |
| `AUTO_VERSION_RETENTION` | `50` | |
| `RUST_LOG` | `info,server=info` | |

#### Setting up sign in with GitHub (optional)
1. GitHub, Settings, Developer settings, OAuth Apps, **New OAuth App**. Application name `SyncSpace`, Homepage URL the web origin, **Authorization callback URL** `<web origin>/api/v1/auth/github/callback` (exactly, no trailing slash).
2. Generate a client secret. Put `GITHUB_CLIENT_ID`, `GITHUB_CLIENT_SECRET` and `PUBLIC_WEB_URL` into the Render service environment and redeploy. Never commit them.
3. Open the login page: "Continue with GitHub" appears. To switch the feature off, remove all three variables.

The app asks GitHub for `read:user user:email` only. It stores no GitHub token; the public repositories are read by the browser from `api.github.com`.

### Web (Vercel, Production)
| Name | Example | Notes |
|---|---|---|
| `API_ORIGIN` | `https://syncspace-api.onrender.com` | used by the rewrite only (server side) |
| `NEXT_PUBLIC_WS_URL` | `wss://syncspace-api.onrender.com` | the browser connects here |

`apps/web/next.config.ts`:
```ts
const nextConfig = {
  async rewrites() {
    return [{ source: "/api/:path*", destination: `${process.env.API_ORIGIN}/api/:path*` }];
  },
};
export default nextConfig;
```

## 6. Render

1. **New > Web Service > Existing image**: `ghcr.io/<owner>/syncspace-api:latest`. If the package is private, add a registry credential (a GitHub personal access token with `read:packages`); or make the package public.
2. Name `syncspace-api`, region Singapore, instance type **Free**.
3. Health check path `/healthz`.
4. Add the server environment variables from §5.
5. Copy the **Deploy Hook** URL into the GitHub secret `RENDER_DEPLOY_HOOK`.
6. Graceful shutdown: the server handles SIGTERM by flushing snapshots (ARCHITECTURE §3.3). Keep the flush under Render's shutdown grace period.

Dockerfile (`crates/server/Dockerfile`, built from the repo root):
```dockerfile
FROM rust:1-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json -p server
COPY . .
RUN cargo build --release -p server

FROM gcr.io/distroless/cc-debian12
COPY --from=builder /app/target/release/server /server
ENV RUST_LOG=info
EXPOSE 10000
USER nonroot
ENTRYPOINT ["/server"]
```

## 7. Vercel

1. **Add New > Project**, import the GitHub repo, root directory `apps/web`, framework Next.js.
2. Settings > Git: turn off automatic production deploys (deploys come from `deploy-web.yml`), or set "Ignored Build Step" to `exit 0`. Keep preview deploys off too, because they would build without the Wasm package.
3. Add the web environment variables from §5. `apps/web/vercel.json` makes Vercel run only `pnpm install` and `next build`, and `apps/web/.vercelignore` makes a CLI upload include the gitignored Wasm package (`src/wasm/pkg`, made by `pnpm --dir apps/web wasm`). A remote build from the laptop is `vercel deploy --prod` in `apps/web` after the Wasm step; do not use `--prebuilt` from Windows (its symlinks are not uploaded).
4. Create a token (Account Settings > Tokens) for `VERCEL_TOKEN`; run `vercel link` once locally in `apps/web` to get `VERCEL_ORG_ID` and `VERCEL_PROJECT_ID` from `.vercel/project.json`.
5. Put the final Vercel URL in the server's `ALLOWED_ORIGINS` and redeploy the server.

## 8. Smoke test after every production deploy

- [ ] `GET https://<api>/healthz` returns `{"status":"ok","mongo":"ok"}`
- [ ] Register and log in on the Vercel URL; the cookie is set on the Vercel domain
- [ ] Create a workspace and a document; type; reload; the text is still there
- [ ] Open the document in a second browser (or a private window as another user added as Editor); edits and cursors appear live both ways
- [ ] Viewer cannot type; Admin demotes an Editor live
- [ ] Save a version, edit, restore; the other window updates live
- [ ] Create and move a task; the other window sees it move
- [ ] Audit trail lists the actions; export JSON works
- [ ] Browser console has no CORS or WebSocket errors

## 9. Demo day checklist

- Open the API URL and the app 5 minutes before presenting so the Render service is awake. The open WebSocket keeps it awake during the demo.
- Run the seed script against the production database the day before, so the screens look like the Figma frames: `MONGODB_URI="<the Atlas URI>" MONGODB_DB=syncspace cargo run -p server --bin seed -- --workspace "Engineering Team"` (it reads only these two variables, from the environment or a `.env`; no server secrets). It creates the workspace `engineering-team` with three accounts (`vibhav@syncspace.demo` ADMIN and owner, `sparsh@syncspace.demo` EDITOR, `suyash@syncspace.demo` VIEWER), three documents with two saved versions each, a board "Sprint 12" with nine tasks in the five columns and the audit rows those actions leave. It **prints a random password once** for the new accounts (or use `--password` or `SEED_PASSWORD`): write it down. Running it again for the same workspace changes nothing and says so; a second workspace name reuses the three accounts and keeps their passwords.
- Have the load-test numbers and a short screen recording ready in case the network fails.
- Log in as the three team members in three browser profiles.

## 10. Local development

You need MongoDB **8.0** on `localhost:27017` (see the version note in section 3), either way:

- **Docker:** `docker compose -f infrastructure/docker-compose.yml up -d mongo`.
- **Native (Windows):** install MongoDB Community Server 8.0 (`winget install MongoDB.Server --version 8.0.x`, or the MSI). It runs as the `MongoDB` service without authentication. Check with `Get-Service MongoDB` and `mongod --version`.

```bash
cp .env.example .env              # then fill JWT_SECRET and TICKET_SECRET (below); keep COOKIE_SECURE=false
cargo run -p server               # http://localhost:8080, GET /healthz
pnpm --dir apps/web wasm && pnpm --dir apps/web dev   # http://localhost:3000, API_ORIGIN=http://localhost:8080
```

Generate the two secrets (they must be different, base64, at least 32 bytes decoded):

```bash
node -e "console.log(require('crypto').randomBytes(32).toString('base64'))"   # run it twice
# or: openssl rand -base64 32
```

The server reads `.env` from the current directory (or a parent) and then the real environment, and **refuses to start** when something is wrong: it prints every problem at once and exits with code 2 for configuration, or 1 when MongoDB cannot be reached or the indexes cannot be created.

**Tests.** `cargo test --workspace` runs the server integration tests against the MongoDB named by `MONGODB_URI` (default `mongodb://localhost:27017`). Each test uses its own database `syncspace_test_<uuid>` and drops it afterwards, so nothing is left behind and tests run in parallel. Without a reachable MongoDB the Mongo-backed tests fail with instructions; the others do not need one. No test starts containers itself (ADR-011): they use the MongoDB at `MONGODB_URI`, so a native install and the compose service work equally.
## 11. Troubleshooting

| Symptom | Likely cause and fix |
|---|---|
| First request takes about a minute | Render free instance was asleep; expected |
| Login works but every request then returns 401 | Cookie set on the API domain instead of the web domain: call REST through `/api/*` on the web domain, not the Render URL |
| WebSocket closes immediately with 4001 | Ticket expired or reused; get a fresh ticket per connection attempt |
| The WebSocket handshake fails with HTTP 403 | `Origin` missing or not in `ALLOWED_ORIGINS` (include the exact Vercel URL, no trailing slash) |
| The server exits at once with "the configuration has problems" (exit code 2) | A required variable is missing or invalid; the message lists every problem. Secrets must be base64 of at least 32 bytes and `JWT_SECRET` must differ from `TICKET_SECRET` |
| The server exits with "stopped with an error" (exit code 1) | MongoDB is not reachable (not running, wrong URI, Atlas network access) or index creation failed; the log has the cause |
| `/healthz` answers 503 `{"status":"degraded","mongo":"error"}` | The server is up but cannot reach MongoDB; the server log has the driver error |
| Server tests fail with "These tests need a running MongoDB" | Start MongoDB 8.0 (section 10) or set `MONGODB_URI` |
| `mongodb_major_version_matches_production` fails | The tests ran on another MongoDB major version than Atlas free clusters run; install 8.0 (or bump everything together, see section 3) |
| `MongoServerSelectionError` in Render logs | Atlas network access missing `0.0.0.0/0`, wrong password, or the cluster is paused |
| Wasm import fails on Vercel | The `pkg` folder was not built before `vercel build`; deploy through `deploy-web.yml` |
| Atlas storage warning | Lower `AUTO_VERSION_RETENTION`, or delete demo documents (versions are deleted with them) |
| Free services suspended | Monthly free hours or bandwidth used up; check the Render billing page |
