import 'package:crawlstat/src/rust/api/accessibility.dart';
import 'package:crawlstat/src/screens/accessibility_screen.dart';
import 'package:crawlstat/src/theme/theme.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

const _imageAlt = A11yRuleInfo(
  id: 'image-alt',
  title: 'Images without a text alternative',
  criteria: ['1.1.1'],
  impact: 'critical',
  certainty: 'fail',
  affects: 'Blind and low-vision people using screen readers',
  why: 'A screen reader announces the file name, or nothing.',
  fix: 'Give every meaningful image an alt that says what it shows.',
  bad: '<img src="hero.jpg">',
  good: '<img src="hero.jpg" alt="Team at the 2026 launch">',
);

A11yCriterion _criterion(
  String id,
  String name,
  String principle,
  String status, {
  List<String> hit = const [],
  String? review,
}) => A11yCriterion(
  id: id,
  name: name,
  level: 'A',
  principle: principle,
  since: '2.0',
  summary: '$name summary.',
  howToTest: 'Test $name by hand.',
  url: 'https://www.w3.org/WAI/WCAG22/Understanding/$id',
  status: status,
  automated: hit.isEmpty ? 'none' : 'partial',
  rulesHit: hit,
  rulesRun: hit,
  failingPages: hit.isEmpty ? 0 : 40,
  reviewStatus: review,
  conflict: false,
  vpat: status == 'fail' ? 'Does Not Support' : 'Not Evaluated',
);

final _report = A11yReport(
  standard: const A11yStandard(
    id: 'wcag21aa',
    label: 'WCAG 2.1 AA',
    laws: ['ADA Title II (US public entities)'],
  ),
  coverage: const A11yCoverage(
    pagesChecked: 120,
    pagesUnchecked: 8,
    thinPages: 0,
    thinExamples: [],
    videoPages: 0,
    audioPages: 0,
    formPages: 3,
    passwordPages: 0,
  ),
  verdict: 'does_not_conform',
  headline: 'Does not conform to WCAG 2.1 AA',
  detail: '1 criterion fails on 40 pages.',
  rules: [
    A11yRule(
      info: _imageAlt,
      pages: 40,
      instances: 95,
      share: 40 / 120,
      criteria: const ['1.1.1'],
      level: 'A',
      priority: 30,
      groups: const [
        A11yGroup(
          sample: '<img src="/logo.svg">',
          pages: 38,
          exampleUrls: ['https://example.com/'],
        ),
      ],
      templateFix: true,
    ),
  ],
  criteria: [
    _criterion(
      '1.1.1',
      'Non-text Content',
      'Perceivable',
      'fail',
      hit: ['image-alt'],
    ),
    _criterion('2.1.1', 'Keyboard', 'Operable', 'untested'),
    _criterion(
      '3.1.1',
      'Language of Page',
      'Understandable',
      'pass',
      review: 'pass',
    ),
  ],
  counts: const A11yCounts(
    fail: 1,
    review: 0,
    checksPassed: 0,
    untested: 1,
    pass: 1,
    notApplicable: 0,
  ),
);

Future<void> _pump(WidgetTester tester) async {
  tester.view.physicalSize = const Size(1400, 2400);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
  await tester.pumpWidget(
    ProviderScope(
      child: MaterialApp(
        theme: CrawlStatTheme.dark(),
        home: Scaffold(
          body: AccessibilityReportView(
            report: _report,
            path: '/tmp/example-com-20261024-0930.db',
          ),
        ),
      ),
    ),
  );
}

void main() {
  testWidgets('leads with the verdict and what to fix first', (tester) async {
    await _pump(tester);

    expect(find.text('Does not conform to WCAG 2.1 AA'), findsOneWidget);
    expect(find.text('Not compliant'), findsOneWidget);
    expect(find.textContaining('Re-crawl to include them'), findsOneWidget);
    expect(find.text('Images without a text alternative'), findsOneWidget);
    expect(
      find.textContaining('fix it once in the shared template'),
      findsOneWidget,
    );
  });

  testWidgets('expanding an issue shows the fix with code', (tester) async {
    await _pump(tester);

    await tester.tap(find.text('Images without a text alternative'));
    await tester.pumpAndSettle();

    expect(find.text('How to fix'), findsOneWidget);
    expect(find.text('<img src="hero.jpg">'), findsOneWidget);
    expect(find.text('<img src="/logo.svg">'), findsOneWidget);
  });

  testWidgets('the checklist hides signed-off criteria until asked', (
    tester,
  ) async {
    await _pump(tester);

    expect(find.text('Non-text Content'), findsOneWidget);
    expect(find.text('Keyboard'), findsOneWidget);
    expect(find.text('Language of Page'), findsNothing);

    await tester.tap(find.text('Done 1'));
    await tester.pumpAndSettle();

    expect(find.text('Language of Page'), findsOneWidget);
    expect(find.text('Keyboard'), findsNothing);
  });

  test('names the site a crawl file is about', () {
    expect(
      siteOfCrawl('/tmp/crawls/example-com-20261024-0930.db'),
      'example.com',
    );
  });
}
