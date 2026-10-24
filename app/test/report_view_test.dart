import 'package:crawlstat/src/theme/theme.dart';
import 'package:crawlstat/src/widgets/report_view.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

/// A report as qwen3 actually wrote it, trimmed.
const _report = '''
# Critical link errors and missing metadata threaten site visibility

Of 1,240 pages crawled, 968 contain issues, putting **31%** of internal link value at risk.

## Fix this first
Prioritize the **820 pages** missing meta descriptions.

## The plan
1. **Update meta description templates**: one change fixes most instances.
2. **Repair broken internal links**: 140 pages with 4xx errors.

## Quick wins
- **Batch fix broken links**: the *12 distinct* patterns in one go.

## Can wait
- **Review noindex tags**: probably deliberate.
''';

Future<void> _pump(WidgetTester tester, String markdown) async {
  await tester.pumpWidget(
    MaterialApp(
      theme: CrawlStatTheme.dark(),
      home: Scaffold(
        body: SingleChildScrollView(
          child: ReportView(markdown: markdown, figures: const Text('FIGURES')),
        ),
      ),
    ),
  );
}

void main() {
  testWidgets('draws the verdict, sections and lists without markup', (
    tester,
  ) async {
    await _pump(tester, _report);

    expect(
      find.text(
        'Critical link errors and missing metadata threaten site '
        'visibility',
        findRichText: true,
      ),
      findsOneWidget,
    );
    for (final heading in [
      'Fix this first',
      'The plan',
      'Quick wins',
      'Can wait',
    ]) {
      expect(find.text(heading), findsOneWidget);
    }
    // Numbered items become badges; no literal Markdown survives.
    expect(find.text('1'), findsOneWidget);
    expect(find.text('2'), findsOneWidget);
    expect(find.textContaining('**', findRichText: true), findsNothing);
    expect(find.textContaining('## ', findRichText: true), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('puts the crawl figures straight under the verdict', (
    tester,
  ) async {
    await _pump(tester, _report);
    final verdict = tester.getTopLeft(
      find.textContaining('Critical link errors', findRichText: true),
    );
    final figures = tester.getTopLeft(find.text('FIGURES'));
    final overview = tester.getTopLeft(
      find.textContaining('Of 1,240 pages', findRichText: true),
    );
    expect(figures.dy, greaterThan(verdict.dy));
    expect(figures.dy, lessThan(overview.dy));
  });

  testWidgets('a report cut off mid-stream still draws', (tester) async {
    // Mid-word, mid-bold, mid-list: the state of the text most of the time
    // it is on screen.
    await _pump(tester, _report.substring(0, _report.indexOf('broken') + 3));
    expect(find.text('The plan'), findsOneWidget);
    expect(find.textContaining('**', findRichText: true), findsNothing);
    expect(tester.takeException(), isNull);
  });
}
