/**
 * Rate limiting, per IP and per licence key.
 *
 * ## What this is and is not
 *
 * An in-memory sliding window, held per serverless instance. That is
 * deliberately modest, and the limits of it are worth stating plainly:
 * Vercel runs several instances and recycles them, so a determined attacker
 * spread across instances gets a higher effective rate than the numbers below.
 *
 * It is still worth having. What it actually defends against is the realistic
 * failure: an app stuck in a retry loop, or a script hammering one key, turning
 * into a bill. For that, a per-instance counter is enough, because one client's
 * requests mostly land on one warm instance.
 *
 * The upgrade path, when the numbers justify it, is Upstash Redis — a free tier
 * and a shared window across instances. The interface below does not change.
 */

export interface RateLimitResult {
  allowed: boolean
  /** Requests left in the window. */
  remaining: number
  /** Seconds until the window resets. */
  resetAfter: number
}

interface Window {
  /** Request timestamps in milliseconds, oldest first. */
  hits: number[]
}

const windows = new Map<string, Window>()

/** Entries kept before the map is pruned, so a long-lived instance cannot grow. */
const MAX_TRACKED_KEYS = 10_000

export interface RateLimit {
  /** Requests allowed per window. */
  limit: number
  /** Window length in seconds. */
  windowSeconds: number
}

/** Activation is rare and deliberate: a handful per hour is generous. */
export const ACTIVATION_LIMIT: RateLimit = { limit: 10, windowSeconds: 3600 }

/**
 * Validation happens about once a day per install, so twenty an hour allows
 * for retries and several installs behind one IP without allowing a loop.
 */
export const VALIDATION_LIMIT: RateLimit = { limit: 20, windowSeconds: 3600 }

/**
 * Check and record a request against a bucket.
 *
 * `now` is injectable so the tests do not have to sleep.
 */
export function checkRateLimit(
  bucket: string,
  limit: RateLimit,
  now: number = Date.now(),
): RateLimitResult {
  const windowMs = limit.windowSeconds * 1000
  const existing = windows.get(bucket) ?? { hits: [] }

  // Drop anything that has fallen out of the window.
  const hits = existing.hits.filter((at) => now - at < windowMs)

  if (hits.length >= limit.limit) {
    const oldest = hits[0]!
    return {
      allowed: false,
      remaining: 0,
      resetAfter: Math.max(1, Math.ceil((oldest + windowMs - now) / 1000)),
    }
  }

  hits.push(now)
  windows.set(bucket, { hits })
  pruneIfLarge(now)

  return {
    allowed: true,
    remaining: limit.limit - hits.length,
    resetAfter: limit.windowSeconds,
  }
}

/**
 * Forget buckets whose windows have all expired.
 *
 * Only when the map is large, so the common case costs nothing.
 */
function pruneIfLarge(now: number): void {
  if (windows.size <= MAX_TRACKED_KEYS) return
  for (const [bucket, window] of windows) {
    // A day is longer than any window here, so this is safe for all of them.
    if (window.hits.every((at) => now - at > 86_400_000)) {
      windows.delete(bucket)
    }
  }
  // Still too big: drop the oldest half rather than grow without bound.
  if (windows.size > MAX_TRACKED_KEYS) {
    const keys = [...windows.keys()].slice(0, Math.floor(windows.size / 2))
    for (const key of keys) windows.delete(key)
  }
}

/** For tests: start from an empty map. */
export function resetRateLimits(): void {
  windows.clear()
}

/**
 * The client's address, from the proxy headers Vercel sets.
 *
 * Falls back to a constant rather than to something spoofable, so a request
 * with no usable address shares one bucket instead of getting an unlimited one
 * each.
 */
export function clientAddress(headers: Record<string, string | undefined>): string {
  const forwarded = headers['x-forwarded-for']
  if (forwarded) {
    // The left-most entry is the original client; the rest are proxies.
    const first = forwarded.split(',')[0]?.trim()
    if (first) return first
  }
  return headers['x-real-ip'] ?? headers['x-vercel-forwarded-for'] ?? 'unknown'
}
