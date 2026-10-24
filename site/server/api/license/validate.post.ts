import { buildEntitlement, signEntitlement } from '../../utils/token'
import { getProvider } from '../../utils/providers'
import {
  VALIDATION_LIMIT,
  checkRateLimit,
  clientAddress,
} from '../../utils/ratelimit'

/**
 * `POST /api/license/validate`
 *
 * Re-checks a licence and issues a fresh token. The app calls this about once
 * a day; its cached token covers it for a fortnight, so a failure here is never
 * urgent and never interrupts work.
 */
export default defineEventHandler(async (event) => {
  const body = await readBody<{ licenseKey?: string; instanceId?: string }>(event)
  const licenseKey = body?.licenseKey?.trim()
  const instanceId = body?.instanceId?.trim()

  if (!licenseKey || !instanceId) {
    throw createError({
      statusCode: 400,
      statusMessage: 'licenseKey and instanceId are both required',
    })
  }

  const address = clientAddress(getHeaders(event) as Record<string, string | undefined>)
  for (const bucket of [`validate:ip:${address}`, `validate:key:${licenseKey}`]) {
    const limit = checkRateLimit(bucket, VALIDATION_LIMIT)
    if (!limit.allowed) {
      setResponseHeader(event, 'retry-after', String(limit.resetAfter))
      // 429 rather than a refusal: the app must back off and keep using its
      // cached token, not conclude the licence is invalid.
      throw createError({ statusCode: 429, statusMessage: 'Rate limited. Retry later.' })
    }
  }

  const config = useRuntimeConfig(event)
  const provider = await getProvider({
    provider: config.licenseProvider,
    apiKey: config.providerApiKey,
    storeId: config.providerStoreId,
  })

  const result = await provider.validate(licenseKey, instanceId)
  if (!result.valid) {
    return { valid: false, reason: result.reason ?? 'this licence is no longer valid' }
  }

  const entitlement = buildEntitlement({
    plan: result.plan,
    licenseKey,
    instanceId,
    expiresAt: result.expiresAt,
  })

  return {
    valid: true,
    token: signEntitlement(entitlement, config.licenseSigningKey),
    plan: entitlement.plan,
    expiresAt: entitlement.expires_at,
  }
})
