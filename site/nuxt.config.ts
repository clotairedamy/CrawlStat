// https://nuxt.com/docs/api/configuration/nuxt-config
export default defineNuxtConfig({
  compatibilityDate: '2026-01-01',

  nitro: {
    // Vercel is the only host, per the project's constraints.
    preset: 'vercel',

    prerender: {
      // Everything the marketing site and docs serve is static, so it is built
      // once and served from the edge. Only `/api/**` runs as a function, which
      // is what keeps the bill flat as traffic grows.
      crawlLinks: true,
      routes: ['/'],
      failOnError: false,
    },
  },

  routeRules: {
    // The update manifest is read by every install on launch, so it is cached
    // hard at the edge: a release changes it a few times a month, and a stale
    // minute costs nobody anything.
    '/api/updates/latest.json': {
      cache: { maxAge: 300, staleMaxAge: 3600, swr: true },
      headers: { 'cache-control': 'public, max-age=300, stale-while-revalidate=3600' },
    },
    // Licence endpoints are per-install and must never be cached.
    '/api/license/**': {
      cache: false,
      headers: { 'cache-control': 'no-store' },
    },
    '/api/webhooks/**': {
      cache: false,
      headers: { 'cache-control': 'no-store' },
    },
  },

  runtimeConfig: {
    // Server-only. Set in Vercel's environment variables; never bundled into
    // the client.
    licenseSigningKey: '',      // NUXT_LICENSE_SIGNING_KEY  (Ed25519 PKCS#8 PEM)
    licenseProvider: 'lemonsqueezy', // NUXT_LICENSE_PROVIDER
    providerApiKey: '',         // NUXT_PROVIDER_API_KEY
    providerStoreId: '',        // NUXT_PROVIDER_STORE_ID
    webhookSecret: '',          // NUXT_WEBHOOK_SECRET
    public: {
      // Safe to ship to the browser.
      downloadsBase: 'https://github.com/crawlstat/crawlstat/releases',
    },
  },

  // Phase 6 fills in the pages; this phase only needs the API routes.
  future: { compatibilityVersion: 4 },
})
