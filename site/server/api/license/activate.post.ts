import { buildEntitlement, signEntitlement } from '../../utils/token'
import { getProvider } from '../../utils/providers'
import {
  ACTIVATION_LIMIT,
  checkRateLimit,
  clientAddress,
} from '../../utils/ratelimit'

/**
 * `POST /api/license/activate`
 *
 * Registers an installation against a licence key and returns a signed
 * entitlement token the app can verify offline for the next fortnight.
 *
 * Called once, when the user pastes their key. Everything after that is
 * `validate`.
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

  // Two buckets: one stops a single machine looping, the other stops one key
  // being tried from many machines.
  const address = clientAddress(getHeaders(event) as Record<string, string | undefined>)
  for (const bucket of [`activate:ip:${address}`, `activate:key:${licenseKey}`]) {
    const limit = checkRateLimit(bucket, ACTIVATION_LIMIT)
    if (!limit.allowed) {
      setResponseHeader(event, 'retry-after', String(limit.resetAfter))
      throw createError({
        statusCode: 429,
        statusMessage: 'Too many activation attempts. Try again shortly.',
      })
    }
  }

  const config = useRuntimeConfig(event)
  const provider = await getProvider({
    provider: config.licenseProvider,
    apiKey: config.providerApiKey,
    storeId: config.providerStoreId,
  })

  const result = await provider.activate(licenseKey, instanceId)
  if (!result.valid) {
    // 200 with `valid: false`, not an error status: "this key is wrong" is a
    // successful answer to the question, and the app should show the reason
    // rather than retry.
    return { valid: false, reason: result.reason ?? 'that licence key was not recognised' }
  }

  if (result.seats && result.seats.used > result.seats.limit) {
    return {
      valid: false,
      reason:
        `This licence allows ${result.seats.limit} ` +
        `install${result.seats.limit === 1 ? '' : 's'} and is already in use on ` +
        `${result.seats.used}. Deactivate one first.`,
    }
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
    seats: result.seats,
  }
})
