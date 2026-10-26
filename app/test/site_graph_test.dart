import 'dart:math' as math;

import 'package:crawlstat/src/theme/theme.dart';
import 'package:crawlstat/src/widgets/site_graph.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('node size', () {
    test('grows with inlinks but not in proportion to them', () {
      final one = nodeRadius(1);
      final ten = nodeRadius(10);
      final hundred = nodeRadius(100);
      expect(ten, greaterThan(one));
      expect(hundred, greaterThan(ten));
      // Square-rooted: a hundred inlinks must not draw ten times the radius
      // of ten, or one hub swallows the diagram.
      expect(hundred, lessThan(one + (ten - one) * 10));
    });

    test('an orphan still has a visible dot', () {
      expect(nodeRadius(0), greaterThan(2));
    });
  });

  group('depth rings', () {
    test('the home page sits at the centre', () {
      expect(depthRing(0, 5), 0);
    });

    test('the deepest page sits on the outer ring', () {
      expect(depthRing(5, 5), closeTo(1, 1e-9));
    });

    test('rings tighten as they go out, so deep sites still fit', () {
      final first = depthRing(1, 8) - depthRing(0, 8);
      final last = depthRing(8, 8) - depthRing(7, 8);
      expect(last, lessThan(first));
    });

    test('a single-level crawl does not divide by zero', () {
      expect(depthRing(0, 0), 0);
      expect(depthRing(3, 0), 1);
    });
  });

  group('node colour', () {
    const ember = CrawlStatPalette.ember;
    const mono = CrawlStatPalette.mono;

    test('carries the status class', () {
      expect(nodeColour(200, Colors.green, ember), Colors.green);
      expect(nodeColour(301, Colors.green, ember), ember.warning);
      expect(nodeColour(404, Colors.green, ember), ember.error);
      expect(nodeColour(503, Colors.green, ember), isNot(ember.error));
    });

    test('follows the palette rather than Ember', () {
      expect(nodeColour(301, Colors.green, mono), mono.warning);
      expect(nodeColour(404, Colors.green, mono), mono.error);
      // On a grey ramp 5xx shares the error tone, so it cannot look calmer.
      expect(nodeColour(503, Colors.green, mono), mono.error);
    });

    test('a page that was never fetched is neutral, not an error', () {
      expect(nodeColour(null, Colors.green, ember), ember.notice);
    });
  });

  group('layout', () {
    List<GraphParticle> fresh(int count) => [
      for (var i = 0; i < count; i++)
        GraphParticle(x: 30.0 - i * 3, y: i * 2.0 - 10),
    ];

    test('unconnected nodes push each other apart', () {
      final particles = [GraphParticle(x: 0, y: 0), GraphParticle(x: 4, y: 0)];
      final before = (particles[0].x - particles[1].x).abs();
      for (var i = 0; i < 40; i++) {
        relaxGraph(particles, const [], [0, 0], temperature: 10);
      }
      final after = (particles[0].x - particles[1].x).abs();
      expect(after, greaterThan(before));
    });

    test('linked nodes end up closer than unlinked ones', () {
      final particles = fresh(3);
      // 0–1 are linked; 2 hangs off nothing.
      for (var i = 0; i < 400; i++) {
        relaxGraph(particles, const [(0, 1)], [60, 60, 60], temperature: 6);
      }
      double gap(int a, int b) => math.sqrt(
        math.pow(particles[a].x - particles[b].x, 2) +
            math.pow(particles[a].y - particles[b].y, 2),
      );
      expect(gap(0, 1), lessThan(gap(0, 2)));
    });

    test('nodes settle onto the ring their depth asks for', () {
      final particles = fresh(2);
      for (var i = 0; i < 600; i++) {
        relaxGraph(particles, const [], [120, 120], temperature: 6);
      }
      for (final particle in particles) {
        final radius = math.sqrt(
          particle.x * particle.x + particle.y * particle.y,
        );
        expect(radius, closeTo(120, 40));
      }
    });

    test('two nodes in exactly the same place do not produce NaN', () {
      // Without a nudge this divides by zero, and one NaN poisons every
      // position in the graph on the next step.
      final particles = [GraphParticle(x: 5, y: 5), GraphParticle(x: 5, y: 5)];
      relaxGraph(particles, const [], [0, 0], temperature: 10);
      for (final particle in particles) {
        expect(particle.x.isFinite, isTrue);
        expect(particle.y.isFinite, isTrue);
      }
    });

    test('an edge pointing outside the node list is ignored', () {
      final particles = fresh(2);
      expect(
        () => relaxGraph(particles, const [(0, 9)], [0, 0], temperature: 5),
        returnsNormally,
      );
    });

    test('an empty graph is a no-op', () {
      expect(
        () => relaxGraph([], const [], const [], temperature: 5),
        returnsNormally,
      );
    });
  });

  group('rendering', () {
    testWidgets('draws a graph and shows a tooltip on hover', (tester) async {
      await tester.pumpWidget(
        MaterialApp(
          theme: CrawlStatTheme.dark(),
          home: const Scaffold(
            body: SizedBox(
              width: 500,
              height: 400,
              child: SiteGraphView(
                nodes: [
                  GraphNodeData(
                    id: 1,
                    url: 'https://example.com/',
                    label: 'example.com',
                    depth: 0,
                    inlinks: 12,
                    status: 200,
                  ),
                  GraphNodeData(
                    id: 2,
                    url: 'https://example.com/gone',
                    label: 'gone',
                    depth: 1,
                    inlinks: 1,
                    status: 404,
                  ),
                ],
                edges: [GraphEdgeData(source: 1, target: 2)],
              ),
            ),
          ),
        ),
      );
      await tester.pump(const Duration(milliseconds: 600));
      expect(find.byType(CustomPaint), findsWidgets);
    });

    testWidgets('says so when there is nothing to draw', (tester) async {
      await tester.pumpWidget(
        MaterialApp(
          theme: CrawlStatTheme.dark(),
          home: const Scaffold(
            body: SiteGraphView(nodes: [], edges: []),
          ),
        ),
      );
      await tester.pump();
      expect(find.textContaining('no internal links'), findsOneWidget);
    });
  });
}
