import 'package:crawlstat/src/theme/theme.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('status colours', () {
    test('carry the meaning of the status class', () {
      expect(CrawlStatTheme.statusColor(200), CrawlStatTheme.success);
      expect(CrawlStatTheme.statusColor(204), CrawlStatTheme.success);
      expect(CrawlStatTheme.statusColor(301), CrawlStatTheme.warning);
      expect(CrawlStatTheme.statusColor(404), CrawlStatTheme.error);
      expect(CrawlStatTheme.statusColor(500), CrawlStatTheme.error);
    });

    test('a URL that was never fetched is neutral, not an error', () {
      // Queued and robots-blocked URLs have no status; colouring them red
      // would make the table look broken when it is not.
      expect(CrawlStatTheme.statusColor(null), CrawlStatTheme.notice);
    });
  });

  test('severity colours match the audit severities', () {
    expect(CrawlStatTheme.severityColor('error'), CrawlStatTheme.error);
    expect(CrawlStatTheme.severityColor('warning'), CrawlStatTheme.warning);
    expect(CrawlStatTheme.severityColor('notice'), CrawlStatTheme.notice);
    // An unknown severity from a newer build must still render.
    expect(CrawlStatTheme.severityColor('whatever'), CrawlStatTheme.notice);
  });

  group('themes', () {
    test('both are built and differ in brightness', () {
      expect(CrawlStatTheme.light().brightness, Brightness.light);
      expect(CrawlStatTheme.dark().brightness, Brightness.dark);
    });

    test('share one accent, so colour keeps its meaning in both', () {
      expect(CrawlStatTheme.light().colorScheme.primary, CrawlStatTheme.accent);
      expect(CrawlStatTheme.dark().colorScheme.primary, CrawlStatTheme.accent);
    });

    test('are dense enough for a data grid', () {
      // The grid assumes a 13px base and compact density; a change to either
      // would quietly break every row height.
      expect(CrawlStatTheme.light().textTheme.bodyMedium?.fontSize, 13);
      expect(CrawlStatTheme.rowHeight, lessThan(36));
    });
  });
}
