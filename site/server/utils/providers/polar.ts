import { createHmac, timingSafeEqual } from 'node:crypto'

import type { LicenseCheck, LicenseProvider, ProviderConfig, WebhookEvent } from '../providers'

/**
 * Polar.
 *
 * Its licence-key API is JSON rather than form-encoded, and it calls an
 * activation an "activation" rather than an "instance", but the shape is the
 * same. Docs: https://docs.polar.sh/api-reference
 */
const API = 'https://api.polar.sh/v1/customer-portal/license-keys'

export function polar(config: ProviderConfig): LicenseProvider {
  return {
    name: 'polar',

    async activate(licenseKey, instanceId) {
      return request('activate', { key: licenseKey, organization_id: config.storeId, label: instanceId })
    },

    async validate(licenseKey, instanceId) {
      return request('validate', { key: licenseKey, organization_id: config.storeId, activation_id: instanceId })
    },

    async verifyWebhook(rawBody, signature, secret) {
      if (!signature || !secret) return false
      const expected = createHmac('sha256', secret).update(rawBody).digest('base64')
      const given = Buffer.from(signature, 'utf8')
      const computed = Buffer.from(expected, 'utf8')
      if (given.length !== computed.length) return false
      return timingSafeEqual(given, computed)
    },

    parseWebhook(payload) {
      return parsePolarWebhook(payload)
    },
  }

  async function request(
    action: 'activate' | 'validate',
    body: Record<string, string>,
  ): Promise<LicenseCheck> {
    try {
      const response = await fetch(`${API}/${action}`, {
        method: 'POST',
        headers: {
          Accept: 'application/json',
          'Content-Type': 'application/json',
          ...(config.apiKey ? { Authorization: `Bearer ${config.apiKey}` } : {}),
        },
        body: JSON.stringify(body),
      })
      const data = (await response.json().catch(() => null)) as PolarResponse | null
      return interpretPolar(data, response.ok)
    } catch (error) {
      return { valid: false, plan: 'free', reason: `provider unreachable: ${error}` }
    }
  }
}

interface PolarResponse {
  status?: string
  limit_activations?: number | null
  usage?: number
  expires_at?: string | null
  detail?: string
}

/** Turn a Polar answer into ours. Exported for testing. */
export function interpretPolar(data: PolarResponse | null, ok: boolean): LicenseCheck {
  if (!data) {
    return { valid: false, plan: 'free', reason: 'the provider sent an unreadable response' }
  }
  if (!ok || (data.status && data.status !== 'granted')) {
    return {
      valid: false,
      plan: 'free',
      reason: data.detail ?? `the licence is ${data.status ?? 'not valid'}`,
    }
  }

  const expiresAt = data.expires_at
    ? Math.floor(new Date(data.expires_at).getTime() / 1000)
    : undefined
  if (expiresAt && expiresAt * 1000 < Date.now()) {
    return { valid: false, plan: 'free', reason: 'the licence has expired' }
  }

  return {
    valid: true,
    plan: 'pro',
    expiresAt,
    seats:
      data.limit_activations == null
        ? undefined
        : { limit: data.limit_activations, used: data.usage ?? 0 },
  }
}

/** Map a Polar webhook onto our event vocabulary. Exported for testing. */
export function parsePolarWebhook(payload: unknown): WebhookEvent | null {
  if (typeof payload !== 'object' || payload === null) return null
  const name = (payload as { type?: string }).type
  if (!name) return null

  const revoking = ['order.refunded', 'subscription.canceled', 'subscription.revoked']
  const kind: WebhookEvent['kind'] = name.includes('refund')
    ? 'refunded'
    : name.includes('cancel')
      ? 'cancelled'
      : name.includes('revoked')
        ? 'cancelled'
        : name.includes('created') || name.includes('active')
          ? 'renewed'
          : 'unknown'

  return { kind, revoked: revoking.includes(name) }
}
