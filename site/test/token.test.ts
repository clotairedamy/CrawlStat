import { createPrivateKey, generateKeyPairSync, verify } from 'node:crypto'
import { describe, expect, it } from 'vitest'

import {
  GRACE_PERIOD_DAYS,
  PRO_FEATURES,
  base64url,
  buildEntitlement,
  fingerprint,
  publicKeyHex,
  signEntitlement,
} from '../server/utils/token'

/** A throwaway Ed25519 key, generated per run. */
function keyPair() {
  const { privateKey, publicKey } = generateKeyPairSync('ed25519')
  return {
    pem: privateKey.export({ type: 'pkcs8', format: 'pem' }).toString(),
    publicKey,
  }
}

const NOW = 1_800_000_000

describe('buildEntitlement', () => {
  it('grants every Pro feature on a Pro plan', () => {
    const entitlement = buildEntitlement({
      plan: 'pro',
      licenseKey: 'CS-TEST-0001',
      instanceId: 'instance-abc',
      now: NOW,
    })

    expect(entitlement.plan).toBe('pro')
    expect(entitlement.features).toEqual([...PRO_FEATURES])
    expect(entitlement.features).toContain('unlimited_urls')
  })

  it('grants nothing on a free plan', () => {
    const entitlement = buildEntitlement({
      plan: 'free',
      licenseKey: 'x',
      instanceId: 'y',
      now: NOW,
    })
    expect(entitlement.features).toEqual([])
  })

  it('expires at the end of the grace period', () => {
    const entitlement = buildEntitlement({
      plan: 'pro',
      licenseKey: 'k',
      instanceId: 'i',
      now: NOW,
    })
    expect(entitlement.issued_at).toBe(NOW)
    expect(entitlement.expires_at).toBe(NOW + GRACE_PERIOD_DAYS * 86400)
  })

  it('never outlives the subscription it is based on', () => {
    // A licence ending in three days must not produce a fortnight's token:
    // that is how a cancelled subscription would keep working.
    const endsInThreeDays = NOW + 3 * 86400
    const entitlement = buildEntitlement({
      plan: 'pro',
      licenseKey: 'k',
      instanceId: 'i',
      now: NOW,
      expiresAt: endsInThreeDays,
    })
    expect(entitlement.expires_at).toBe(endsInThreeDays)
  })

  it('uses the grace period when the subscription outlasts it', () => {
    const entitlement = buildEntitlement({
      plan: 'pro',
      licenseKey: 'k',
      instanceId: 'i',
      now: NOW,
      expiresAt: NOW + 365 * 86400,
    })
    expect(entitlement.expires_at).toBe(NOW + GRACE_PERIOD_DAYS * 86400)
  })

  it('carries a fingerprint rather than the key, and nothing personal', () => {
    const entitlement = buildEntitlement({
      plan: 'pro',
      licenseKey: 'CS-LIVE-abcd-efgh',
      instanceId: 'instance-abc',
      now: NOW,
    })

    const json = JSON.stringify(entitlement)
    expect(json).not.toContain('CS-LIVE-abcd-efgh')
    expect(json).not.toContain('@')
    expect(entitlement.key_fingerprint).toHaveLength(8)
  })
})

describe('fingerprint', () => {
  it('is stable, short and not a slice of the key', () => {
    const key = 'CS-LIVE-9f3a-2b71'
    expect(fingerprint(key)).toBe(fingerprint(key))
    expect(fingerprint(key)).toBe(fingerprint(`  ${key}  `))
    expect(fingerprint(key)).toHaveLength(8)
    expect(key).not.toContain(fingerprint(key))
    expect(fingerprint(key)).not.toBe(fingerprint('CS-LIVE-9f3a-2b72'))
  })
})

describe('signEntitlement', () => {
  it('produces a payload.signature token', () => {
    const { pem } = keyPair()
    const token = signEntitlement(
      buildEntitlement({ plan: 'pro', licenseKey: 'k', instanceId: 'i', now: NOW }),
      pem,
    )

    const parts = token.split('.')
    expect(parts).toHaveLength(2)
    // base64url: no padding, no plus, no slash — the app decodes it with
    // Rust's URL_SAFE_NO_PAD, which rejects all three.
    for (const part of parts) {
      expect(part).not.toContain('=')
      expect(part).not.toContain('+')
      expect(part).not.toContain('/')
    }
  })

  it('signs the encoded payload, so re-serialising cannot change it', () => {
    const { pem, publicKey } = keyPair()
    const entitlement = buildEntitlement({
      plan: 'pro',
      licenseKey: 'k',
      instanceId: 'i',
      now: NOW,
    })
    const token = signEntitlement(entitlement, pem)
    const [payload, signature] = token.split('.') as [string, string]

    // Verified against the payload string's bytes, exactly as the engine does.
    const valid = verify(
      null,
      Buffer.from(payload),
      publicKey,
      Buffer.from(signature, 'base64url'),
    )
    expect(valid).toBe(true)
  })

  it('round-trips the entitlement through the payload', () => {
    const { pem } = keyPair()
    const entitlement = buildEntitlement({
      plan: 'pro',
      licenseKey: 'k',
      instanceId: 'instance-xyz',
      now: NOW,
    })
    const [payload] = signEntitlement(entitlement, pem).split('.') as [string]

    const decoded = JSON.parse(Buffer.from(payload, 'base64url').toString('utf8'))
    expect(decoded).toEqual(entitlement)
    expect(decoded.version).toBe(1)
  })

  it('a token signed with one key does not verify with another', () => {
    const first = keyPair()
    const second = keyPair()
    const token = signEntitlement(
      buildEntitlement({ plan: 'pro', licenseKey: 'k', instanceId: 'i', now: NOW }),
      first.pem,
    )
    const [payload, signature] = token.split('.') as [string, string]

    expect(
      verify(null, Buffer.from(payload), second.publicKey, Buffer.from(signature, 'base64url')),
    ).toBe(false)
  })

  it('accepts a PEM whose newlines arrived escaped', () => {
    // Which is how a key survives most environment-variable editors.
    const { pem } = keyPair()
    const escaped = pem.replace(/\n/g, '\\n')
    expect(() =>
      signEntitlement(
        buildEntitlement({ plan: 'pro', licenseKey: 'k', instanceId: 'i', now: NOW }),
        escaped,
      ),
    ).not.toThrow()
  })
})

describe('publicKeyHex', () => {
  it('derives the 32-byte public key the app embeds', () => {
    const { pem, publicKey } = keyPair()
    const hex = publicKeyHex(pem)

    expect(hex).toHaveLength(64)
    expect(hex).toMatch(/^[0-9a-f]+$/)

    // The same bytes the engine's SIGNING_PUBLIC_KEY_HEX must contain.
    const der = publicKey.export({ type: 'spki', format: 'der' })
    expect(hex).toBe(Buffer.from(der.subarray(der.length - 32)).toString('hex'))
  })

  it('matches the key that actually signs, which is the point', () => {
    const { pem } = keyPair()
    // A mismatch between these two rejects every licence in production, so it
    // is worth a test that would catch an export-format mistake.
    expect(publicKeyHex(pem)).toBe(
      publicKeyHex(
        createPrivateKey(pem).export({ type: 'pkcs8', format: 'pem' }).toString(),
      ),
    )
  })
})

describe('base64url', () => {
  it('omits padding, as the decoder on the Rust side requires', () => {
    expect(base64url('a')).toBe('YQ')
    expect(base64url('ab')).toBe('YWI')
    expect(base64url('abc')).toBe('YWJj')
  })
})
