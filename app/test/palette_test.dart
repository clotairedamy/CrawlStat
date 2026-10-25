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

  test('an unknown palette name falls back rather than throwing', () {
    expect(CrawlStatPalette.byName('nonsense').name, 'ember');
    expect(CrawlStatPalette.byName(null).name, 'ember');
  });

  test('the light variant keeps its name so selection still matches', () {
    expect(CrawlStatPalette.gold.forLight().name, 'gold');
  });
}
