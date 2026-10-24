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

  /// The single accent, used for selection, focus and primary actions.
  static const Color accent = Color(0xFFFF6A14);

  /// Severity colours. These are the only other colours with meaning, and
  /// they are tuned to read on both the light and the dark canvas.
  static const Color error = Color(0xFFF2543D);
  static const Color warning = Color(0xFFE8A33D);
  static const Color notice = Color(0xFF8A8A94);
  static const Color success = Color(0xFF4ABD7F);

  /// The system UI font on each platform, named explicitly so every text style
  /// — buttons included — uses the same family.
  static String? get fontFamily => switch (defaultTargetPlatform) {
    TargetPlatform.macOS || TargetPlatform.iOS => '.AppleSystemUIFont',
    TargetPlatform.windows => 'Segoe UI',
    _ => null,
  };

  static ThemeData light() => _build(
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

  static ThemeData dark() => _build(
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
      primary: accent,
      onPrimary: Colors.white,
      secondary: accent,
      onSecondary: Colors.white,
      error: error,
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
      fontSize: baseSize,
      height: 1.35,
      color: text,
    );
    const controlShape = RoundedRectangleBorder(
      borderRadius: BorderRadius.all(Radius.circular(controlRadius)),
    );

    return ThemeData(
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
        focusedBorder: _inputBorder(accent, width: 1.5),
        errorBorder: _inputBorder(error),
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
          // The halo is cast in the accent, not in black, so the pill looks
          // lit rather than merely elevated.
          elevation: 10,
          shadowColor: accent.withValues(alpha: 0.55),
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
          foregroundColor: accent,
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
          selectedBackgroundColor: accent,
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
          (states) => states.contains(WidgetState.selected) ? accent : raised,
        ),
        trackOutlineColor: WidgetStateProperty.resolveWith(
          (states) => states.contains(WidgetState.selected) ? accent : border,
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
        actionTextColor: accent,
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
        color: accent,
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
      // A stack rather than a bundled font: these ship with the OS, so the app
      // stays small and looks native on each platform.
      fontFamily: 'SF Mono',
      fontFamilyFallback: const [
        'Menlo',
        'Consolas',
        'DejaVu Sans Mono',
        'Courier New',
        'monospace',
      ],
      fontSize: size ?? 12,
      height: 1.3,
      color: color ?? Theme.of(context).colorScheme.onSurface,
    );
  }

  /// Colour for an HTTP status code: 2xx green, 3xx accent, 4xx/5xx red.
  static Color statusColor(int? status) {
    if (status == null) return notice;
    if (status >= 500) return error;
    if (status >= 400) return error;
    if (status >= 300) return warning;
    if (status >= 200) return success;
    return notice;
  }

  static Color severityColor(String severity) {
    switch (severity) {
      case 'error':
        return error;
      case 'warning':
        return warning;
      default:
        return notice;
    }
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
