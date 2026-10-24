import { createHmac } from 'node:crypto'
import { describe, expect, it } from 'vitest'

import { getProvider } from '../server/utils/providers'
import {
  interpretLemonSqueezy,
  lemonSqueezy,
  parseLemonSqueezyWebhook,
} from '../server/utils/providers/lemonsqueezy'
import { interpretPolar, parsePolarWebhook } from '../server/utils/providers/polar'
import { interpretPaddle, parsePaddleWebhook } from '../server/utils/providers/paddle'

const config = { provider: 'lemonsqueezy', apiKey: 'test', storeId: '1' }

describe('choosing a provider', () => {
  it('loads each supported provider', async () => {
    for (const provider of ['lemonsqueezy', 'polar', 'paddle']) {
      const loaded = await getProvider({ ...config, provider })
      expect(loaded.name).toBe(provider)
    }
  })

  it('refuses an unknown provider rather than silently falling back', async () => {
    // A typo in the environment variable must be loud: falling back would
    // refuse every real customer's licence instead.
    await expect(getProvider({ ...config, provider: 'stripe' })).rejects.toThrow(
      /Unknown licence provider/,
    )
  })
})

describe('Lemon Squeezy responses', () => {
  it('grants Pro for an activated key', () => {
    const result = interpretLemonSqueezy({
      activated: true,
      license_key: { status: 'active', activation_limit: 3, activation_usage: 1 },
    })
    expect(result).toMatchObject({ valid: true, plan: 'pro', seats: { limit: 3, used: 1 } })
  })

  it('grants Pro for a validated key', () => {
    expect(interpretLemonSqueezy({ valid: true, license_key: { status: 'active' } })).toMatchObject(
      { valid: true, plan: 'pro' },
    )
  })

  it('passes the provider’s reason through for a bad key', () => {
    const result = interpretLemonSqueezy({ valid: false, error: 'license_key not found' })
    expect(result.valid).toBe(false)
    expect(result.reason).toBe('license_key not found')
  })

  it('refuses a key that is valid but no longer entitled', () => {
    // A refunded licence stays "valid" at the provider but is disabled.
    for (const status of ['disabled', 'expired']) {
      const result = interpretLemonSqueezy({ valid: true, license_key: { status } })
      expect(result.valid).toBe(false)
      expect(result.reason).toContain(status)
    }
  })

  it('refuses a licence whose subscription has already ended', () => {
    const result = interpretLemonSqueezy({
      valid: true,
      license_key: { status: 'active', expires_at: '2020-01-01T00:00:00Z' },
    })
    expect(result.valid).toBe(false)
    expect(result.reason).toContain('expired')
  })

  it('carries a future expiry through', () => {
    const future = new Date(Date.now() + 30 * 86400_000).toISOString()
    const result = interpretLemonSqueezy({
      valid: true,
      license_key: { status: 'active', expires_at: future },
    })
    expect(result.valid).toBe(true)
    expect(result.expiresAt).toBeGreaterThan(Math.floor(Date.now() / 1000))
  })

  it('fails closed on an unreadable response', () => {
    // A provider outage must never grant a licence.
    const result = interpretLemonSqueezy(null)
    expect(result.valid).toBe(false)
    expect(result.plan).toBe('free')
  })

  it('treats an inactive-but-unused key as grantable', () => {
    // Lemon Squeezy marks a key "inactive" until its first activation.
    expect(interpretLemonSqueezy({ activated: true, license_key: { status: 'inactive' } }).valid)
      .toBe(true)
  })
})

describe('Lemon Squeezy webhooks', () => {
  const provider = lemonSqueezy(config)

  it('accepts a correctly signed body', async () => {
    const body = '{"meta":{"event_name":"order_created"}}'
    const signature = createHmac('sha256', 'secret').update(body).digest('hex')
    await expect(provider.verifyWebhook(body, signature, 'secret')).resolves.toBe(true)
  })

  it('rejects a wrong signature, a wrong secret and a missing one', async () => {
    const body = '{"meta":{"event_name":"order_created"}}'
    const signature = createHmac('sha256', 'secret').update(body).digest('hex')

    await expect(provider.verifyWebhook(body, 'deadbeef', 'secret')).resolves.toBe(false)
    await expect(provider.verifyWebhook(body, signature, 'wrong')).resolves.toBe(false)
    await expect(provider.verifyWebhook(body, '', 'secret')).resolves.toBe(false)
    await expect(provider.verifyWebhook(body, signature, '')).resolves.toBe(false)
    // A body edited after signing.
    await expect(provider.verifyWebhook(`${body} `, signature, 'secret')).resolves.toBe(false)
  })

  it('maps the events that should revoke a licence', () => {
    expect(parseLemonSqueezyWebhook({ meta: { event_name: 'order_refunded' } })).toEqual({
      kind: 'refunded',
      revoked: true,
    })
    expect(parseLemonSqueezyWebhook({ meta: { event_name: 'subscription_cancelled' } })).toEqual({
      kind: 'cancelled',
      revoked: true,
    })
    expect(parseLemonSqueezyWebhook({ meta: { event_name: 'subscription_expired' } })).toEqual({
      kind: 'expired',
      revoked: true,
    })
  })

  it('does not revoke on a renewal', () => {
    const event = parseLemonSqueezyWebhook({ meta: { event_name: 'subscription_payment_success' } })
    expect(event?.revoked).toBe(false)
  })

  it('returns null for something that is not a webhook', () => {
    expect(parseLemonSqueezyWebhook(null)).toBeNull()
    expect(parseLemonSqueezyWebhook({})).toBeNull()
    expect(parseLemonSqueezyWebhook('nope')).toBeNull()
  })
})

describe('Polar responses', () => {
  it('grants Pro for a granted key', () => {
    expect(interpretPolar({ status: 'granted', limit_activations: 5, usage: 2 }, true))
      .toMatchObject({ valid: true, plan: 'pro', seats: { limit: 5, used: 2 } })
  })

  it('refuses anything other than granted', () => {
    expect(interpretPolar({ status: 'revoked' }, true).valid).toBe(false)
    expect(interpretPolar({ detail: 'not found' }, false).reason).toBe('not found')
    expect(interpretPolar(null, true).valid).toBe(false)
  })

  it('treats an unlimited activation count as unlimited, not as zero', () => {
    const result = interpretPolar({ status: 'granted', limit_activations: null }, true)
    expect(result.valid).toBe(true)
    expect(result.seats).toBeUndefined()
  })

  it('maps its revoking events', () => {
    expect(parsePolarWebhook({ type: 'order.refunded' })?.revoked).toBe(true)
    expect(parsePolarWebhook({ type: 'subscription.revoked' })?.revoked).toBe(true)
    expect(parsePolarWebhook({ type: 'subscription.active' })?.revoked).toBe(false)
    expect(parsePolarWebhook({})).toBeNull()
  })
})

describe('Paddle responses', () => {
  it('grants Pro for an active subscription', () => {
    const result = interpretPaddle(
      { data: [{ status: 'active', current_billing_period: { ends_at: '2030-01-01T00:00:00Z' } }] },
      true,
    )
    expect(result).toMatchObject({ valid: true, plan: 'pro' })
    expect(result.expiresAt).toBeGreaterThan(0)
  })

  it('refuses when there is no active subscription', () => {
    expect(interpretPaddle({ data: [] }, true).valid).toBe(false)
    expect(interpretPaddle({ data: [{ status: 'past_due' }] }, true).valid).toBe(false)
    expect(interpretPaddle({ error: { detail: 'not found' } }, false).reason).toBe('not found')
  })

  it('maps its revoking events', () => {
    expect(parsePaddleWebhook({ event_type: 'subscription.canceled' })?.revoked).toBe(true)
    expect(parsePaddleWebhook({ event_type: 'adjustment.created' })?.kind).toBe('refunded')
    expect(parsePaddleWebhook({ event_type: 'subscription.created' })?.revoked).toBe(false)
  })

  it('verifies its timestamped signature format', async () => {
    const provider = (await getProvider({ ...config, provider: 'paddle' }))
    const body = '{"event_type":"subscription.created"}'
    const ts = '1800000000'
    const h1 = createHmac('sha256', 'secret').update(`${ts}:${body}`).digest('hex')

    await expect(provider.verifyWebhook(body, `ts=${ts};h1=${h1}`, 'secret')).resolves.toBe(true)
    // A replayed body with a different timestamp does not verify.
    await expect(provider.verifyWebhook(body, `ts=999;h1=${h1}`, 'secret')).resolves.toBe(false)
    await expect(provider.verifyWebhook(body, 'nonsense', 'secret')).resolves.toBe(false)
  })
})
