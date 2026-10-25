import 'package:crawlstat/src/theme/theme.dart';
import 'package:crawlstat/src/widgets/reveal_list.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  Future<void> pump(WidgetTester tester, int count, {Size? size}) async {
    await tester.binding.setSurfaceSize(size ?? const Size(900, 700));
    await tester.pumpWidget(
      MaterialApp(
        theme: CrawlStatTheme.dark(),
        home: Scaffold(
          body: RevealList(
            children: [
              for (var i = 0; i < count; i++)
                SizedBox(height: 90, child: Text('Card $i')),
            ],
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
  }

  testWidgets('packs as many cards as fit, rather than one at a time',
      (tester) async {
    // The whole point of the rewrite: a 700px viewport showing one 90px card
    // in the middle of an empty field was worse than the list it replaced.
    await pump(tester, 10);
    final onScreen = tester
        .widgetList<Text>(find.byType(Text))
        .where((text) {
          final finder = find.text(text.data!);
          return finder.evaluate().isNotEmpty &&
              tester.getTopLeft(finder).dy < 700;
        })
        .length;
    expect(onScreen, greaterThan(4));
  });

  testWidgets('draws no scrollbar', (tester) async {
    await pump(tester, 20);
    expect(find.byType(Scrollbar), findsNothing);
  });

  testWidgets('everything is reachable by scrolling', (tester) async {
    await pump(tester, 20);
    await tester.drag(find.text('Card 0'), const Offset(0, -1400));
    await tester.pumpAndSettle();
    expect(find.text('Card 19'), findsOneWidget);
    expect(tester.getTopLeft(find.text('Card 19')).dy, lessThan(700));
  });

  testWidgets('each card ends up fully opaque once it has arrived',
      (tester) async {
    await pump(tester, 5);
    final opacities = tester
        .widgetList<Opacity>(find.byType(Opacity))
        .map((o) => o.opacity);
    expect(opacities, isNotEmpty);
    for (final value in opacities) {
      expect(value, 1.0);
    }
  });

  testWidgets('an empty list renders nothing rather than throwing',
      (tester) async {
    await pump(tester, 0);
    expect(find.byType(ListView), findsNothing);
  });

  testWidgets('scrolling an item away mid-animation does not throw',
      (tester) async {
    // The staggered items start a delayed callback; disposing one before it
    // fires used to drive a disposed controller.
    await tester.binding.setSurfaceSize(const Size(900, 300));
    await tester.pumpWidget(
      MaterialApp(
        theme: CrawlStatTheme.dark(),
        home: Scaffold(
          body: RevealList(
            children: [
              for (var i = 0; i < 40; i++)
                SizedBox(height: 90, child: Text('Card $i')),
            ],
          ),
        ),
      ),
    );
    await tester.pump(const Duration(milliseconds: 20));
    await tester.drag(find.text('Card 0'), const Offset(0, -2000));
    await tester.pumpAndSettle();
    expect(tester.takeException(), isNull);
  });
}
