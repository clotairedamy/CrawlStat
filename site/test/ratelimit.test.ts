import { beforeEach, describe, expect, it } from 'vitest'

import {
  ACTIVATION_LIMIT,
  VALIDATION_LIMIT,
  checkRateLimit,
  clientAddress,
  resetRateLimits,
} from '../server/utils/ratelimit'

describe('checkRateLimit', () => {
  beforeEach(resetRateLimits)

  it('allows up to the limit and then refuses', () => {
    const limit = { limit: 3, windowSeconds: 60 }
    for (let attempt = 1; attempt <= 3; attempt += 1) {
      const result = checkRateLimit('bucket', limit, 1000)
      expect(result.allowed).toBe(true)
      expect(result.remaining).toBe(3 - attempt)
    }

    const refused = checkRateLimit('bucket', limit, 1000)
    expect(refused.allowed).toBe(false)
    expect(refused.remaining).toBe(0)
    expect(refused.resetAfter).toBeGreaterThan(0)
  })

  it('keeps buckets separate', () => {
    const limit = { limit: 1, windowSeconds: 60 }
    expect(checkRateLimit('one', limit, 1000).allowed).toBe(true)
    expect(checkRateLimit('one', limit, 1000).allowed).toBe(false)
    // A different key, or a different IP, is unaffected.
    expect(checkRateLimit('two', limit, 1000).allowed).toBe(true)
  })

  it('lets the window slide rather than resetting on a fixed boundary', () => {
    const limit = { limit: 2, windowSeconds: 10 }
    checkRateLimit('slide', limit, 0)
    checkRateLimit('slide', limit, 5_000)
    expect(checkRateLimit('slide', limit, 6_000).allowed).toBe(false)

    // The first hit has aged out, so one slot frees up — but not both.
    expect(checkRateLimit('slide', limit, 11_000).allowed).toBe(true)
    expect(checkRateLimit('slide', limit, 11_000).allowed).toBe(false)
  })

  it('says how long to wait, which is what the app needs to back off', () => {
    const limit = { limit: 1, windowSeconds: 60 }
    checkRateLimit('wait', limit, 0)
    const refused = checkRateLimit('wait', limit, 30_000)
    expect(refused.resetAfter).toBe(30)
  })

  it('has limits that leave room for retries but not for a loop', () => {
    // Validation happens about once a day per install; twenty an hour allows
    // several installs behind one IP and a few retries, and nothing more.
    expect(VALIDATION_LIMIT.limit).toBeGreaterThanOrEqual(10)
    expect(VALIDATION_LIMIT.limit).toBeLessThanOrEqual(60)
    expect(VALIDATION_LIMIT.windowSeconds).toBe(3600)
    // Activation is rare and deliberate, so it is tighter.
    expect(ACTIVATION_LIMIT.limit).toBeLessThan(VALIDATION_LIMIT.limit)
  })
})

describe('clientAddress', () => {
  it('takes the original client from x-forwarded-for', () => {
    expect(clientAddress({ 'x-forwarded-for': '203.0.113.7, 10.0.0.1, 10.0.0.2' })).toBe(
      '203.0.113.7',
    )
    expect(clientAddress({ 'x-forwarded-for': '203.0.113.7' })).toBe('203.0.113.7')
  })

  it('falls back through the other proxy headers', () => {
    expect(clientAddress({ 'x-real-ip': '198.51.100.4' })).toBe('198.51.100.4')
    expect(clientAddress({ 'x-vercel-forwarded-for': '198.51.100.5' })).toBe('198.51.100.5')
  })

  it('shares one bucket when there is no usable address', () => {
    // Not a per-request bucket: that would hand an unlimited rate to anyone
    // who strips the headers.
    expect(clientAddress({})).toBe('unknown')
    expect(clientAddress({ 'x-forwarded-for': '' })).toBe('unknown')
  })
})
