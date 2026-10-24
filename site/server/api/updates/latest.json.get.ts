/**
 * `GET /api/updates/latest.json`
 *
 * What the newest release is and where to download it. The app checks this on
 * launch and offers an update.
 *
 * Installers are **not** served from here: the URLs point at GitHub Releases,
 * which hosts binaries for free. This endpoint is a few hundred bytes, cached
 * at the edge for five minutes with a stale-while-revalidate hour — so a
 * hundred thousand installs checking daily cost a handful of function
 * invocations, not a hundred thousand.
 */

/** Bumped by the release workflow, which rewrites this file's constants. */
const LATEST_VERSION = '0.1.0'
const RELEASED_AT = '2026-02-24'

const RELEASE_NOTES = [
  'First public release.',
  'Crawl engine: sitemaps, robots.txt, redirect chains, link graph.',
  '35 SEO audits with fix guidance.',
]

export default defineEventHandler((event) => {
  const config = useRuntimeConfig(event)
  const base = `${config.public.downloadsBase}/download/v${LATEST_VERSION}`

  // Belt and braces alongside the route rule, so the header is right even if
  // the route config is ever changed.
  setResponseHeader(event, 'cache-control', 'public, max-age=300, stale-while-revalidate=3600')

  return {
    version: LATEST_VERSION,
    releasedAt: RELEASED_AT,
    notes: RELEASE_NOTES,
    downloads: {
      // Names must match the assets the release workflow uploads.
      'macos-arm64': `${base}/CrawlStat-${LATEST_VERSION}-arm64.dmg`,
      'macos-x64': `${base}/CrawlStat-${LATEST_VERSION}-x64.dmg`,
      'windows-x64': `${base}/CrawlStat-${LATEST_VERSION}-setup.exe`,
      'linux-x64-appimage': `${base}/CrawlStat-${LATEST_VERSION}-x86_64.AppImage`,
      'linux-x64-deb': `${base}/crawlstat_${LATEST_VERSION}_amd64.deb`,
    },
    releasePage: `${config.public.downloadsBase}/tag/v${LATEST_VERSION}`,
  }
})
