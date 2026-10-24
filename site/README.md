# crawlstat.com

Nuxt 3 on Vercel. Two jobs: the marketing site and docs (Phase 6), and the
licence API (this phase).

```
site/
  server/
    api/
      license/activate.post.ts    activate a key, return a signed token
      license/validate.post.ts    re-check, return a fresh token
      webhooks/payments.post.ts   refunds and cancellations
      updates/latest.json.get.ts  what the newest release is
    utils/
      token.ts                    Ed25519 signing
      providers.ts                the LicenseProvider interface
      providers/                  Lemon Squeezy, Polar, Paddle
      ratelimit.ts                per-IP and per-key limits
  test/                           vitest, no server needed
```

```bash
npm install
npm run dev
npm test
```

## How licensing works without a database

The payment provider is the source of truth for who has paid. This API is a
thin proxy in front of it that signs a short-lived **entitlement token** with an
Ed25519 key. The app embeds the public key and verifies tokens itself.

```
  app ──activate(key)──▶ /api/license/activate ──▶ merchant of record
                                  │
                         signs an entitlement
                                  ▼
  app ◀────── token (14 days) ────┘        app verifies offline, forever after
```

Which buys three things:

- **No database to run.** No licence table, no backups, no migrations, nothing
  to pay for. The provider already stores all of it.
- **The app works offline.** A verified token is proof on its own, so a crawl
  never waits on this API — or fails because of it.
- **Tiny function usage.** Each install calls `validate` about once a day. Ten
  thousand customers is a few hundred thousand invocations a month, well inside
  the free allowance, and `latest.json` is edge-cached so it barely counts.

The cost is a **revocation window**: a refund takes effect at the install's next
validation, up to a day later, or up to a fortnight if the machine is offline
the whole time. Closing that would mean a revocation list, which means a
database. For a tool at this price that is the wrong trade, so the window is
accepted deliberately rather than by accident. The webhook endpoint exists to
verify signatures and to be the place a revocation list plugs into the day it is
justified.

### Swapping providers

[`server/utils/providers.ts`](server/utils/providers.ts) is the interface;
`providers/` holds one file per merchant of record. Lemon Squeezy and Polar both
have a licence-key API; Paddle does not, so its implementation looks up a
subscription instead — which is exactly the difference the interface exists to
absorb. Switching is one environment variable.

Every implementation must **fail closed**: a provider that cannot be reached
answers `valid: false`, never "probably fine". The app's grace period, not a
hopeful server, is what protects a paying customer during an outage.

### Setting up the signing key

```bash
openssl genpkey -algorithm ed25519 -out signing.pem
# the value to embed in the app (engine/src/license.rs):
openssl pkey -in signing.pem -pubout -outform DER | tail -c 32 | xxd -p -c 32
# the value for Vercel:
awk 'BEGIN{ORS="\\n"} {print}' signing.pem
```

Put the PEM in `NUXT_LICENSE_SIGNING_KEY` and the hex public key in
`SIGNING_PUBLIC_KEY_HEX`. If those two ever disagree, **every** activation
fails — so `engine/tests/license_contract.rs` verifies a token this code
actually signed, and fails the build if the two sides' formats drift.

Regenerate that fixture after changing anything in `token.ts`:

```bash
npx tsx test/make-fixture.ts
```

### Rate limiting, honestly

[`ratelimit.ts`](server/utils/ratelimit.ts) is an in-memory sliding window, held
per serverless instance. Vercel runs several and recycles them, so a determined
attacker spread across instances gets a higher effective rate than the configured
numbers. What it does reliably defend against is the realistic failure — an app
stuck in a retry loop, or a script hammering one key, turning into a bill. The
upgrade, when the numbers justify it, is Upstash Redis behind the same interface.

## Not yet built

The marketing site, the docs and the download page are Phase 6. The audit docs
pages will be generated from `engine/src/audit/catalog.rs`, so the fix text in
the app and the text on the site cannot drift.
