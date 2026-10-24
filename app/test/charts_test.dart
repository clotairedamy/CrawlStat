import 'dart:math' as math;

import 'package:crawlstat/src/widgets/charts.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('donutSegments', () {
    test('lays slices out clockwise from twelve, filling the ring exactly', () {
      final segments = donutSegments([1, 1, 2]);

      expect(segments.first.start, 0);
      expect(segments[0].fraction, closeTo(0.25, 1e-9));
      expect(segments[1].fraction, closeTo(0.25, 1e-9));
      expect(segments[2].fraction, closeTo(0.5, 1e-9));

      // No gap and no overlap: each starts where the last ended, and the last
      // closes the circle.
      for (var index = 1; index < segments.length; index++) {
        expect(
          segments[index].start,
          closeTo(segments[index - 1].start + segments[index - 1].sweep, 1e-9),
        );
      }
      final last = segments.last;
      expect(last.start + last.sweep, closeTo(2 * math.pi, 1e-9));
    });

    test('a single value fills the whole ring', () {
      final segments = donutSegments([7]);
      expect(segments.single.sweep, closeTo(2 * math.pi, 1e-9));
      expect(segments.single.fraction, 1);
    });

    test('keeps zero values as zero-width slices, so indexes still line up', () {
      // The donut maps a hovered slice back to its datum by index, so dropping
      // empty entries would point hover at the wrong row.
      final segments = donutSegments([5, 0, 5]);
      expect(segments.length, 3);
      expect(segments[1].sweep, 0);
      expect(segments[2].start, closeTo(math.pi, 1e-9));
    });

    test(
      'an all-zero or empty series charts nothing rather than dividing by zero',
      () {
        expect(donutSegments([]), isEmpty);
        expect(donutSegments([0, 0]), isEmpty);
      },
    );

    test('negative values are ignored rather than inverting the ring', () {
      final segments = donutSegments([-5, 10]);
      expect(segments[0].sweep, 0);
      expect(segments[1].fraction, 1);
    });
  });

  group('niceAxisMax', () {
    test('rounds up to a number a person would pick', () {
      expect(niceAxisMax(43), 50);
      expect(niceAxisMax(1340), 1500);
      expect(niceAxisMax(7), 10);
      expect(niceAxisMax(99), 100);
      expect(niceAxisMax(230), 300);
    });

    test('never sits below the data it has to contain', () {
      for (final value in [1, 3, 17, 64, 128, 999, 1001, 45678]) {
        expect(
          niceAxisMax(value),
          greaterThanOrEqualTo(value),
          reason: 'axis clipped the largest bar at $value',
        );
      }
    });

    test('degenerate inputs still give a usable axis', () {
      // A zero axis would divide by zero when scaling the bars.
      expect(niceAxisMax(0), greaterThan(0));
      expect(niceAxisMax(-10), greaterThan(0));
    });
  });

  group('sparklinePoints', () {
    test('spreads points evenly and puts the newest last', () {
      final points = sparklinePoints([1, 2, 3], 100, 50);
      expect(points.length, 3);
      expect(points.first.dx, 0);
      expect(points[1].dx, closeTo(50, 1e-9));
      expect(points.last.dx, closeTo(100, 1e-9));
    });

    test('maps the highest value to the top and the lowest to the floor', () {
      final points = sparklinePoints([0, 10], 100, 50);
      // y grows downward, so the larger value gets the smaller y.
      expect(points.first.dy, closeTo(50, 1e-9));
      expect(points.last.dy, closeTo(0, 1e-9));
    });

    test('a flat series sits on the middle line, not on the floor', () {
      // Collapsing a steady rate to the baseline would read as "stopped".
      final points = sparklinePoints([5, 5, 5], 100, 50);
      expect(points.every((point) => point.dy == 25), isTrue);
    });

    test('too little to draw returns nothing rather than a broken path', () {
      expect(sparklinePoints([], 100, 50), isEmpty);
      expect(sparklinePoints([1], 100, 50), isEmpty);
      expect(sparklinePoints([1, 2], 0, 50), isEmpty);
    });
  });

  group('percentLabel', () {
    test('rounds to whole percents', () {
      expect(percentLabel(25, 100), '25%');
      expect(percentLabel(1, 3), '33%');
    });

    test('a sliver reads as <1% rather than as nothing', () {
      // "0%" next to a non-zero count looks like a bug.
      expect(percentLabel(1, 1000), '<1%');
    });

    test('nothing is 0%, and no total never divides by zero', () {
      expect(percentLabel(0, 100), '0%');
      expect(percentLabel(5, 0), '0%');
    });
  });

  group('rendering', () {
    Future<void> pump(WidgetTester tester, Widget child) async {
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(body: Center(child: child)),
        ),
      );
      await tester.pumpAndSettle();
    }

    testWidgets('a donut shows its total and a legend row per slice', (
      tester,
    ) async {
      await pump(
        tester,
        const DonutChart(
          data: [
            ChartDatum(label: '2xx', value: 90, color: Colors.green),
            ChartDatum(label: '4xx', value: 10, color: Colors.red),
          ],
          centreLabel: 'URLs',
        ),
      );

      expect(find.text('100'), findsOneWidget);
      expect(find.text('2xx'), findsOneWidget);
      expect(find.text('4xx'), findsOneWidget);
      expect(find.text('90%'), findsOneWidget);
      expect(find.text('10%'), findsOneWidget);
    });

    testWidgets('an empty donut says so instead of drawing a blank ring', (
      tester,
    ) async {
      await pump(tester, const DonutChart(data: []));
      expect(find.textContaining('Nothing to chart'), findsOneWidget);
    });

    testWidgets('a bar chart labels every bar', (tester) async {
      await pump(
        tester,
        const BarChart(
          data: [
            ChartDatum(label: '0', value: 1, color: Colors.blue),
            ChartDatum(label: '1', value: 12, color: Colors.blue),
            ChartDatum(label: '2', value: 40, color: Colors.blue),
          ],
        ),
      );

      expect(find.text('0'), findsWidgets);
      expect(find.text('1'), findsOneWidget);
      expect(find.text('2'), findsOneWidget);
      // The axis tops out above the tallest bar, rounded.
      expect(find.text('50'), findsOneWidget);
    });

    testWidgets('a healthy severity bar renders without any issues', (
      tester,
    ) async {
      await pump(tester, const SeverityBar(errors: 0, warnings: 0, notices: 0));
      expect(tester.takeException(), isNull);
    });

    testWidgets('a sparkline needs two points before it draws', (tester) async {
      await pump(tester, const Sparkline(values: [1]));
      expect(tester.takeException(), isNull);
      await pump(tester, const Sparkline(values: [1, 5, 3, 9]));
      expect(tester.takeException(), isNull);
    });
  });
}
