/**
 * The merchant of record, behind one interface.
 *
 * CrawlStat does not process payments or store licences: a provider does, and
 * this API is a thin proxy that turns "is this key valid?" into a signed token.
 * Keeping the provider behind an interface means switching between Lemon
 * Squeezy, Polar and Paddle — or being acquired by one of them — is a new file
 * here and an environment variable, not a change to the app.
 *
 * Every implementation must:
 *   * never throw for an invalid key — that is a `valid: false` answer, not an
 *     error, and the difference decides whether the app retries;
 *   * fail closed on a network or credential problem, so a provider outage
 *     cannot silently hand out Pro licences.
 */

export type Plan = 'free' | 'pro'

export interface LicenseCheck {
  valid: boolean
  plan: Plan
  /** Why it was refused, shown to the user verbatim. */
  reason?: string
  /** Unix seconds when the subscription lapses, if it is time-limited. */
  expiresAt?: number
  /** Seats the licence allows, and how many are in use. */
  seats?: { limit: number; used: number }
}

export interface ActivationResult extends LicenseCheck {
  /** The provider's own id for this activation, if it tracks them. */
  instanceName?: string
}

export interface LicenseProvider {
  readonly name: string

  /** Register this installation against a key. Called once, by the user. */
  activate(licenseKey: string, instanceId: string): Promise<ActivationResult>

  /** Re-check a key. Called about once a day per install. */
  validate(licenseKey: string, instanceId: string): Promise<LicenseCheck>

  /** Verify a webhook came from the provider. */
  verifyWebhook(rawBody: string, signature: string, secret: string): Promise<boolean>

  /** What a webhook means, in our terms. */
  parseWebhook(payload: unknown): WebhookEvent | null
}

export interface WebhookEvent {
  /** `activated`, `renewed`, `refunded`, `cancelled`, `expired`. */
  kind: 'activated' | 'renewed' | 'refunded' | 'cancelled' | 'expired' | 'unknown'
  licenseKeyFingerprint?: string
  /** Whether the licence should stop working. */
  revoked: boolean
}

export interface ProviderConfig {
  provider: string
  apiKey: string
  storeId: string
}

/**
 * Pick the configured provider.
 *
 * An unknown name is a configuration error rather than a silent fallback: a
 * typo must not quietly refuse every customer's licence.
 */
export async function getProvider(config: ProviderConfig): Promise<LicenseProvider> {
  switch (config.provider) {
    case 'lemonsqueezy': {
      const { lemonSqueezy } = await import('./providers/lemonsqueezy')
      return lemonSqueezy(config)
    }
    case 'polar': {
      const { polar } = await import('./providers/polar')
      return polar(config)
    }
    case 'paddle': {
      const { paddle } = await import('./providers/paddle')
      return paddle(config)
    }
    default:
      throw new Error(
        `Unknown licence provider "${config.provider}". Set NUXT_LICENSE_PROVIDER to ` +
          'lemonsqueezy, polar or paddle.',
      )
  }
}
