import 'package:crawlstat/src/theme/theme.dart';
import 'package:crawlstat/src/widgets/section_stage.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('wheel handling', () {
    test('a small nudge does not move the stage', () {
      // A trackpad sends a stream of tiny deltas; advancing on each one
      // would fly through the whole screen on a single flick.
      expect(shouldAdvance(12), isFalse);
      expect(advanceTarget(1, 12, 5), 1);
    });

    test('a decisive scroll moves one section, not several', () {
      expect(advanceTarget(1, 900, 5), 2);
      expect(advanceTarget(1, -900, 5), 0);
    });

    test('it cannot run off either end', () {
      expect(advanceTarget(0, -500, 3), 0);
      expect(advanceTarget(2, 500, 3), 2);
    });

    test('an empty stage has nowhere to go', () {
      expect(advanceTarget(0, 500, 0), 0);
    });
  });

  group('rendering', () {
    Future<void> pump(WidgetTester tester, int count) async {
      await tester.binding.setSurfaceSize(const Size(900, 600));
      await tester.pumpWidget(
        MaterialApp(
          theme: CrawlStatTheme.dark(),
          home: Scaffold(
            body: SectionStage(
              sections: [
                for (var i = 0; i < count; i++)
                  SizedBox(height: 200, child: Text('Section $i')),
              ],
              labels: [for (var i = 0; i < count; i++) 'Label $i'],
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
    }

    testWidgets('shows the first section and no scrollbar', (tester) async {
      await pump(tester, 3);
      expect(find.text('Section 0'), findsOneWidget);
      expect(find.byType(Scrollbar), findsNothing);
    });

    testWidgets('an empty stage renders nothing rather than throwing',
        (tester) async {
      await pump(tester, 0);
      expect(find.byType(PageView), findsNothing);
    });

    testWidgets('a section taller than the stage can still be scrolled',
        (tester) async {
      // The thing this must never do is hide content. Snapping that stranded
      // half a paragraph off-screen would be worse than the scrollbar.
      await tester.binding.setSurfaceSize(const Size(900, 400));
      await tester.pumpWidget(
        MaterialApp(
          theme: CrawlStatTheme.dark(),
          home: Scaffold(
            body: SectionStage(
              sections: [
                Column(
                  children: [
                    for (var i = 0; i < 20; i++)
                      SizedBox(height: 60, child: Text('Row $i')),
                  ],
                ),
              ],
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      // `findsNothing` would be the wrong check here: SingleChildScrollView
      // builds every child whether or not it is on screen, so the question is
      // whether the content *moves*, not whether it exists.
      final before = tester.getTopLeft(find.text('Row 19')).dy;
      expect(before, greaterThan(400), reason: 'the last row starts below the fold');

      // 1,200px of content in a 400px stage: the drag has to cover the lot.
      await tester.drag(find.text('Row 0'), const Offset(0, -1000));
      await tester.pumpAndSettle();

      final after = tester.getTopLeft(find.text('Row 19')).dy;
      expect(after, lessThan(before), reason: 'the section scrolled inside itself');
      expect(after, lessThan(400), reason: 'the last row is now reachable');
    });

    testWidgets('arrow keys move between sections', (tester) async {
      await pump(tester, 3);
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
      await tester.pumpAndSettle();
      expect(find.text('Section 1'), findsOneWidget);

      await tester.sendKeyEvent(LogicalKeyboardKey.arrowUp);
      await tester.pumpAndSettle();
      expect(find.text('Section 0'), findsOneWidget);
    });

    testWidgets('End jumps to the last section', (tester) async {
      await pump(tester, 4);
      await tester.sendKeyEvent(LogicalKeyboardKey.end);
      await tester.pumpAndSettle();
      expect(find.text('Section 3'), findsOneWidget);
    });

    testWidgets('one progress mark per section', (tester) async {
      await pump(tester, 4);
      expect(find.byType(Tooltip), findsNWidgets(4));
    });
  });
}
