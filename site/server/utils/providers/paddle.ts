import { createHmac, timingSafeEqual } from 'node:crypto'

import type { LicenseCheck, LicenseProvider, ProviderConfig, WebhookEvent } from '../providers'

/**
 * Paddle Billing.
 *
 * Paddle has no licence-key product of its own: a key is a custom field on a
 * subscription, so validation is a subscription lookup rather than a licence
 * check. That is why this implementation looks least like the other two —
 * which is the reason the `LicenseProvider` interface exists.
 *
 * Docs: https://developer.paddle.com/api-reference/overview
 */
const API = 'https://api.paddle.com'

export function paddle(config: ProviderConfig): LicenseProvider {
  return {
    name: 'paddle',

    async activate(licenseKey, instanceId) {
      // Paddle does not track activations, so activation and validation are
      // the same call. Seat counting would need a database, which this design
      // deliberately avoids until it is actually needed.
      return this.validate(licenseKey, instanceId)
    },

    async validate(licenseKey) {
      try {
        const response = await fetch(
          `${API}/subscriptions?status=active&per_page=1&` +
            new URLSearchParams({ customer_id: '', id: licenseKey }).toString(),
          {
            headers: {
              Accept: 'application/json',
              Authorization: `Bearer ${config.apiKey}`,
            },
          },
        )
        const data = (await response.json().catch(() => null)) as PaddleResponse | null
        return interpretPaddle(data, response.ok)
      } catch (error) {
        return { valid: false, plan: 'free', reason: `provider unreachable: ${error}` }
      }
    },

    async verifyWebhook(rawBody, signature, secret) {
      // Paddle sends `ts=…;h1=…`, and signs the timestamp with the body.
      if (!signature || !secret) return false
      const parts = Object.fromEntries(
        signature.split(';').map((part) => part.split('=') as [string, string]),
      )
      if (!parts.ts || !parts.h1) return false

      const expected = createHmac('sha256', secret)
        .update(`${parts.ts}:${rawBody}`)
        .digest('hex')
      const given = Buffer.from(parts.h1, 'utf8')
      const computed = Buffer.from(expected, 'utf8')
      if (given.length !== computed.length) return false
      return timingSafeEqual(given, computed)
    },

    parseWebhook(payload) {
      return parsePaddleWebhook(payload)
    },
  }
}

interface PaddleResponse {
  data?: Array<{ status?: string; current_billing_period?: { ends_at?: string } }>
  error?: { detail?: string }
}

/** Turn a Paddle answer into ours. Exported for testing. */
export function interpretPaddle(data: PaddleResponse | null, ok: boolean): LicenseCheck {
  if (!data || !ok) {
    return {
      valid: false,
      plan: 'free',
      reason: data?.error?.detail ?? 'that licence key was not recognised',
    }
  }
  const subscription = data.data?.[0]
  if (!subscription || subscription.status !== 'active') {
    return { valid: false, plan: 'free', reason: 'no active subscription for that key' }
  }

  const endsAt = subscription.current_billing_period?.ends_at
  return {
    valid: true,
    plan: 'pro',
    expiresAt: endsAt ? Math.floor(new Date(endsAt).getTime() / 1000) : undefined,
  }
}

/** Map a Paddle webhook onto our event vocabulary. Exported for testing. */
export function parsePaddleWebhook(payload: unknown): WebhookEvent | null {
  if (typeof payload !== 'object' || payload === null) return null
  const name = (payload as { event_type?: string }).event_type
  if (!name) return null

  const revoking = ['subscription.canceled', 'adjustment.created', 'transaction.payment_failed']
  const kind: WebhookEvent['kind'] = name.includes('adjustment')
    ? 'refunded'
    : name.includes('canceled')
      ? 'cancelled'
      : name.includes('past_due')
        ? 'expired'
        : name.includes('created') || name.includes('activated')
          ? 'renewed'
          : 'unknown'

  return { kind, revoked: revoking.includes(name) }
}
