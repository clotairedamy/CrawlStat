import 'package:crawlstat/src/theme/theme.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  /// Run [body] with a context under the dark theme.
  Future<void> withContext(
    WidgetTester tester,
    void Function(BuildContext context) body,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: CrawlStatTheme.dark(),
        home: Builder(
          builder: (context) {
            body(context);
            return const SizedBox();
          },
        ),
      ),
    );
  }

  group('status colours', () {
    testWidgets('carry the meaning of the status class', (tester) async {
      await withContext(tester, (context) {
        final palette = CrawlStatTheme.palette(context);
        expect(CrawlStatTheme.statusColor(context, 200), palette.success);
        expect(CrawlStatTheme.statusColor(context, 204), palette.success);
        expect(CrawlStatTheme.statusColor(context, 301), palette.warning);
        expect(CrawlStatTheme.statusColor(context, 404), palette.error);
        expect(CrawlStatTheme.statusColor(context, 500), palette.error);
      });
    });

    testWidgets('a URL that was never fetched is neutral, not an error', (
      tester,
    ) async {
      // Queued and robots-blocked URLs have no status; colouring them red
      // would make the table look broken when it is not.
      await withContext(tester, (context) {
        expect(
          CrawlStatTheme.statusColor(context, null),
          CrawlStatTheme.palette(context).notice,
        );
      });
    });
  });

  testWidgets('severity colours match the audit severities', (tester) async {
    await withContext(tester, (context) {
      final palette = CrawlStatTheme.palette(context);
      expect(CrawlStatTheme.severityColor(context, 'error'), palette.error);
      expect(CrawlStatTheme.severityColor(context, 'warning'), palette.warning);
      expect(CrawlStatTheme.severityColor(context, 'notice'), palette.notice);
      // An unknown severity from a newer build must still render.
      expect(CrawlStatTheme.severityColor(context, 'whatever'), palette.notice);
    });
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
