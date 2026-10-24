//! The audit catalogue: every issue CrawlStat reports.
//!
//! Each entry carries a stable `id`, a severity, a description of what the
//! problem is, and how to fix it. The fix text is written to be shown verbatim
//! in the app's Issues panel *and* to become the body of
//! `crawlstat.com/docs/audits/<docs_slug>` in Phase 6, where each audit doubles
//! as an SEO landing page ("How to fix redirect chains").
//!
//! Most checks are a single SQL query returning the affected URLs. The handful
//! that cannot be expressed in SQL — near-duplicate clustering, language-tag
//! validation — are [`Check::Rust`] and live in their own modules.
//!
//! ## Thresholds in SQL
//!
//! Queries contain `{placeholder}` markers that [`super::AuditConfig::apply`]
//! replaces with numbers. These come from a typed struct of integers and
//! floats, never from user text, so there is nothing to inject; binding them as
//! parameters would mean every audit declaring which of the eight thresholds it
//! uses, which is a lot of ceremony for no safety gain.

use super::{Check, RustCheck, Severity};

/// One audit: what it looks for, why it matters, and how to fix it.
#[derive(Debug, Clone, Copy)]
pub struct Audit {
    /// Stable identifier. Stored in the database and used by the UI, the CLI
    /// and (later) CrawlStat CI, so it must not change once shipped.
    pub id: &'static str,
    pub title: &'static str,
    pub severity: Severity,
    /// What the issue is, in one or two sentences.
    pub description: &'static str,
    /// What to do about it.
    pub how_to_fix: &'static str,
    /// Page slug under `/docs/audits/`.
    pub docs_slug: &'static str,
    pub check: Check,
}

/// Pages a search engine could actually index: internal, HTML, 200, not
/// excluded by a robots directive or a canonical pointing elsewhere.
///
/// Created as a view by the audit pass so the queries below stay readable.
pub const INDEXABLE_VIEW: &str = "indexable_pages";

/// Every audit, in the order the UI should show them.
pub const AUDITS: &[Audit] = &[
    // ─── Errors ──────────────────────────────────────────────────────────
    Audit {
        id: "status_5xx",
        title: "Server error (5xx)",
        severity: Severity::Error,
        description: "The server failed while answering these URLs. Search \
            engines drop pages that return 5xx repeatedly, and visitors see \
            nothing at all.",
        how_to_fix: "Check the server's error log for the exact failure. A 500 \
            is usually an application exception, a 502 or 504 an upstream or \
            timeout problem, and a 503 either genuine overload or a \
            maintenance page left switched on. Fix the cause, then re-crawl to \
            confirm. If a 503 is deliberate, send a `Retry-After` header so \
            crawlers come back rather than dropping the page.",
        docs_slug: "server-errors-5xx",
        check: Check::Sql(
            "SELECT id AS url_id, 'HTTP ' || status_code AS detail
             FROM urls WHERE status_code >= 500",
        ),
    },
    Audit {
        id: "status_4xx",
        title: "Client error (4xx)",
        severity: Severity::Error,
        description: "These URLs do not exist or refuse access. Any link, \
            sitemap entry or bookmark pointing at them is wasted, and the link \
            equity they would have passed on is lost.",
        how_to_fix: "For each URL decide which it is: content that moved, in \
            which case 301 it to the new location; content that is gone for \
            good, in which case leave the 404 and remove the links to it; or a \
            mistake, in which case fix the URL. Do not blanket-redirect \
            everything to the home page — a soft 404 is worse than an honest \
            one. Check the Inlinks panel to find the pages that need editing.",
        docs_slug: "client-errors-4xx",
        check: Check::Sql(
            "SELECT id AS url_id, 'HTTP ' || status_code AS detail
             FROM urls WHERE status_code >= 400 AND status_code < 500",
        ),
    },
    Audit {
        id: "fetch_failed",
        title: "URL could not be fetched",
        severity: Severity::Error,
        description: "The request never completed: DNS did not resolve, the \
            connection was refused or reset, TLS failed, or the server did not \
            answer in time. A crawler that cannot reach a page cannot index it.",
        how_to_fix: "Look at the error type on each row. DNS failures mean a \
            hostname that no longer exists — usually an old subdomain still \
            linked somewhere. Connection failures point at a firewall, a rate \
            limiter reacting to the crawl, or a service that is down. Timeouts \
            mean the page is too slow to render; raise the timeout to confirm, \
            then fix the page. TLS errors mean an expired, mismatched or \
            incomplete certificate chain.",
        docs_slug: "fetch-failures",
        check: Check::Sql(
            "SELECT id AS url_id, COALESCE(error_kind, 'unknown') AS detail
             FROM urls WHERE state = 3",
        ),
    },
    Audit {
        id: "broken_internal_link",
        title: "Page links to a broken URL",
        severity: Severity::Error,
        description: "These pages contain links to URLs that return 4xx or \
            5xx. Unlike the error itself, this issue lists the pages you need \
            to edit.",
        how_to_fix: "Edit each page listed and either correct the link, point \
            it at the replacement content, or remove it. The detail column \
            names one broken target per page; open the page's Outlinks panel \
            to see them all. Fixing links at the source is better than adding \
            a redirect for every typo, because a redirect still costs a round \
            trip on every visit.",
        docs_slug: "broken-internal-links",
        check: Check::Sql(
            "SELECT DISTINCT links.source_id AS url_id,
                    'links to ' || target.url AS detail
             FROM links
             JOIN urls AS target ON target.dedup_key = links.target_key
             WHERE target.status_code >= 400
               AND links.link_source = 0
               AND links.is_internal = 1",
        ),
    },
    Audit {
        id: "redirect_loop",
        title: "Redirect loop",
        severity: Severity::Error,
        description: "These URLs redirect in a circle, so nothing is ever \
            served. Browsers show an error after a few hops and search engines \
            give up on the URL entirely.",
        how_to_fix: "Find the two or more rules that point at each other — \
            commonly an HTTP→HTTPS rule fighting a trailing-slash rule, or a \
            CMS redirect duplicating a server one. Order your rules so each \
            URL is rewritten at most once, and prefer one canonical form \
            (scheme, host, trailing slash) applied in a single place.",
        docs_slug: "redirect-loops",
        check: Check::Sql(
            "SELECT id AS url_id, redirect_count || ' hops' AS detail
             FROM urls WHERE redirect_loop = 1",
        ),
    },
    Audit {
        id: "canonical_non_200",
        title: "Canonical points at a broken URL",
        severity: Severity::Error,
        description: "The canonical tag names a URL that does not return 200. \
            A canonical pointing at a 404 or a redirect tells search engines to \
            index something that is not there, which usually means neither URL \
            gets indexed.",
        how_to_fix: "Point the canonical at the final, working URL — the one \
            that returns 200 directly, with no redirect. Check the scheme, the \
            host and the trailing slash: a canonical to the http:// version of \
            an https:// page is the most common form of this mistake.",
        docs_slug: "canonical-points-at-broken-url",
        check: Check::Sql(
            "SELECT page.id AS url_id,
                    'canonical → HTTP ' || COALESCE(target.status_code, '?') AS detail
             FROM urls AS page
             JOIN urls AS target ON target.url = page.canonical
             WHERE page.canonical IS NOT NULL
               AND page.is_internal = 1
               AND (target.status_code IS NULL OR target.status_code != 200)",
        ),
    },
    Audit {
        id: "mixed_content",
        title: "Mixed content on an HTTPS page",
        severity: Severity::Error,
        description: "These HTTPS pages reference images, stylesheets or \
            scripts over plain HTTP. Browsers block active mixed content \
            outright and warn about passive mixed content, so the page is both \
            broken and insecure.",
        how_to_fix: "Change every http:// reference on the page to https://, \
            or make it protocol-relative by starting it at the path. Most cases \
            come from hard-coded absolute URLs in old content or in a theme. \
            Once the references are fixed, add a \
            `Content-Security-Policy: upgrade-insecure-requests` header so any \
            that are missed are upgraded rather than blocked.",
        docs_slug: "mixed-content",
        check: Check::Sql(
            "SELECT DISTINCT links.source_id AS url_id,
                    'insecure reference to ' || links.target_url AS detail
             FROM links
             JOIN urls AS source ON source.id = links.source_id
             WHERE source.url LIKE 'https://%'
               AND links.target_url LIKE 'http://%'
               AND links.link_source IN (1, 2, 3)",
        ),
    },
    // ─── Warnings ────────────────────────────────────────────────────────
    Audit {
        id: "redirect_chain",
        title: "Redirect chain",
        severity: Severity::Warning,
        description: "These URLs take more than {max_redirect_hops} hops to \
            reach their destination. Every hop adds latency for visitors, and \
            long chains risk being abandoned by crawlers before the \
            destination is reached.",
        how_to_fix: "Collapse the chain: change each redirect to point \
            straight at the final destination instead of at the next hop. Then \
            update the internal links, the sitemap and any canonical tags to \
            name the destination directly, so the redirect only ever serves \
            external traffic and old bookmarks.",
        docs_slug: "redirect-chains",
        check: Check::Sql(
            "SELECT id AS url_id, redirect_count || ' hops' AS detail
             FROM urls
             WHERE redirect_count > {max_redirect_hops} AND redirect_loop = 0",
        ),
    },
    Audit {
        id: "internal_link_to_redirect",
        title: "Internal link to a redirect",
        severity: Severity::Warning,
        description: "These pages link to URLs that redirect. The link works, \
            but every visitor and crawler pays for an extra round trip, and \
            some of the link's value is diluted on the way.",
        how_to_fix: "Edit the links to point at the destination URL directly. \
            The detail column names the redirecting target; the destination is \
            on that URL's row. This is one of the cheapest wins on most sites, \
            because a site-wide template link to an old URL can account for \
            thousands of wasted requests.",
        docs_slug: "internal-links-to-redirects",
        check: Check::Sql(
            "SELECT DISTINCT links.source_id AS url_id,
                    'links to ' || target.url AS detail
             FROM links
             JOIN urls AS target ON target.dedup_key = links.target_key
             WHERE target.status_code >= 300 AND target.status_code < 400
               AND links.link_source = 0
               AND links.is_internal = 1",
        ),
    },
    Audit {
        id: "title_missing",
        title: "Missing title",
        severity: Severity::Warning,
        description: "These indexable pages have no `<title>`, or an empty \
            one. The title is the single strongest on-page signal and the \
            headline of every search result, so a missing one costs both \
            ranking and clicks.",
        how_to_fix: "Write a unique title for each page: the page's subject \
            first, the site name last, between {title_min_chars} and \
            {title_max_chars} characters. Describe what the page is, do not \
            stuff keywords, and never reuse the same title across pages.",
        docs_slug: "missing-page-titles",
        check: Check::Sql(
            "SELECT id AS url_id, NULL AS detail
             FROM indexable_pages
             WHERE title IS NULL OR trim(title) = ''",
        ),
    },
    Audit {
        id: "title_duplicate",
        title: "Duplicate title",
        severity: Severity::Warning,
        description: "Several indexable pages share the same title. Search \
            engines use it to tell pages apart, so duplicates suggest the pages \
            are interchangeable — and make the ones that do get indexed \
            compete with each other.",
        how_to_fix: "Give each page a title that describes what is unique \
            about it. Templated titles are the usual cause: a paginated series \
            needs the page number, a filtered listing needs the filter, and a \
            product variant needs the variant. If two pages genuinely are the \
            same, consolidate them and canonicalise one to the other instead.",
        docs_slug: "duplicate-page-titles",
        check: Check::Sql(
            "SELECT id AS url_id, 'shared with ' || (shared - 1) || ' other page(s)' AS detail
             FROM (
                 SELECT id, title,
                        count(*) OVER (PARTITION BY title) AS shared
                 FROM indexable_pages
                 WHERE title IS NOT NULL AND trim(title) != ''
             )
             WHERE shared > 1",
        ),
    },
    Audit {
        id: "title_too_long",
        title: "Title too long",
        severity: Severity::Warning,
        description: "These titles are longer than {title_max_chars} \
            characters, so search engines will usually truncate them. Anything \
            past the cut is invisible in the result.",
        how_to_fix: "Put the important words first and trim the rest. Dropping \
            the site name, or shortening it to an abbreviation, is often enough. \
            The {title_max_chars}-character guide is a proxy for the real limit, \
            which is pixel width, so a title full of capitals or wide letters \
            may be cut sooner.",
        docs_slug: "titles-too-long",
        check: Check::Sql(
            "SELECT id AS url_id, title_length || ' characters' AS detail
             FROM indexable_pages WHERE title_length > {title_max_chars}",
        ),
    },
    Audit {
        id: "title_too_short",
        title: "Title too short",
        severity: Severity::Notice,
        description: "These titles are shorter than {title_min_chars} \
            characters. Short is not wrong, but it usually means the page is \
            leaving room unused and describing itself less well than it could.",
        how_to_fix: "Add the detail a searcher would be looking for: what kind \
            of thing the page is, who it is for, or where it applies. Keep it \
            readable — a title padded out with keywords performs worse than a \
            short, honest one.",
        docs_slug: "titles-too-short",
        check: Check::Sql(
            "SELECT id AS url_id, title_length || ' characters' AS detail
             FROM indexable_pages
             WHERE title_length > 0 AND title_length < {title_min_chars}",
        ),
    },
    Audit {
        id: "description_missing",
        title: "Missing meta description",
        severity: Severity::Warning,
        description: "These indexable pages have no meta description. Search \
            engines then write their own snippet from the page text, which is \
            rarely the pitch you would have chosen.",
        how_to_fix: "Write {description_min_chars}–{description_max_chars} \
            characters per page saying what the page offers and why to click. \
            It is not a ranking factor, so write it for the reader, not for a \
            keyword. Leave it off deliberately only where the page text \
            genuinely makes a better snippet than anything you would write.",
        docs_slug: "missing-meta-descriptions",
        check: Check::Sql(
            "SELECT id AS url_id, NULL AS detail
             FROM indexable_pages
             WHERE meta_description IS NULL OR trim(meta_description) = ''",
        ),
    },
    Audit {
        id: "description_duplicate",
        title: "Duplicate meta description",
        severity: Severity::Warning,
        description: "Several indexable pages share the same meta \
            description. A snippet that describes a whole section rather than \
            the page tends to be discarded and rewritten.",
        how_to_fix: "Rewrite each one around what that page specifically \
            offers. As with titles, templates are the usual cause; if writing \
            one per page is impractical for thousands of pages, build the \
            description from the page's own fields rather than repeating a \
            constant.",
        docs_slug: "duplicate-meta-descriptions",
        check: Check::Sql(
            "SELECT id AS url_id, 'shared with ' || (shared - 1) || ' other page(s)' AS detail
             FROM (
                 SELECT id, meta_description,
                        count(*) OVER (PARTITION BY meta_description) AS shared
                 FROM indexable_pages
                 WHERE meta_description IS NOT NULL AND trim(meta_description) != ''
             )
             WHERE shared > 1",
        ),
    },
    Audit {
        id: "description_too_long",
        title: "Meta description too long",
        severity: Severity::Notice,
        description: "These descriptions are longer than \
            {description_max_chars} characters and will usually be truncated \
            mid-sentence in the search result.",
        how_to_fix: "Lead with the point. Make the first sentence work on its \
            own, so the description still reads as a complete thought when the \
            rest is cut.",
        docs_slug: "meta-descriptions-too-long",
        check: Check::Sql(
            "SELECT id AS url_id, meta_description_length || ' characters' AS detail
             FROM indexable_pages
             WHERE meta_description_length > {description_max_chars}",
        ),
    },
    Audit {
        id: "description_too_short",
        title: "Meta description too short",
        severity: Severity::Notice,
        description: "These descriptions are shorter than \
            {description_min_chars} characters, which usually means the snippet \
            says less than it could.",
        how_to_fix: "Add what the page actually contains or offers. A \
            description of a few words is often a leftover from a template \
            field that was never filled in properly.",
        docs_slug: "meta-descriptions-too-short",
        check: Check::Sql(
            "SELECT id AS url_id, meta_description_length || ' characters' AS detail
             FROM indexable_pages
             WHERE meta_description_length > 0
               AND meta_description_length < {description_min_chars}",
        ),
    },
    Audit {
        id: "h1_missing",
        title: "Missing H1",
        severity: Severity::Warning,
        description: "These indexable pages have no `<h1>`. The first heading \
            tells both readers and search engines what the page is about, and \
            screen-reader users rely on it to orient themselves.",
        how_to_fix: "Add one `<h1>` naming the page's subject. It should agree \
            with the title but need not be identical — the title is written for \
            a search result, the H1 for someone already on the page. If the \
            design has no visible heading, the page probably still needs one; \
            style it rather than removing it.",
        docs_slug: "missing-h1",
        check: Check::Sql(
            "SELECT id AS url_id, NULL AS detail
             FROM indexable_pages WHERE COALESCE(h1_count, 0) = 0",
        ),
    },
    Audit {
        id: "h1_multiple",
        title: "Multiple H1s",
        severity: Severity::Notice,
        description: "These pages have more than one `<h1>`. HTML5 permits it, \
            but in practice several top-level headings usually mean the heading \
            hierarchy is being used for styling rather than structure.",
        how_to_fix: "Keep one `<h1>` for the page's subject and demote the \
            others to `<h2>` or `<h3>` according to how they nest. Check \
            templates first: a header logo wrapped in an `<h1>` plus a page \
            title in another is the most common cause.",
        docs_slug: "multiple-h1-tags",
        check: Check::Sql(
            "SELECT id AS url_id, h1_count || ' H1 tags' AS detail
             FROM indexable_pages WHERE h1_count > 1",
        ),
    },
    Audit {
        id: "canonical_missing",
        title: "Missing canonical",
        severity: Severity::Notice,
        description: "These indexable pages declare no canonical URL. It is \
            not required, but without one any URL variant that reaches the page \
            — a tracking parameter, a trailing slash, an uppercase path — can \
            be indexed as a separate page.",
        how_to_fix: "Add a self-referencing \
            `<link rel=\"canonical\" href=\"…\">` with the page's absolute, \
            preferred URL. Most CMSs and frameworks can do this for every page \
            from one template change.",
        docs_slug: "missing-canonical-tags",
        check: Check::Sql(
            "SELECT id AS url_id, NULL AS detail
             FROM indexable_pages WHERE canonical IS NULL",
        ),
    },
    Audit {
        id: "canonicalised",
        title: "Canonicalised to another URL",
        severity: Severity::Notice,
        description: "These pages name a different URL as canonical, so they \
            are asking not to be indexed themselves. That is often correct — \
            but it is worth checking that each one is deliberate, because a \
            canonical pointing at the wrong page removes it from search \
            results entirely.",
        how_to_fix: "Confirm the target is the page you want indexed, that it \
            returns 200, and that its content really is equivalent. A canonical \
            across genuinely different pages is ignored at best and loses you \
            the page at worst. Pagination and filtered listings should usually \
            self-canonicalise instead.",
        docs_slug: "canonicalised-pages",
        check: Check::Sql(
            "SELECT id AS url_id, 'canonical → ' || canonical AS detail
             FROM urls
             WHERE is_internal = 1 AND kind = 0 AND status_code = 200
               AND canonical IS NOT NULL AND canonical_is_self = 0",
        ),
    },
    Audit {
        id: "noindex_page",
        title: "Noindex page",
        severity: Severity::Notice,
        description: "These pages tell search engines not to index them, via a \
            meta robots tag or an X-Robots-Tag header. Listed so you can check \
            that each one is intended — a `noindex` left on by accident is one \
            of the few SEO mistakes that removes a page completely.",
        how_to_fix: "Keep `noindex` on pages that genuinely should not appear \
            in search: internal search results, thank-you pages, staging \
            content. Remove it anywhere else. Note that a `noindex` page still \
            consumes crawl budget, so for large sets of them consider blocking \
            the pattern in robots.txt once they are out of the index.",
        docs_slug: "noindex-pages",
        check: Check::Sql(
            "SELECT id AS url_id,
                    CASE WHEN LOWER(COALESCE(meta_robots, '')) LIKE '%noindex%'
                         THEN 'meta robots' ELSE 'X-Robots-Tag' END AS detail
             FROM urls
             WHERE is_internal = 1 AND status_code = 200
               AND (LOWER(COALESCE(meta_robots, '')) LIKE '%noindex%'
                 OR LOWER(COALESCE(x_robots_tag, '')) LIKE '%noindex%')",
        ),
    },
    Audit {
        id: "noindex_internal_links",
        title: "Noindex page receives internal links",
        severity: Severity::Warning,
        description: "These pages are marked `noindex` but are linked to from \
            elsewhere on the site. The links spend crawl budget and pass value \
            into a page that can never rank.",
        how_to_fix: "Decide which is wrong. If the page should rank, remove \
            the `noindex`. If it should not, reduce how prominently it is \
            linked — particularly from site-wide navigation — so crawl budget \
            goes to pages that can earn traffic. Do not add `nofollow` to the \
            links: that discards the value rather than redirecting it.",
        docs_slug: "noindex-pages-with-internal-links",
        check: Check::Sql(
            "SELECT id AS url_id, internal_inlinks || ' internal link(s)' AS detail
             FROM urls
             WHERE is_internal = 1 AND status_code = 200 AND internal_inlinks > 0
               AND (LOWER(COALESCE(meta_robots, '')) LIKE '%noindex%'
                 OR LOWER(COALESCE(x_robots_tag, '')) LIKE '%noindex%')",
        ),
    },
    Audit {
        id: "robots_blocked_but_linked",
        title: "Blocked by robots.txt but linked internally",
        severity: Severity::Warning,
        description: "robots.txt forbids crawling these URLs, yet the site \
            links to them. A blocked URL can still be indexed from its links, \
            with no content and no description, which is the worst of both \
            outcomes.",
        how_to_fix: "If the page should be private, use `noindex` — or real \
            access control — rather than robots.txt, and remove the internal \
            links. robots.txt stops crawling, not indexing. If the page should \
            be public, unblock the path in robots.txt so it can be crawled \
            properly.",
        docs_slug: "pages-blocked-by-robots-txt",
        check: Check::Sql(
            "SELECT id AS url_id, internal_inlinks || ' internal link(s)' AS detail
             FROM urls WHERE state = 5 AND internal_inlinks > 0",
        ),
    },
    Audit {
        id: "hreflang_missing_return",
        title: "Hreflang missing a return link",
        severity: Severity::Warning,
        description: "These pages declare an hreflang alternate that does not \
            point back at them. Hreflang must be reciprocal: an unconfirmed \
            annotation is ignored, so the whole cluster stops working.",
        how_to_fix: "Every page in a language group must list every page in \
            that group, including itself. If A declares B, then B must declare \
            A. Generate the block from one shared mapping rather than writing \
            it per page, and remember to include the self-referencing entry and \
            an `x-default` where you have one.",
        docs_slug: "hreflang-missing-return-links",
        check: Check::Sql(
            "SELECT DISTINCT a.url_id AS url_id,
                    'no return link from ' || target.url AS detail
             FROM hreflang AS a
             JOIN urls AS source ON source.id = a.url_id
             JOIN urls AS target ON target.dedup_key = a.href_key
             WHERE target.id != a.url_id
               AND NOT EXISTS (
                   SELECT 1 FROM hreflang AS b
                   WHERE b.url_id = target.id AND b.href_key = source.dedup_key
               )",
        ),
    },
    Audit {
        id: "hreflang_missing_self",
        title: "Hreflang block has no self-reference",
        severity: Severity::Warning,
        description: "These pages list hreflang alternates but do not include \
            themselves. A page must name its own language in its own block, or \
            search engines cannot tell which language the page they are looking \
            at is written in, and may discard the whole cluster.",
        how_to_fix: "Add an entry for the page itself, with its own language \
            code and its own URL. The simplest way to get this right is to \
            generate the block from one list of every URL in the language group \
            and output the same list on every page in it, including the page \
            doing the output.",
        docs_slug: "hreflang-missing-self-reference",
        check: Check::Sql(
            "SELECT page.id AS url_id, 'no entry for this page' AS detail
             FROM urls AS page
             WHERE page.is_internal = 1
               AND EXISTS (SELECT 1 FROM hreflang WHERE hreflang.url_id = page.id)
               AND NOT EXISTS (
                   SELECT 1 FROM hreflang
                   WHERE hreflang.url_id = page.id
                     AND hreflang.href_key = page.dedup_key
               )",
        ),
    },
    Audit {
        id: "hreflang_invalid_code",
        title: "Invalid hreflang language code",
        severity: Severity::Warning,
        description: "These hreflang values are not valid language or \
            language-region codes. An unrecognised code is ignored, so the page \
            is left out of its language group.",
        how_to_fix: "Use an ISO 639-1 language code, optionally followed by a \
            hyphen and an ISO 3166-1 alpha-2 *region* code: `en`, `en-GB`, \
            `pt-BR`. The region is a country, not a language — `en-UK` is \
            wrong (the country code is `GB`), and `en_us` uses the wrong \
            separator and case. The only non-language value permitted is \
            `x-default`.",
        docs_slug: "invalid-hreflang-codes",
        check: Check::Rust(RustCheck::HreflangInvalidCodes),
    },
    Audit {
        id: "duplicate_content",
        title: "Duplicate content",
        severity: Severity::Warning,
        description: "These pages have identical text. Search engines pick one \
            and discard the rest, and the choice is theirs, not yours.",
        how_to_fix: "Consolidate: keep the best version, 301 the others to it, \
            or canonicalise them to it if they must stay reachable. Where \
            duplication is structural — a printer-friendly copy, the same \
            product under two categories — a canonical to the primary URL is \
            the right fix. Where it is thin templated text, write real content \
            or remove the pages.",
        docs_slug: "duplicate-content",
        check: Check::Sql(
            "SELECT id AS url_id, 'identical to ' || (shared - 1) || ' other page(s)' AS detail
             FROM (
                 SELECT id, text_hash,
                        count(*) OVER (PARTITION BY text_hash) AS shared
                 FROM indexable_pages
                 WHERE text_hash IS NOT NULL AND COALESCE(word_count, 0) > 0
             )
             WHERE shared > 1",
        ),
    },
    Audit {
        id: "near_duplicate_content",
        title: "Near-duplicate content",
        severity: Severity::Notice,
        description: "These pages are not identical but are more than \
            {near_duplicate_similarity}% similar — typically the same text \
            with a different heading, price or date. Search engines treat them \
            as competing versions of one page.",
        how_to_fix: "Decide whether the differences matter to a reader. If \
            they do, make them substantial: different openings, different \
            detail, different examples. If they do not, consolidate the pages \
            as you would for exact duplicates. Thin pages generated per \
            location or per variant are the usual source.",
        docs_slug: "near-duplicate-content",
        check: Check::Rust(RustCheck::NearDuplicates),
    },
    Audit {
        id: "image_missing_alt",
        title: "Image missing alt text",
        severity: Severity::Warning,
        description: "These pages contain images with no `alt` attribute. \
            Screen-reader users get the file name read out instead of a \
            description, and the image cannot be understood by image search.",
        how_to_fix: "Describe what the image shows, in a few words, as you \
            would to someone who cannot see it. Purely decorative images should \
            have an empty `alt=\"\"`, which is explicitly different from having \
            none: it tells a screen reader to skip the image rather than \
            announce it.",
        docs_slug: "images-missing-alt-text",
        check: Check::Sql(
            "SELECT images.url_id AS url_id,
                    count(*) || ' image(s)' AS detail
             FROM images
             JOIN urls ON urls.id = images.url_id
             WHERE images.alt_missing = 1 AND urls.is_internal = 1
             GROUP BY images.url_id",
        ),
    },
    Audit {
        id: "orphan_page",
        title: "Orphan page",
        severity: Severity::Warning,
        description: "Nothing on the site links to these pages. They were \
            found only through the sitemap or a canonical tag. A page with no \
            internal links is hard for crawlers to find and gets almost no \
            internal link value.",
        how_to_fix: "Link to each page from somewhere relevant — a section \
            index, a related-content block, the navigation if it deserves it. \
            If the page should not be reachable, remove it from the sitemap \
            too, because a sitemap entry with no links is a contradictory \
            signal.",
        docs_slug: "orphan-pages",
        check: Check::Sql(
            "SELECT page.id AS url_id,
                    CASE WHEN sitemap.dedup_key IS NOT NULL
                         THEN 'in sitemap, no internal links'
                         ELSE 'no internal links' END AS detail
             FROM indexable_pages AS page
             LEFT JOIN sitemap_urls AS sitemap ON sitemap.dedup_key = page.dedup_key
             WHERE page.internal_inlinks = 0
               -- A seed has no `discovered_via`: the crawl started there, so
               -- having nothing link to it is not a fault.
               AND page.discovered_via IS NOT NULL",
        ),
    },
    Audit {
        id: "http_url_on_https_site",
        title: "HTTP URL on an HTTPS site",
        severity: Severity::Warning,
        description: "The site is served over HTTPS, but these internal URLs \
            were crawled over plain HTTP. Even when they redirect, they leak \
            the request to the network and cost an extra round trip.",
        how_to_fix: "Update the internal links to use https:// directly. Keep \
            a site-wide HTTP→HTTPS redirect for outside traffic, and add an \
            HSTS header (`Strict-Transport-Security`) so browsers stop trying \
            HTTP at all after their first visit.",
        docs_slug: "http-urls-on-https-sites",
        check: Check::Sql(
            "SELECT id AS url_id, NULL AS detail
             FROM urls
             WHERE is_internal = 1 AND url LIKE 'http://%' AND {site_is_https} = 1",
        ),
    },
    // ─── Accessibility ───────────────────────────────────────────────────
    //
    // Every check here is a definite WCAG failure that is visible in the
    // markup. Anything needing layout, computed style or a real accessibility
    // tree — contrast, focus order, whether a thing is actually visible — is
    // deliberately absent: a crawler that never renders cannot know it, and
    // guessing would make the whole set untrustworthy.
    Audit {
        id: "a11y_link_without_name",
        title: "Link with no accessible name",
        severity: Severity::Error,
        description: "These pages contain links a screen reader announces as \
            just \"link\". An icon-only link with no label, or one wrapping an \
            image with empty alt text, is unusable without sight — and search \
            engines get nothing from it either.",
        how_to_fix: "Give the link text. If it must look like an icon alone, \
            put the words in `aria-label`, or give the image inside it real alt \
            text describing *the destination* rather than the picture: a home \
            icon's alt is \"Home\", not \"house\". WCAG 2.4.4 and 4.1.2.",
        docs_slug: "links-without-accessible-names",
        check: Check::Sql(
            "SELECT id AS url_id, a11y_links_no_name || ' link(s)' AS detail
             FROM indexable_pages WHERE a11y_links_no_name > 0",
        ),
    },
    Audit {
        id: "a11y_button_without_name",
        title: "Button with no accessible name",
        severity: Severity::Error,
        description: "A button with no text, label or value is announced as \
            \"button\" and nothing more, so nobody using a screen reader can \
            know what it does before pressing it.",
        how_to_fix: "Put text in the button. Where the design is an icon only, \
            add `aria-label` saying what the button *does* — \"Close dialog\", \
            \"Add to basket\" — and keep the icon decorative. WCAG 4.1.2.",
        docs_slug: "buttons-without-accessible-names",
        check: Check::Sql(
            "SELECT id AS url_id, a11y_buttons_no_name || ' button(s)' AS detail
             FROM indexable_pages WHERE a11y_buttons_no_name > 0",
        ),
    },
    Audit {
        id: "a11y_input_without_label",
        title: "Form field with no label",
        severity: Severity::Error,
        description: "These form fields have no `<label>`, `aria-label` or \
            `aria-labelledby`. A screen reader reaches the field and can say \
            only \"edit text\", which makes the form impossible to complete.",
        how_to_fix: "Add a `<label for=\"…\">` matching the field's id, or wrap \
            the field in the label. A `placeholder` is not a label: it vanishes \
            as soon as someone types, and several screen readers ignore it. \
            Where the design has no room for visible text, `aria-label` is the \
            right answer. WCAG 1.3.1 and 3.3.2.",
        docs_slug: "form-fields-without-labels",
        check: Check::Sql(
            "SELECT id AS url_id, a11y_inputs_no_label || ' field(s)' AS detail
             FROM indexable_pages WHERE a11y_inputs_no_label > 0",
        ),
    },
    Audit {
        id: "a11y_broken_aria_reference",
        title: "ARIA reference points at nothing",
        severity: Severity::Error,
        description: "An `aria-labelledby` or `aria-describedby` on these \
            pages names an element id that is not on the page. The reference is \
            silently dropped, so the element ends up with no name at all — \
            worse than if the attribute had never been added.",
        how_to_fix: "Check each id actually exists and is spelled identically; \
            they are case-sensitive. This usually follows a template change \
            that renamed or removed the element being pointed at. Watch for \
            ids that only exist after JavaScript runs, which is too late for \
            some assistive technology.",
        docs_slug: "broken-aria-references",
        check: Check::Sql(
            "SELECT id AS url_id, a11y_broken_aria || ' reference(s)' AS detail
             FROM indexable_pages WHERE a11y_broken_aria > 0",
        ),
    },
    Audit {
        id: "a11y_page_without_lang",
        title: "Page does not declare a language",
        severity: Severity::Error,
        description: "These pages have no `lang` attribute on `<html>`. A \
            screen reader then guesses, and reads English in a French accent or \
            the reverse — which ranges from irritating to incomprehensible.",
        how_to_fix: "Add `lang` to the `<html>` element: `<html lang=\"en-GB\">`. \
            Mark any passage in another language with its own `lang` too. This \
            is one attribute, it takes a minute, and it is WCAG 3.1.1 at \
            level A.",
        docs_slug: "pages-without-a-language",
        check: Check::Sql(
            "SELECT id AS url_id, NULL AS detail
             FROM indexable_pages
             WHERE html_lang IS NULL OR trim(html_lang) = ''",
        ),
    },
    Audit {
        id: "a11y_duplicate_id",
        title: "Duplicate element id",
        severity: Severity::Warning,
        description: "The same `id` appears more than once on these pages. \
            Every `aria-labelledby`, `aria-describedby` and `<label for>` \
            pointing at it resolves to the first one only, so some controls \
            quietly get the wrong name or none.",
        how_to_fix: "Make each id unique. Duplicates almost always come from a \
            component rendered several times with a hard-coded id — generate \
            one per instance instead. WCAG 4.1.1.",
        docs_slug: "duplicate-element-ids",
        check: Check::Sql(
            "SELECT id AS url_id, a11y_duplicate_ids || ' duplicated id(s)' AS detail
             FROM indexable_pages WHERE a11y_duplicate_ids > 0",
        ),
    },
    Audit {
        id: "a11y_invalid_role",
        title: "Invalid ARIA role",
        severity: Severity::Warning,
        description: "These pages use a `role` that ARIA does not define. An \
            unrecognised role is ignored outright, so the element falls back to \
            its native semantics — or, on a `<div>`, to none at all.",
        how_to_fix: "Correct the spelling against the ARIA role list. The \
            usual culprits are invented names like `role=\"clickable\"` and \
            near-misses like `role=\"buton\"`. Better still, use the element \
            that already has the semantics: a `<button>` needs no role.",
        docs_slug: "invalid-aria-roles",
        check: Check::Sql(
            "SELECT id AS url_id, a11y_invalid_roles AS detail
             FROM indexable_pages WHERE a11y_invalid_roles IS NOT NULL",
        ),
    },
    Audit {
        id: "a11y_positive_tabindex",
        title: "Positive tabindex",
        severity: Severity::Warning,
        description: "A `tabindex` greater than zero pulls an element to the \
            front of the tab order, ahead of everything in document order. One \
            of them rearranges keyboard navigation for the whole page, usually \
            in a way nobody intended.",
        how_to_fix: "Use `tabindex=\"0\"` to make something focusable in its \
            natural position, or `-1` for something only script will focus. If \
            the tab order is wrong, fix the order of the markup rather than \
            overriding it. WCAG 2.4.3.",
        docs_slug: "positive-tabindex",
        check: Check::Sql(
            "SELECT id AS url_id, a11y_positive_tabindex || ' element(s)' AS detail
             FROM indexable_pages WHERE a11y_positive_tabindex > 0",
        ),
    },
    Audit {
        id: "a11y_iframe_without_title",
        title: "Frame with no title",
        severity: Severity::Warning,
        description: "An `<iframe>` with no `title` is announced as just \
            \"frame\". Someone navigating by frame has no way to tell the \
            video from the map from the advert.",
        how_to_fix: "Add `title` describing what the frame contains — \
            \"Office location map\", not \"iframe\". A frame that is purely \
            decorative or offscreen should carry `aria-hidden=\"true\"` \
            instead. WCAG 4.1.2.",
        docs_slug: "frames-without-titles",
        check: Check::Sql(
            "SELECT id AS url_id, a11y_iframes_no_title || ' frame(s)' AS detail
             FROM indexable_pages WHERE a11y_iframes_no_title > 0",
        ),
    },
    Audit {
        id: "a11y_heading_level_skipped",
        title: "Heading level skipped",
        severity: Severity::Notice,
        description: "These pages jump down more than one heading level at \
            once, such as an `<h2>` straight to an `<h4>`. Screen-reader users \
            navigate by heading, and a gap reads as a missing section.",
        how_to_fix: "Use heading levels to describe structure, not size: go \
            down one level at a time and style them with CSS. Jumping back *up* \
            any number of levels is fine — that is just a new section. WCAG \
            1.3.1.",
        docs_slug: "skipped-heading-levels",
        check: Check::Sql(
            "SELECT id AS url_id, a11y_heading_skips || ' skip(s)' AS detail
             FROM indexable_pages WHERE a11y_heading_skips > 0",
        ),
    },
    // ─── Notices ─────────────────────────────────────────────────────────
    Audit {
        id: "slow_response",
        title: "Slow response",
        severity: Severity::Notice,
        description: "These URLs took longer than {slow_response_ms} ms to \
            respond. Server response time is the floor under every other speed \
            metric: nothing on the page can start until the first byte arrives.",
        how_to_fix: "Look for the usual culprits: an uncached database query \
            per request, a slow third-party call made during rendering, or no \
            page cache at all. Measure where the time goes before optimising. \
            Note that this is server time only, measured from a local crawl — \
            it is not a Core Web Vitals score, which Phase 5's PageSpeed \
            integration will provide.",
        docs_slug: "slow-server-response",
        check: Check::Sql(
            "SELECT id AS url_id, response_time_ms || ' ms' AS detail
             FROM urls WHERE response_time_ms > {slow_response_ms}",
        ),
    },
    Audit {
        id: "image_oversized",
        title: "Oversized image",
        severity: Severity::Notice,
        description: "These images are larger than {large_image_kb} KB. Images \
            are usually the heaviest part of a page, and an oversized one \
            delays everything that follows it.",
        how_to_fix: "Serve WebP or AVIF instead of JPEG or PNG, resize to the \
            dimensions actually displayed rather than relying on the browser to \
            scale down, and use `srcset` so small screens get small files. Add \
            `loading=\"lazy\"` to images below the fold — but never to the main \
            image at the top of the page, which should load as early as \
            possible.",
        docs_slug: "oversized-images",
        check: Check::Sql(
            "SELECT id AS url_id,
                    (COALESCE(body_bytes, content_length) / 1024) || ' KB' AS detail
             FROM urls
             WHERE kind = 1
               AND COALESCE(body_bytes, content_length) > {large_image_bytes}",
        ),
    },
    Audit {
        id: "deep_page",
        title: "Page buried deep in the site",
        severity: Severity::Notice,
        description: "These pages are more than {max_depth_notice} clicks from \
            the start URL. Depth is a reasonable proxy for how important a site \
            treats a page: the deeper it is, the less often it is crawled and \
            the less internal link value it receives.",
        how_to_fix: "Shorten the path to the pages that matter. Add section \
            hubs, link related content across sections, and keep pagination \
            short — a 40-page paginated archive buries everything past page \
            two. Check the crawl-depth distribution in the report: a site whose \
            pages cluster at depth six or more usually has a navigation \
            problem, not a content one.",
        docs_slug: "crawl-depth",
        check: Check::Sql(
            "SELECT id AS url_id, depth || ' clicks deep' AS detail
             FROM indexable_pages WHERE depth > {max_depth_notice}",
        ),
    },
];

/// Look up an audit by its stable id.
pub fn find(id: &str) -> Option<&'static Audit> {
    AUDITS.iter().find(|audit| audit.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_slug_shaped() {
        let mut ids: Vec<&str> = AUDITS.iter().map(|a| a.id).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "duplicate audit id");

        for audit in AUDITS {
            assert!(
                audit
                    .id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit()),
                "{} is not a snake_case id",
                audit.id
            );
            assert!(
                audit
                    .docs_slug
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '-' || c.is_ascii_digit()),
                "{} has a non-kebab-case docs slug",
                audit.docs_slug
            );
        }
    }

    #[test]
    fn docs_slugs_are_unique() {
        let mut slugs: Vec<&str> = AUDITS.iter().map(|a| a.docs_slug).collect();
        let count = slugs.len();
        slugs.sort_unstable();
        slugs.dedup();
        assert_eq!(slugs.len(), count, "two audits would share a docs page");
    }

    #[test]
    fn every_audit_explains_itself() {
        for audit in AUDITS {
            assert!(!audit.title.is_empty(), "{} has no title", audit.id);
            // These strings are shown to users and become documentation pages,
            // so a stub would be a visible defect.
            assert!(
                audit.description.len() > 60,
                "{} has a description too short to be useful",
                audit.id
            );
            assert!(
                audit.how_to_fix.len() > 80,
                "{} does not really explain the fix",
                audit.id
            );
        }
    }

    #[test]
    fn sql_audits_select_a_url_id() {
        for audit in AUDITS {
            if let Check::Sql(sql) = audit.check {
                assert!(sql.contains("AS url_id"), "{} must select url_id", audit.id);
                assert!(sql.contains("AS detail"), "{} must select detail", audit.id);
            }
        }
    }

    #[test]
    fn lookup_by_id() {
        assert_eq!(find("status_4xx").unwrap().severity, Severity::Error);
        assert_eq!(find("slow_response").unwrap().severity, Severity::Notice);
        assert!(find("no_such_audit").is_none());
    }

    #[test]
    fn severities_are_spread_across_the_catalogue() {
        let count = |severity| AUDITS.iter().filter(|a| a.severity == severity).count();
        assert!(count(Severity::Error) >= 5);
        assert!(count(Severity::Warning) >= 10);
        assert!(count(Severity::Notice) >= 5);
    }
}
