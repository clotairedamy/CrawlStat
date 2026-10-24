import 'dart:math' as math;

import 'package:crawlstat/src/widgets/gauge.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('gaugeCeiling', () {
    test('rounds a peak up to a readable scale', () {
      expect(gaugeCeiling(43), 50);
      expect(gaugeCeiling(180), 200);
      expect(gaugeCeiling(1340), 2000);
    });

    test('never sits below the peak it has to show', () {
      for (final peak in [11.0, 47.0, 99.0, 101.0, 777.0, 9999.0]) {
        expect(
          gaugeCeiling(peak),
          greaterThanOrEqualTo(peak),
          reason: 'peak $peak',
        );
      }
    });

    test('a slow or idle crawl still gets a usable dial', () {
      // Scaling a 0.4/s crawl to 0.4 would pin the needle at full.
      expect(gaugeCeiling(0), 10);
      expect(gaugeCeiling(0.4), 10);
      expect(gaugeCeiling(-5), 10);
    });

    test('is stable across nearby peaks, so the scale does not twitch', () {
      // The dial must not re-scale on every reading, or the needle position
      // stops meaning anything.
      expect(gaugeCeiling(51), gaugeCeiling(76));
      expect(gaugeCeiling(21), gaugeCeiling(24));
    });
  });

  group('gaugeAngle', () {
    test('zero sits at the start of the dial and the maximum at its end', () {
      expect(gaugeAngle(0, 100), closeTo(gaugeStart, 1e-9));
      expect(gaugeAngle(100, 100), closeTo(gaugeStart + gaugeSweep, 1e-9));
      expect(gaugeAngle(50, 100), closeTo(gaugeStart + gaugeSweep / 2, 1e-9));
    });

    test(
      'the dial spans three quarters of a turn, leaving a gap at the foot',
      () {
        // A full circle would make the start and end indistinguishable.
        expect(gaugeSweep, closeTo(math.pi * 1.5, 1e-9));
        expect(gaugeSweep, lessThan(2 * math.pi));
      },
    );

    test('a reading past the maximum pins rather than wrapping round', () {
      expect(gaugeAngle(500, 100), closeTo(gaugeStart + gaugeSweep, 1e-9));
    });

    test('a zero scale does not divide by zero', () {
      expect(gaugeAngle(5, 0), gaugeStart);
    });
  });

  group('rampColourAt', () {
    // Plain colours, not MaterialColors: Color.lerp returns a plain Color,
    // and a MaterialColor never equals one even with identical channels.
    const red = Color(0xFFFF0000);
    const orange = Color(0xFFFF8800);
    const yellow = Color(0xFFFFFF00);
    const ramp = [red, orange, yellow];

    test('hits the end stops exactly', () {
      expect(rampColourAt(0, ramp), red);
      expect(rampColourAt(1, ramp), yellow);
    });

    test('interpolates between stops rather than stepping', () {
      final middle = rampColourAt(0.5, ramp);
      expect(middle, orange);
      final quarter = rampColourAt(0.25, ramp);
      expect(quarter, isNot(red));
      expect(quarter, isNot(orange));
    });

    test('degenerate ramps do not crash the painter', () {
      expect(rampColourAt(0.5, const []), Colors.white);
      expect(rampColourAt(0.5, const [red]), red);
      // Out of range is clamped, not wrapped.
      expect(rampColourAt(-1, ramp), red);
      expect(rampColourAt(9, ramp), yellow);
    });
  });

  group('rendering', () {
    testWidgets('shows the reading, its unit and the secondary figure', (
      tester,
    ) async {
      await tester.pumpWidget(
        const MaterialApp(
          home: Scaffold(
            body: Center(
              child: CrawlGauge(
                value: 42.5,
                max: 100,
                progress: 0.5,
                secondaryValue: '1,204',
                secondaryLabel: 'crawled',
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('42.5'), findsOneWidget);
      expect(find.text('URL/s'), findsOneWidget);
      expect(find.text('1,204'), findsOneWidget);
      expect(find.text('crawled'), findsOneWidget);
    });

    testWidgets('drops the decimal once the rate is large', (tester) async {
      await tester.pumpWidget(
        const MaterialApp(
          home: Scaffold(
            body: Center(child: CrawlGauge(value: 1240.7, max: 2000)),
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.text('1241'), findsOneWidget);
    });

    testWidgets('an idle dial renders without a progress arc', (tester) async {
      await tester.pumpWidget(
        const MaterialApp(
          home: Scaffold(
            body: Center(child: CrawlGauge(value: 0, max: 10, live: false)),
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      expect(find.text('0.0'), findsOneWidget);
    });
  });
}
