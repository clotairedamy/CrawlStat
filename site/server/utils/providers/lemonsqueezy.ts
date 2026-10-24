import { createHmac, timingSafeEqual } from 'node:crypto'

import type {
  ActivationResult,
  LicenseCheck,
  LicenseProvider,
  ProviderConfig,
  WebhookEvent,
} from '../providers'

/**
 * Lemon Squeezy, the default provider.
 *
 * Its licence API is a plain form POST and needs no SDK, which keeps the
 * function's cold start small — worth caring about when the app calls this on
 * launch.
 *
 * Docs: https://docs.lemonsqueezy.com/api/license-api
 */
const API = 'https://api.lemonsqueezy.com/v1/licenses'

export function lemonSqueezy(config: ProviderConfig): LicenseProvider {
  return {
    name: 'lemonsqueezy',

    async activate(licenseKey, instanceId) {
      return request('activate', { license_key: licenseKey, instance_name: instanceId })
    },

    async validate(licenseKey, instanceId) {
      return request('validate', { license_key: licenseKey, instance_id: instanceId })
    },

    async verifyWebhook(rawBody, signature, secret) {
      if (!signature || !secret) return false
      const expected = createHmac('sha256', secret).update(rawBody).digest('hex')
      const given = Buffer.from(signature, 'utf8')
      const computed = Buffer.from(expected, 'utf8')
      // Length check first: timingSafeEqual throws on a mismatch.
      if (given.length !== computed.length) return false
      return timingSafeEqual(given, computed)
    },

    parseWebhook(payload) {
      return parseLemonSqueezyWebhook(payload)
    },
  }

  async function request(
    action: 'activate' | 'validate',
    body: Record<string, string>,
  ): Promise<ActivationResult> {
    let response: Response
    try {
      response = await fetch(`${API}/${action}`, {
        method: 'POST',
        headers: {
          Accept: 'application/json',
          'Content-Type': 'application/x-www-form-urlencoded',
          ...(config.apiKey ? { Authorization: `Bearer ${config.apiKey}` } : {}),
        },
        body: new URLSearchParams(body).toString(),
      })
    } catch (error) {
      // Fail closed: a provider we cannot reach must not grant anything.
      // The app's grace period is what protects the paying customer here.
      return { valid: false, plan: 'free', reason: `provider unreachable: ${error}` }
    }

    if (!response.ok && response.status !== 400 && response.status !== 404) {
      return { valid: false, plan: 'free', reason: `provider error ${response.status}` }
    }

    const data = (await response.json().catch(() => null)) as LemonSqueezyResponse | null
    return interpretLemonSqueezy(data)
  }
}

interface LemonSqueezyResponse {
  activated?: boolean
  valid?: boolean
  error?: string | null
  license_key?: {
    status?: string
    activation_limit?: number
    activation_usage?: number
    expires_at?: string | null
  }
  meta?: { store_id?: number }
}

/**
 * Turn a Lemon Squeezy answer into ours.
 *
 * Exported for testing: this mapping is where a provider's vocabulary becomes
 * our entitlement, and getting it wrong either refuses paying customers or
 * grants free ones.
 */
export function interpretLemonSqueezy(data: LemonSqueezyResponse | null): ActivationResult {
  if (!data) {
    return { valid: false, plan: 'free', reason: 'the provider sent an unreadable response' }
  }

  const ok = data.activated === true || data.valid === true
  if (!ok) {
    return {
      valid: false,
      plan: 'free',
      reason: data.error ?? 'that licence key was not recognised',
    }
  }

  const license = data.license_key ?? {}
  // A key can be valid but no longer entitled: disabled after a refund, or
  // expired at the end of a subscription.
  if (license.status && !['active', 'inactive'].includes(license.status)) {
    return { valid: false, plan: 'free', reason: `the licence is ${license.status}` }
  }

  const expiresAt = license.expires_at
    ? Math.floor(new Date(license.expires_at).getTime() / 1000)
    : undefined
  if (expiresAt && expiresAt * 1000 < Date.now()) {
    return { valid: false, plan: 'free', reason: 'the licence has expired' }
  }

  return {
    valid: true,
    plan: 'pro',
    expiresAt,
    seats:
      license.activation_limit === undefined
        ? undefined
        : { limit: license.activation_limit, used: license.activation_usage ?? 0 },
  }
}

/** Map a Lemon Squeezy webhook onto our event vocabulary. */
export function parseLemonSqueezyWebhook(payload: unknown): WebhookEvent | null {
  if (typeof payload !== 'object' || payload === null) return null
  const event = payload as { meta?: { event_name?: string } }
  const name = event.meta?.event_name
  if (!name) return null

  // Anything that takes money back, or stops it arriving, revokes the licence.
  const revoking = [
    'order_refunded',
    'subscription_cancelled',
    'subscription_expired',
    'license_key_updated',
  ]

  const kind: WebhookEvent['kind'] = name.includes('refund')
    ? 'refunded'
    : name.includes('cancel')
      ? 'cancelled'
      : name.includes('expired')
        ? 'expired'
        : name.includes('created') || name.includes('payment_success')
          ? 'renewed'
          : 'unknown'

  return { kind, revoked: revoking.some((candidate) => name === candidate) }
}
