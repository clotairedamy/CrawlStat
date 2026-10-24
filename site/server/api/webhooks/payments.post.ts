import { getProvider } from '../../utils/providers'

/**
 * `POST /api/webhooks/payments`
 *
 * Refunds and cancellations, from the payment provider.
 *
 * ## Why this does so little
 *
 * There is no licence database to update: the provider is the source of truth,
 * and the app re-checks with it daily. A refund therefore takes effect on the
 * install's next validation — at most a day later, and at most a fortnight if
 * the machine is offline the whole time.
 *
 * That window is a deliberate trade. Closing it would mean a revocation list,
 * which means a database, which means a service to run and pay for — to shorten
 * the tail on refund abuse from two weeks to an hour. The honest answer for a
 * tool at this price is to accept the window.
 *
 * So this endpoint verifies the signature and logs. It exists now because the
 * signature verification and the event mapping are the fiddly parts, and
 * because the day a revocation list *is* justified, this is where it plugs in.
 */
export default defineEventHandler(async (event) => {
  const config = useRuntimeConfig(event)

  // The raw body, not the parsed one: the signature covers the bytes exactly
  // as sent, and re-serialising JSON would change them.
  const rawBody = await readRawBody(event, 'utf8')
  if (!rawBody) {
    throw createError({ statusCode: 400, statusMessage: 'empty body' })
  }

  const headers = getHeaders(event)
  const signature =
    headers['x-signature'] ?? headers['paddle-signature'] ?? headers['webhook-signature'] ?? ''

  const provider = await getProvider({
    provider: config.licenseProvider,
    apiKey: config.providerApiKey,
    storeId: config.providerStoreId,
  })

  const verified = await provider.verifyWebhook(rawBody, signature, config.webhookSecret)
  if (!verified) {
    // 401, and nothing about why: an unsigned request gets no help.
    throw createError({ statusCode: 401, statusMessage: 'invalid signature' })
  }

  let payload: unknown
  try {
    payload = JSON.parse(rawBody)
  } catch {
    throw createError({ statusCode: 400, statusMessage: 'body is not JSON' })
  }

  const parsed = provider.parseWebhook(payload)
  if (!parsed) {
    // Acknowledged: an event shape we do not recognise is not a failure, and
    // returning an error would make the provider retry it forever.
    return { received: true, handled: false }
  }

  if (parsed.revoked) {
    // The install finds out at its next validation. Logged so a refund that
    // does not take effect can be traced.
    console.info(`[webhook] licence revoked: ${parsed.kind}`)
  }

  return { received: true, handled: true, kind: parsed.kind, revoked: parsed.revoked }
})
