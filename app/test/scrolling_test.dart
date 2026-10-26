import 'package:crawlstat/src/theme/scrolling.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  Future<void> pump(WidgetTester tester, {int count = 40}) async {
    await tester.binding.setSurfaceSize(const Size(600, 400));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      MaterialApp(
        scrollBehavior: const CrawlStatScrollBehavior(),
        home: Scaffold(
          body: ListView(
            children: [
              for (var i = 0; i < count; i++)
                SizedBox(height: 50, child: Text('Row $i')),
            ],
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
  }

  // Desktop is where Material adds scrollbars by default, so that is where
  // their absence has to be checked.
  testWidgets('no scrollbar on desktop, an edge fade instead', (tester) async {
    await pump(tester);
    expect(find.byType(RawScrollbar), findsNothing);
    expect(find.byType(EdgeFade), findsOneWidget);
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));

  testWidgets('scrolling keeps its place as the fade switches on', (
    tester,
  ) async {
    await pump(tester);
    await tester.drag(find.text('Row 0'), const Offset(0, -300));
    await tester.pumpAndSettle();
    // Had the fade swapped a widget into the tree, the scrollable would have
    // been rebuilt and jumped back to the top.
    expect(find.text('Row 0'), findsNothing);
    expect(find.text('Row 7'), findsOneWidget);
  }, variant: TargetPlatformVariant.only(TargetPlatform.macOS));
}
