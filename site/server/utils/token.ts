import { createPrivateKey, createPublicKey, sign } from 'node:crypto'
import { createHash } from 'node:crypto'

/**
 * Signing entitlement tokens.
 *
 * The app verifies these offline with an embedded Ed25519 public key, which is
 * what lets CrawlStat have no licence database and no runtime dependency on
 * this API. The private key lives only in Vercel's environment.
 *
 * The wire format is `base64url(payload).base64url(signature)`, and the
 * signature covers the *encoded* payload string rather than the JSON — so
 * re-serialising the JSON on either side can never change what was signed.
 * `engine/src/license.rs` is the other half of this contract.
 */

/** How long a token is valid without being refreshed. Mirrors the engine. */
export const GRACE_PERIOD_DAYS = 14

/** Token format version. Bump only for a breaking change to the payload. */
export const TOKEN_VERSION = 1

export type Plan = 'free' | 'pro'

/** Pro capabilities, matching `engine/src/license.rs`. */
export const PRO_FEATURES = [
  'unlimited_urls',
  'crawl_comparison',
  'custom_extraction',
  'scheduled_crawls',
  'xlsx_export',
  'html_report',
  'byok_integrations',
] as const

export interface Entitlement {
  version: number
  plan: Plan
  issued_at: number
  expires_at: number
  key_fingerprint: string
  instance_id: string
  features: string[]
}

/** Base64url without padding, matching Rust's `URL_SAFE_NO_PAD`. */
export function base64url(input: Buffer | string): string {
  return Buffer.from(input).toString('base64url')
}

/**
 * A licence key's fingerprint: the first eight characters of its BLAKE3 hash.
 *
 * Node has no BLAKE3, so this uses SHA-256 — the engine only ever *displays*
 * the fingerprint and compares it to itself, never recomputes it from the key,
 * so the two sides do not need the same hash. What matters is that it is
 * stable, short, and not reversible into the key.
 */
export function fingerprint(licenseKey: string): string {
  return createHash('sha256').update(licenseKey.trim()).digest('hex').slice(0, 8)
}

/** Build the entitlement for a licence the provider says is valid. */
export function buildEntitlement(options: {
  plan: Plan
  licenseKey: string
  instanceId: string
  now?: number
  features?: string[]
  /** Overrides the grace period, for a licence that ends sooner than that. */
  expiresAt?: number
}): Entitlement {
  const issuedAt = options.now ?? Math.floor(Date.now() / 1000)
  const graceExpiry = issuedAt + GRACE_PERIOD_DAYS * 86400

  return {
    version: TOKEN_VERSION,
    plan: options.plan,
    issued_at: issuedAt,
    // Never issue a token that outlives the subscription it is based on: a
    // licence cancelled tomorrow must not keep working for a fortnight.
    expires_at: options.expiresAt ? Math.min(graceExpiry, options.expiresAt) : graceExpiry,
    key_fingerprint: fingerprint(options.licenseKey),
    instance_id: options.instanceId,
    features: options.features ?? (options.plan === 'pro' ? [...PRO_FEATURES] : []),
  }
}

/**
 * Sign an entitlement.
 *
 * `privateKeyPem` is an Ed25519 key in PKCS#8 PEM form. Newlines may arrive
 * escaped, which is how they survive most environment-variable editors.
 */
export function signEntitlement(entitlement: Entitlement, privateKeyPem: string): string {
  const payload = base64url(JSON.stringify(entitlement))
  const key = createPrivateKey({
    key: privateKeyPem.replace(/\\n/g, '\n'),
    format: 'pem',
  })
  const signature = sign(null, Buffer.from(payload), key)
  return `${payload}.${base64url(signature)}`
}

/**
 * The public key matching a private one, hex-encoded.
 *
 * Used by the setup script so the value embedded in the app provably matches
 * the key signing tokens — a mismatch there rejects every licence, and is
 * miserable to debug after the fact.
 */
export function publicKeyHex(privateKeyPem: string): string {
  const key = createPublicKey({
    key: privateKeyPem.replace(/\\n/g, '\n'),
    format: 'pem',
  })
  // The last 32 bytes of the DER encoding are the raw Ed25519 public key.
  const der = key.export({ type: 'spki', format: 'der' })
  return Buffer.from(der.subarray(der.length - 32)).toString('hex')
}
