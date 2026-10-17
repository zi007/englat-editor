# Publishing the browser version (Cloudflare R2)

The optimised browser build is about 40 MiB (`.wasm`), about 15 MiB gzip-compressed (measured
with `trunk build --release`, wasm-opt -Oz). Cloudflare Pages and Workers static assets have a
25 MiB per-file limit, so the files are served from an R2 bucket instead.

## One-time setup (Cloudflare dashboard)
1. R2 → create a bucket, for example `englat-editor-web`.
2. Bucket → Settings → Custom Domains → add `editor.englat.com` (the zone must be in the same
   Cloudflare account). R2 serves objects by key, so make `/` serve `index.html` (for example with a
   Transform Rule that rewrites the path `/` to `/index.html`); check this in your account.
3. My Profile → API Tokens → create a token with R2 edit permission for that bucket.
4. GitHub → this repository → Settings → Secrets and variables → Actions: add
   `CLOUDFLARE_API_TOKEN`, `CLOUDFLARE_ACCOUNT_ID` and `R2_BUCKET`.

## Publish
GitHub → Actions → `deploy-web` → Run workflow. It builds with trunk and uploads the files.
Hashed `.js`/`.wasm` files are cached for a year; `index.html` is not cached.

## Before linking from englat.com
Open the page on a phone over mobile data and measure the first load. A ~15 MiB first download is
heavy on slow connections; repeat visits use the browser cache.
