import 'package:crawlstat/src/state/settings.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

const _fallback = 'CrawlStatBot/1.0 (+https://crawlstat.com/bot)';

void main() {
  group('Settings serialisation', () {
    test('round-trips every field', () {
      const original = Settings(
        userAgent: 'MyBot/2.0',
        themeMode: ThemeMode.dark,
        concurrency: 12,
        crawlDelayMs: 250,
        requestTimeoutMs: 45000,
        respectRobots: false,
        crawlSubdomains: true,
        checkExternalLinks: false,
        followNofollow: true,
        crawlImages: false,
        crawlCss: true,
        crawlJavascript: true,
        crawlDocuments: false,
        storeHtml: true,
        runAudits: false,
        projectsDirectory: '/tmp/crawls',
        exportDirectory: '/tmp/exports',
      );

      final restored = Settings.fromJson(original.toJson(), fallbackUserAgent: _fallback);

      expect(restored.userAgent, 'MyBot/2.0');
      expect(restored.themeMode, ThemeMode.dark);
      expect(restored.concurrency, 12);
      expect(restored.crawlDelayMs, 250);
      expect(restored.requestTimeoutMs, 45000);
      expect(restored.respectRobots, isFalse);
      expect(restored.crawlSubdomains, isTrue);
      expect(restored.checkExternalLinks, isFalse);
      expect(restored.followNofollow, isTrue);
      expect(restored.crawlImages, isFalse);
      expect(restored.crawlCss, isTrue);
      expect(restored.crawlJavascript, isTrue);
      expect(restored.crawlDocuments, isFalse);
      expect(restored.storeHtml, isTrue);
      expect(restored.runAudits, isFalse);
      expect(restored.projectsDirectory, '/tmp/crawls');
      expect(restored.exportDirectory, '/tmp/exports');
    });

    test('a file from an older build still opens', () {
      // Only one key: everything else must fall back to a default.
      final restored = Settings.fromJson(
        {'concurrency': 3},
        fallbackUserAgent: _fallback,
      );
      expect(restored.concurrency, 3);
      expect(restored.userAgent, _fallback);
      expect(restored.respectRobots, isTrue);
      expect(restored.themeMode, ThemeMode.system);
    });

    test('a file from a newer build does not crash this one', () {
      final restored = Settings.fromJson(
        {'concurrency': 7, 'somethingFromTheFuture': {'nested': true}},
        fallbackUserAgent: _fallback,
      );
      expect(restored.concurrency, 7);
    });

    test('a value of the wrong type falls back rather than throwing', () {
      final restored = Settings.fromJson(
        {'concurrency': 'lots', 'respectRobots': 'yes'},
        fallbackUserAgent: _fallback,
      );
      expect(restored.concurrency, 5);
      expect(restored.respectRobots, isTrue);
    });

    test('an unknown theme name falls back to following the system', () {
      final restored = Settings.fromJson(
        {'themeMode': 'sepia'},
        fallbackUserAgent: _fallback,
      );
      expect(restored.themeMode, ThemeMode.system);
    });
  });

  group('building a crawl request', () {
    test('carries the user’s defaults, not the engine’s', () {
      const settings = Settings(
        userAgent: 'MyBot/2.0',
        concurrency: 9,
        crawlDelayMs: 300,
        respectRobots: false,
        crawlSubdomains: true,
        crawlCss: true,
      );

      final request = settings.toRequest(startUrl: 'https://a.test');

      expect(request.startUrl, 'https://a.test');
      expect(request.mode, 'spider');
      expect(request.userAgent, 'MyBot/2.0');
      expect(request.concurrency, 9);
      expect(request.crawlDelayMs, 300);
      expect(request.respectRobots, isFalse);
      expect(request.crawlSubdomains, isTrue);
      expect(request.crawlCss, isTrue);
    });

    test('per-crawl options override nothing they are not given', () {
      const settings = Settings(userAgent: _fallback);
      final request = settings.toRequest(
        startUrl: 'https://a.test',
        mode: 'list',
        listUrls: ['https://a.test/one', 'https://a.test/two'],
        maxUrls: 500,
        maxDepth: 3,
        excludePatterns: ['/cart'],
      );

      expect(request.mode, 'list');
      expect(request.listUrls.length, 2);
      expect(request.maxUrls, 500);
      expect(request.maxDepth, 3);
      expect(request.excludePatterns, ['/cart']);
      // Not given, so it keeps the default.
      expect(request.includePatterns, isEmpty);
      expect(request.runAudits, isTrue);
    });

    test('no limit is sent as no limit, not as zero', () {
      const settings = Settings(userAgent: _fallback);
      final request = settings.toRequest(startUrl: 'https://a.test');
      expect(request.maxUrls, isNull);
      expect(request.maxDepth, isNull);
    });
  });
}
