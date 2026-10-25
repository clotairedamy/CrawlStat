import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

/// CrawlStat's design system.
///
/// A calm dashboard: content sits in rounded cards on a deep canvas, with the
/// navigation in a sidebar that collapses to a rail when the table needs the
/// room. Three rules hold it together:
///
/// * **Cards, not strips.** Every block of content is a card with the same
///   radius and padding, floating on the canvas. Alignment comes from the
///   shared [CrawlStatTheme.gutter], not from borders between full-width bars.
/// * **Numbers are monospaced.** Status codes, byte counts and URLs line up
///   column to column, so the eye can scan down instead of reading across.
/// * **One accent.** Colour carries meaning here — severity, status class — so
///   spending it on decoration would make the meaningful uses invisible.
class CrawlStatTheme {
  const CrawlStatTheme._();

  /// Row height in data grids. Tight enough to show ~25 rows on a laptop.
  static const double rowHeight = 34;

  /// Header height in data grids.
  static const double headerHeight = 38;

  /// Space between the window edge and the cards, and between cards. Every
  /// screen uses it, which is what keeps their edges lined up.
  static const double gutter = 24;

  /// Corner radius of cards.
  ///
  /// Large on purpose. Soft UI reads as moulded rather than drawn, and a
  /// tight 8pt corner immediately looks like a stock dashboard again — the
  /// radius is doing as much of the work here as the shadows are.
  static const double radius = 20;

  /// Corner radius of buttons, inputs and chips inside a card.
  static const double controlRadius = 13;

  /// Fully round, for pills and circular icon buttons.
  static const double pillRadius = 999;

  /// The default accent. Kept as a constant so const widgets still compile;
  /// the live value for a screen comes from [palette].
  static const Color accent = Color(0xFFFF6A14);

  /// Severity colours for the default theme.
  static const Color error = Color(0xFFF2543D);
  static const Color warning = Color(0xFFE8A33D);
  static const Color notice = Color(0xFF8A8A94);
  static const Color success = Color(0xFF4ABD7F);

  /// The palette in force, which depends on the chosen theme.
  ///
  /// Call this rather than the constants above wherever a `BuildContext` is
  /// at hand — the constants are the Ember theme's values and do not follow
  /// the user's choice.
  static CrawlStatPalette palette(BuildContext context) =>
      Theme.of(context).extension<CrawlStatPalette>() ?? CrawlStatPalette.ember;

  /// The interface typeface.
  ///
  /// Outfit, bundled rather than loaded at runtime: a crawler that works on a
  /// plane cannot wait on a font CDN, and the app is the same on every
  /// platform instead of three different system faces. It is the same
  /// typeface as crawlstat.com, so the product and the site stop looking like
  /// two companies.
  ///
  /// It is a variable font, so every weight from 100 to 900 costs one 108 KB
  /// file rather than one file per weight.
  static const String fontFamily = 'Outfit';

  /// The system face, kept as a fallback for scripts Outfit does not cover.
  static List<String> get fontFallback => switch (defaultTargetPlatform) {
    TargetPlatform.macOS || TargetPlatform.iOS => const ['.AppleSystemUIFont'],
    TargetPlatform.windows => const ['Segoe UI'],
    _ => const [],
  };

  static ThemeData light([CrawlStatPalette palette = CrawlStatPalette.ember]) =>
      _build(
        palette: palette.forLight(),
        brightness: Brightness.light,
        surface: const Color(0xFFECECEF),
        canvas: const Color(0xFFE2E2E6),
        panel: const Color(0xFFF3F3F6),
        raised: const Color(0xFFF7F7FA),
        border: const Color(0xFFDBDBE1),
        text: const Color(0xFF1A1A1D),
        subdued: const Color(0xFF75757E),
        hover: const Color(0xFFE9E9EE),
      );

  static ThemeData dark([CrawlStatPalette palette = CrawlStatPalette.ember]) =>
      _build(
        palette: palette,
        brightness: Brightness.dark,
        surface: const Color(0xFF26262A),
        canvas: const Color(0xFF1B1B1E),
        panel: const Color(0xFF2E2E33),
        raised: const Color(0xFF313137),
        // Nearly invisible: rows are separated by a whisper, not a rule.
        border: const Color(0xFF2C2C31),
        text: const Color(0xFFF3F3F5),
        // Neutral grey. The old subdued was blue-tinted, which is what made
        // the whole app read cold next to the reference's warm charcoal.
        subdued: const Color(0xFF8B8B93),
        hover: const Color(0xFF303036),
      );

  static ThemeData _build({
    required CrawlStatPalette palette,
    required Brightness brightness,
    required Color surface,
    required Color canvas,
    required Color panel,
    required Color raised,
    required Color border,
    required Color text,
    required Color subdued,
    required Color hover,
  }) {
    final colors = ColorScheme(
      brightness: brightness,
      primary: palette.accent,
      onPrimary: Colors.white,
      secondary: palette.highlight,
      onSecondary: Colors.white,
      error: palette.error,
      onError: Colors.white,
      surface: surface,
      onSurface: text,
      surfaceContainerLowest: canvas,
      surfaceContainerLow: panel,
      surfaceContainer: raised,
      surfaceContainerHigh: raised,
      outline: border,
      outlineVariant: border,
      onSurfaceVariant: subdued,
    );

    // 13px base: the density a data tool needs without becoming unreadable.
    const baseSize = 13.0;
    final body = TextStyle(
      fontFamily: fontFamily,
      fontFamilyFallback: fontFallback,
      fontSize: baseSize,
      height: 1.35,
      color: text,
    );
    const controlShape = RoundedRectangleBorder(
      borderRadius: BorderRadius.all(Radius.circular(controlRadius)),
    );

    return ThemeData(
      extensions: [palette],
      useMaterial3: true,
      brightness: brightness,
      colorScheme: colors,
      fontFamily: fontFamily,
      scaffoldBackgroundColor: canvas,
      canvasColor: canvas,
      dividerColor: border,
      splashFactory: NoSplash.splashFactory,
      // A pointer-driven app does not need ink ripples; they read as lag.
      // Both of these are transparent on purpose: Material paints them as a
      // *rectangle* behind the child, which on a rounded panel appears as a
      // grey box around the corners. Surfaces that respond to the pointer do
      // it themselves, by lifting their own shadow.
      highlightColor: Colors.transparent,
      hoverColor: Colors.transparent,
      visualDensity: VisualDensity.compact,
      dividerTheme: DividerThemeData(color: border, thickness: 1, space: 1),
      textTheme: TextTheme(
        bodyMedium: body,
        bodySmall: body.copyWith(fontSize: 12, color: subdued),
        titleSmall: body.copyWith(fontSize: 14, fontWeight: FontWeight.w600),
        titleMedium: body.copyWith(fontSize: 15, fontWeight: FontWeight.w600),
        // Screen titles are big and heavy, with a quiet line under them. That
        // contrast — one loud thing, one soft thing — is the typographic half
        // of the look; a 22pt semibold title reads as a settings pane.
        titleLarge: body.copyWith(
          fontSize: 27,
          fontWeight: FontWeight.w700,
          height: 1.15,
          letterSpacing: -0.45,
        ),
        labelLarge: body.copyWith(fontWeight: FontWeight.w500),
        labelMedium: body.copyWith(
          fontSize: 11,
          fontWeight: FontWeight.w600,
          color: subdued,
          letterSpacing: 0.5,
        ),
      ),
      iconTheme: IconThemeData(size: 16, color: subdued),
      inputDecorationTheme: InputDecorationTheme(
        isDense: true,
        filled: true,
        fillColor: raised,
        hintStyle: body.copyWith(color: subdued),
        contentPadding: const EdgeInsets.symmetric(
          horizontal: 12,
          vertical: 11,
        ),
        // A field is a recess, so its resting edge is a shade *darker* than
        // the surface rather than a drawn line.
        border: _inputBorder(Color.lerp(surface, Colors.black, 0.18)!),
        enabledBorder: _inputBorder(Color.lerp(surface, Colors.black, 0.18)!),
        focusedBorder: _inputBorder(palette.highlight, width: 1.5),
        errorBorder: _inputBorder(palette.error),
      ),
      filledButtonTheme: FilledButtonThemeData(
        style: FilledButton.styleFrom(
          textStyle: body.copyWith(
            fontWeight: FontWeight.w600,
            letterSpacing: 0.2,
          ),
          shape: const StadiumBorder(),
          padding: const EdgeInsets.symmetric(horizontal: 22, vertical: 15),
          minimumSize: const Size(0, 40),
          // The halo is cast in the palette.accent, not in black, so the pill looks
          // lit rather than merely elevated.
          elevation: 10,
          shadowColor: palette.accent.withValues(alpha: 0.55),
        ),
      ),
      outlinedButtonTheme: OutlinedButtonThemeData(
        style: OutlinedButton.styleFrom(
          foregroundColor: text,
          backgroundColor: panel,
          // Borderless: a secondary button is a soft slab, raised off the
          // page by its shadow alone.
          side: BorderSide.none,
          textStyle: body.copyWith(fontWeight: FontWeight.w500),
          shape: const StadiumBorder(),
          padding: const EdgeInsets.symmetric(horizontal: 20, vertical: 13),
          minimumSize: const Size(0, 40),
          elevation: 6,
          shadowColor: Colors.black.withValues(
            alpha: brightness == Brightness.dark ? 0.6 : 0.22,
          ),
        ),
      ),
      textButtonTheme: TextButtonThemeData(
        style: TextButton.styleFrom(
          foregroundColor: palette.highlight,
          textStyle: body.copyWith(fontWeight: FontWeight.w500),
          shape: controlShape,
          minimumSize: const Size(0, 32),
          padding: const EdgeInsets.symmetric(horizontal: 10),
        ),
      ),
      segmentedButtonTheme: SegmentedButtonThemeData(
        style: SegmentedButton.styleFrom(
          textStyle: body.copyWith(fontWeight: FontWeight.w500),
          backgroundColor: raised,
          foregroundColor: subdued,
          selectedBackgroundColor: palette.highlight,
          selectedForegroundColor: Colors.white,
          side: BorderSide.none,
          shape: const StadiumBorder(),
        ),
      ),
      switchTheme: SwitchThemeData(
        thumbColor: WidgetStateProperty.resolveWith(
          (states) =>
              states.contains(WidgetState.selected) ? Colors.white : subdued,
        ),
        trackColor: WidgetStateProperty.resolveWith(
          (states) => states.contains(WidgetState.selected)
              ? palette.highlight
              : raised,
        ),
        trackOutlineColor: WidgetStateProperty.resolveWith(
          (states) => states.contains(WidgetState.selected)
              ? palette.highlight
              : border,
        ),
      ),
      popupMenuTheme: PopupMenuThemeData(
        color: panel,
        elevation: 8,
        shadowColor: Colors.black.withValues(alpha: 0.4),
        textStyle: body,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(controlRadius + 3),
        ),
      ),
      snackBarTheme: SnackBarThemeData(
        behavior: SnackBarBehavior.floating,
        backgroundColor: raised,
        contentTextStyle: body,
        actionTextColor: palette.accent,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(controlRadius + 3),
        ),
      ),
      tooltipTheme: TooltipThemeData(
        waitDuration: const Duration(milliseconds: 500),
        textStyle: body.copyWith(color: Colors.white, fontSize: 12),
        decoration: BoxDecoration(
          color: const Color(0xFF0D0F1E).withValues(alpha: 0.96),
          borderRadius: BorderRadius.circular(6),
        ),
      ),
      scrollbarTheme: ScrollbarThemeData(
        thickness: const WidgetStatePropertyAll(7),
        radius: const Radius.circular(4),
        thumbColor: WidgetStatePropertyAll(subdued.withValues(alpha: 0.35)),
      ),
      cardTheme: CardThemeData(
        color: panel,
        elevation: 0,
        margin: EdgeInsets.zero,
        // Borderless on purpose: in soft UI an edge is implied by light, not
        // drawn. Anything still using a raw Card gets the shape without an
        // outline that would clash with the panels around it.
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(radius),
        ),
      ),
      dialogTheme: DialogThemeData(
        backgroundColor: surface,
        elevation: 0,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(radius + 4),
        ),
      ),
      progressIndicatorTheme: ProgressIndicatorThemeData(
        linearMinHeight: 4,
        color: palette.accent,
        linearTrackColor: raised,
      ),
    );
  }

  static OutlineInputBorder _inputBorder(Color color, {double width = 1}) {
    return OutlineInputBorder(
      borderRadius: BorderRadius.circular(controlRadius),
      borderSide: BorderSide(color: color, width: width),
    );
  }

  /// The style for URLs, numbers and anything that should align in a column.
  static TextStyle mono(BuildContext context, {Color? color, double? size}) {
    return TextStyle(
      // Bundled rather than borrowed from the OS: the system monospace
      // differs on every platform, and a table of URLs and byte counts is
      // exactly where that shows. JetBrains Mono also disambiguates 0/O and
      // 1/l/I, which matters when the column is a status code.
      fontFamily: 'JetBrainsMono',
      fontFamilyFallback: const [
        'SF Mono',
        'Menlo',
        'Consolas',
        'DejaVu Sans Mono',
        'monospace',
      ],
      fontSize: size ?? 12,
      height: 1.3,
      color: color ?? Theme.of(context).colorScheme.onSurface,
    );
  }

  /// Colour for an HTTP status code: 2xx green, 3xx palette.accent, 4xx/5xx red.
  /// The colour for an HTTP status, from the theme in force.
  ///
  /// This used to be static, which meant every status chip, grid cell and
  /// severity dot in the app kept the Ember palette's red and green however
  /// the theme was set. A colour that carries meaning has to follow the
  /// palette or the palette is decoration.
  static Color statusColor(BuildContext context, int? status) {
    final palette = CrawlStatTheme.palette(context);
    if (status == null) return palette.notice;
    if (status >= 400) return palette.error;
    if (status >= 300) return palette.warning;
    if (status >= 200) return palette.success;
    return palette.notice;
  }

  static Color severityColor(BuildContext context, String severity) {
    final palette = CrawlStatTheme.palette(context);
    return switch (severity) {
      'error' => palette.error,
      'warning' => palette.warning,
      _ => palette.notice,
    };
  }
}

/// Shortcuts used across the app, declared in one place so the Settings screen
/// can list them and nothing drifts.
class CrawlStatShortcuts {
  const CrawlStatShortcuts._();

  static const newCrawl = SingleActivator(LogicalKeyboardKey.keyN, meta: true);
  static const openProject = SingleActivator(
    LogicalKeyboardKey.keyO,
    meta: true,
  );
  static const focusFilter = SingleActivator(
    LogicalKeyboardKey.keyF,
    meta: true,
  );
  static const exportCsv = SingleActivator(LogicalKeyboardKey.keyE, meta: true);
  static const pauseResume = SingleActivator(
    LogicalKeyboardKey.space,
    meta: true,
  );
  static const closePanel = SingleActivator(LogicalKeyboardKey.escape);
}

/// The colours that carry meaning, as a theme extension.
///
/// Severity and the palette.accent are the only places colour says something in
/// CrawlStat, so they are the only things a theme is allowed to change. The
/// surfaces stay the same soft charcoal (or the same soft light) in every
/// theme — swapping those as well would be a different app, not a theme.
@immutable
class CrawlStatPalette extends ThemeExtension<CrawlStatPalette> {
  const CrawlStatPalette({
    required this.name,
    required this.accent,
    Color? highlight,
    required this.error,
    required this.warning,
    required this.notice,
    required this.success,
  }) : highlight = highlight ?? accent;

  final String name;

  /// The primary action. Spent on one button per screen.
  final Color accent;

  /// Selection, focus rings, switch tracks — everything that is *marked*
  /// rather than *offered*. Defaults to [accent]; a theme sets it separately
  /// when it wants its colour reserved for the call to action alone.
  final Color highlight;
  final Color error;
  final Color warning;
  final Color notice;
  final Color success;

  /// The original: a vivid ember accent with red, amber and green severities.
  static const CrawlStatPalette ember = CrawlStatPalette(
    name: 'ember',
    accent: Color(0xFFFF6A14),
    error: Color(0xFFF2543D),
    warning: Color(0xFFE8A33D),
    notice: Color(0xFF8A8A94),
    success: Color(0xFF4ABD7F),
  );

  /// Gold on graphite: the website's copper accent, and severities told apart
  /// by lightness instead of hue.
  ///
  /// Three reasons this is not just a quieter skin:
  ///
  /// * **Hue was doing too much.** Red, amber and green at full strength on
  ///   every screen is a lot of shouting for a tool somebody has open all day,
  ///   and when everything is coloured nothing reads as urgent.
  /// * **Lightness survives colour blindness.** Roughly one man in twelve
  ///   cannot reliably separate the red and the green above. A ramp from near
  ///   white to mid grey is legible to everyone, and still ordered.
  /// * **One accent, spent carefully.** Gold marks the primary action and
  ///   nothing else, so it keeps meaning something.
  ///
  /// The four severities are set three steps apart in lightness, which stays
  /// distinguishable side by side and in a dense table.
  static const CrawlStatPalette gold = CrawlStatPalette(
    name: 'gold',
    // crawlstat.com's copper, unchanged.
    accent: Color(0xFFE3AD7B),
    // Error is the brightest: in a dark table the eye goes to the lightest
    // thing, which is what an error should be.
    error: Color(0xFFF0EFEC),
    warning: Color(0xFFBDB9B2),
    notice: Color(0xFF7E7B76),
    // Success is deliberately the quietest. "Fine" does not need attention.
    success: Color(0xFF5E5B57),
  );

  /// Gold for the one button that matters, and nothing else coloured at all.
  ///
  /// The difference from [gold] is where the colour is allowed to appear.
  /// There, the accent marks selection, focus and switches as well, so a busy
  /// screen ends up with gold in a dozen places and the primary action stops
  /// standing out. Here it appears once: on the call to action. Everything
  /// else — selected rows, focus rings, severity — is a step on a grey ramp.
  ///
  /// The ramp runs light to dark on purpose. In a dark interface the eye goes
  /// to the lightest thing, so the most serious finding is the brightest, and
  /// "fine" is the quietest thing on the screen.
  static const CrawlStatPalette graphite = CrawlStatPalette(
    name: 'graphite',
    accent: Color(0xFFE3AD7B),
    // A bright neutral, clearly not the gold, so selection never reads as a
    // button you are supposed to press.
    highlight: Color(0xFFD9D6D0),
    error: Color(0xFFF4F3F0),
    warning: Color(0xFFC3BFB8),
    notice: Color(0xFF8A8782),
    success: Color(0xFF615E5A),
  );

  static const List<CrawlStatPalette> all = [ember, gold, graphite];

  /// The light theme needs the ramp inverted: on a pale canvas the *darkest*
  /// tone is the one that draws the eye.
  CrawlStatPalette forLight() {
    if (name == 'graphite') {
      return const CrawlStatPalette(
        name: 'graphite',
        accent: Color(0xFFA9712F),
        highlight: Color(0xFF3A3833),
        error: Color(0xFF141311),
        warning: Color(0xFF4E4B46),
        notice: Color(0xFF86837D),
        success: Color(0xFFB4B0A9),
      );
    }
    if (name != 'gold') return this;
    return const CrawlStatPalette(
      name: 'gold',
      accent: Color(0xFFA9712F),
      error: Color(0xFF171614),
      warning: Color(0xFF55524D),
      notice: Color(0xFF8A8781),
      success: Color(0xFFAEAAA3),
    );
  }

  String get label => switch (name) {
    'gold' => 'Gold on graphite',
    'graphite' => 'Graphite',
    _ => 'Ember',
  };

  String get describes => switch (name) {
    'graphite' =>
      'Gold on the call to action and nowhere else. Selection, focus and '
          'severity are all steps on one grey ramp.',
    'gold' =>
      'The website’s gold, with severity shown by lightness rather than '
          'colour. Quieter, and legible if you cannot separate red from green.',
    _ =>
      'A vivid orange accent with the usual red, amber and green severities.',
  };

  static CrawlStatPalette byName(String? name) =>
      all.firstWhere((palette) => name == name, orElse: () => ember);

  @override
  CrawlStatPalette copyWith({
    String? name,
    Color? accent,
    Color? highlight,
    Color? error,
    Color? warning,
    Color? notice,
    Color? success,
  }) {
    return CrawlStatPalette(
      name: name ?? this.name,
      accent: accent ?? this.accent,
      highlight: highlight ?? this.highlight,
      error: error ?? this.error,
      warning: warning ?? this.warning,
      notice: notice ?? this.notice,
      success: success ?? this.success,
    );
  }

  @override
  CrawlStatPalette lerp(ThemeExtension<CrawlStatPalette>? other, double t) {
    if (other is! CrawlStatPalette) return this;
    return CrawlStatPalette(
      name: t < 0.5 ? name : other.name,
      accent: Color.lerp(accent, other.accent, t)!,
      highlight: Color.lerp(highlight, other.highlight, t)!,
      error: Color.lerp(error, other.error, t)!,
      warning: Color.lerp(warning, other.warning, t)!,
      notice: Color.lerp(notice, other.notice, t)!,
      success: Color.lerp(success, other.success, t)!,
    );
  }
}
