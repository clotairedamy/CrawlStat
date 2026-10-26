import 'package:crawlstat/src/screens/settings_screen.dart';
import 'package:crawlstat/src/state/settings.dart';
import 'package:crawlstat/src/theme/theme.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

/// The tick in the palette chooser must follow the stored setting. It did not
/// — the chooser showed Gold selected while the app was running Ember — and
/// the only way to tell the difference was to pump it.
void main() {
  Future<void> pump(WidgetTester tester, String paletteName) async {
    await tester.pumpWidget(
      ProviderScope(
        child: MaterialApp(
          theme: CrawlStatTheme.dark(),
          home: Scaffold(
            body: PaletteChooser(
              settings: Settings(userAgent: 'test', paletteName: paletteName),
            ),
          ),
        ),
      ),
    );
    await tester.pump();
  }

  /// Which rows are showing the selected tick.
  List<String> ticked(WidgetTester tester) {
    return tester
        .widgetList<PaletteOption>(find.byType(PaletteOption))
        .where((option) => option.selected)
        .map((option) => option.name)
        .toList();
  }

  testWidgets('the tick follows the stored palette', (tester) async {
    await pump(tester, 'ember');
    expect(ticked(tester), ['ember']);

    await pump(tester, 'gold');
    expect(ticked(tester), ['gold']);
  });

  testWidgets('every palette gets a row, in order', (tester) async {
    await pump(tester, 'ember');
    final names = tester
        .widgetList<PaletteOption>(find.byType(PaletteOption))
        .map((option) => option.name)
        .toList();
    expect(names, CrawlStatPalette.all.map((p) => p.name).toList());
  });

  test('every palette can be found by its own name', () {
    // byName once compared the argument with itself, so every lookup matched
    // the first palette and the app ran Ember whatever was chosen.
    for (final palette in CrawlStatPalette.all) {
      expect(CrawlStatPalette.byName(palette.name), same(palette));
    }
  });

  group('monochrome', () {
    bool isGrey(Color c) => c.r == c.g && c.g == c.b;

    test('has no hue anywhere, in either brightness', () {
      for (final palette in [
        CrawlStatPalette.mono,
        CrawlStatPalette.mono.forLight(),
      ]) {
        for (final colour in [
          palette.accent,
          palette.highlight,
          palette.error,
          palette.warning,
          palette.notice,
          palette.success,
        ]) {
          expect(isGrey(colour), isTrue, reason: '$colour');
        }
      }
    });

    test('text on its pale accent is dark, not white', () {
      expect(CrawlStatPalette.mono.onAccent, isNot(Colors.white));
      expect(CrawlStatPalette.mono.forLight().onAccent, Colors.white);
    });

    test('ember and gold keep white text on the accent', () {
      expect(CrawlStatPalette.ember.onAccent, Colors.white);
      expect(CrawlStatPalette.gold.onAccent, Colors.white);
    });
  });

  test('an unknown palette name falls back rather than throwing', () {
    expect(CrawlStatPalette.byName('nonsense').name, 'ember');
    expect(CrawlStatPalette.byName(null).name, 'ember');
  });

  test('the light variant keeps its name so selection still matches', () {
    expect(CrawlStatPalette.gold.forLight().name, 'gold');
    expect(CrawlStatPalette.graphite.forLight().name, 'graphite');
  });

  group('graphite', () {
    test('reserves its colour for the call to action', () {
      // The whole point of the third palette: gold marks the one button
      // worth pressing, and marking anything else with it would put the
      // colour back in a dozen places.
      final palette = CrawlStatPalette.graphite;
      expect(palette.highlight, isNot(palette.accent));
    });

    test('the other two spend one colour on everything accented', () {
      for (final palette in [CrawlStatPalette.ember, CrawlStatPalette.gold]) {
        expect(palette.highlight, palette.accent, reason: palette.name);
      }
    });

    test('severity is ordered by lightness, worst first', () {
      // Hue carries nothing here, so the ramp has to be monotonic or the
      // severities stop being tellable apart.
      final p = CrawlStatPalette.graphite;
      double light(Color c) => (c.r + c.g + c.b) / 3;
      expect(light(p.error), greaterThan(light(p.warning)));
      expect(light(p.warning), greaterThan(light(p.notice)));
      expect(light(p.notice), greaterThan(light(p.success)));
    });

    test('the light theme inverts the ramp, because the page is pale', () {
      final p = CrawlStatPalette.graphite.forLight();
      double light(Color c) => (c.r + c.g + c.b) / 3;
      expect(light(p.error), lessThan(light(p.warning)));
      expect(light(p.warning), lessThan(light(p.notice)));
    });

    test('every palette is offered in Settings', () {
      expect(
        CrawlStatPalette.all.map((p) => p.name),
        containsAll(['ember', 'gold', 'graphite']),
      );
    });
  });
}
