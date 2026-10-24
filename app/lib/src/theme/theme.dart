import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

/// CrawlStat's design system.
///
/// The app is a tool, not a page: dense, quiet, and built so a table of a
/// hundred thousand rows is the thing you notice, not the chrome around it.
/// Three rules follow from that and are worth stating, because they are what
/// makes the app feel different from a web dashboard:
///
/// * **Borders, not shadows.** Elevation implies depth and movement. A data
///   grid wants flat, crisp separation that stays readable at 13px.
/// * **Numbers are monospaced.** Status codes, byte counts and URLs line up
///   column to column, so the eye can scan down instead of reading across.
/// * **One accent.** Colour carries meaning here — severity, status class — so
///   spending it on decoration would make the meaningful uses invisible.
class CrawlStatTheme {
  const CrawlStatTheme._();

  /// Row height in data grids. Tight enough to show ~30 rows on a laptop.
  static const double rowHeight = 30;

  /// Header height in data grids.
  static const double headerHeight = 34;

  /// The single accent, used for selection, focus and primary actions.
  static const Color accent = Color(0xFF3B6EF6);

  /// Severity colours. These are the only other colours with meaning.
  static const Color error = Color(0xFFD5342F);
  static const Color warning = Color(0xFFC9790B);
  static const Color notice = Color(0xFF5A6673);
  static const Color success = Color(0xFF1C8A4B);

  static ThemeData light() => _build(
        brightness: Brightness.light,
        surface: const Color(0xFFFFFFFF),
        canvas: const Color(0xFFF7F8FA),
        panel: const Color(0xFFFCFCFD),
        border: const Color(0xFFE1E4E9),
        text: const Color(0xFF14181D),
        subdued: const Color(0xFF5E6672),
        hover: const Color(0xFFF0F3F8),
      );

  static ThemeData dark() => _build(
        brightness: Brightness.dark,
        surface: const Color(0xFF15181D),
        canvas: const Color(0xFF0E1115),
        panel: const Color(0xFF1A1E24),
        border: const Color(0xFF2A3039),
        text: const Color(0xFFE6E9ED),
        subdued: const Color(0xFF99A2AE),
        hover: const Color(0xFF212731),
      );

  static ThemeData _build({
    required Brightness brightness,
    required Color surface,
    required Color canvas,
    required Color panel,
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
      outline: border,
      outlineVariant: border,
      onSurfaceVariant: subdued,
    );

    // 13px base: the density a data tool needs without becoming unreadable.
    const baseSize = 13.0;
    final body = TextStyle(fontSize: baseSize, height: 1.35, color: text);

    return ThemeData(
      useMaterial3: true,
      brightness: brightness,
      colorScheme: colors,
      scaffoldBackgroundColor: canvas,
      canvasColor: canvas,
      dividerColor: border,
      splashFactory: NoSplash.splashFactory,
      // A pointer-driven app does not need ink ripples; they read as lag.
      highlightColor: hover,
      hoverColor: hover,
      visualDensity: VisualDensity.compact,
      dividerTheme: DividerThemeData(color: border, thickness: 1, space: 1),
      textTheme: TextTheme(
        bodyMedium: body,
        bodySmall: body.copyWith(fontSize: 12, color: subdued),
        titleMedium: body.copyWith(fontSize: 15, fontWeight: FontWeight.w600),
        titleLarge: body.copyWith(fontSize: 19, fontWeight: FontWeight.w600, height: 1.2),
        labelLarge: body.copyWith(fontWeight: FontWeight.w500),
        labelMedium: body.copyWith(
          fontSize: 11,
          fontWeight: FontWeight.w600,
          color: subdued,
          letterSpacing: 0.4,
        ),
      ),
      iconTheme: IconThemeData(size: 16, color: subdued),
      inputDecorationTheme: InputDecorationTheme(
        isDense: true,
        filled: true,
        fillColor: surface,
        hintStyle: body.copyWith(color: subdued),
        contentPadding: const EdgeInsets.symmetric(horizontal: 10, vertical: 9),
        border: _inputBorder(border),
        enabledBorder: _inputBorder(border),
        focusedBorder: _inputBorder(accent, width: 1.5),
        errorBorder: _inputBorder(error),
      ),
      filledButtonTheme: FilledButtonThemeData(
        style: FilledButton.styleFrom(
          textStyle: body.copyWith(fontWeight: FontWeight.w600),
          shape: const RoundedRectangleBorder(
            borderRadius: BorderRadius.all(Radius.circular(6)),
          ),
          padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 14),
          minimumSize: const Size(0, 34),
        ),
      ),
      outlinedButtonTheme: OutlinedButtonThemeData(
        style: OutlinedButton.styleFrom(
          foregroundColor: text,
          side: BorderSide(color: border),
          textStyle: body,
          shape: const RoundedRectangleBorder(
            borderRadius: BorderRadius.all(Radius.circular(6)),
          ),
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 12),
          minimumSize: const Size(0, 34),
        ),
      ),
      textButtonTheme: TextButtonThemeData(
        style: TextButton.styleFrom(
          foregroundColor: accent,
          textStyle: body.copyWith(fontWeight: FontWeight.w500),
          minimumSize: const Size(0, 30),
          padding: const EdgeInsets.symmetric(horizontal: 10),
        ),
      ),
      tooltipTheme: TooltipThemeData(
        waitDuration: const Duration(milliseconds: 500),
        textStyle: body.copyWith(color: Colors.white, fontSize: 12),
        decoration: BoxDecoration(
          color: const Color(0xFF14181D).withValues(alpha: 0.94),
          borderRadius: BorderRadius.circular(4),
        ),
      ),
      scrollbarTheme: ScrollbarThemeData(
        thickness: const WidgetStatePropertyAll(8),
        radius: const Radius.circular(4),
        thumbColor: WidgetStatePropertyAll(subdued.withValues(alpha: 0.35)),
      ),
      cardTheme: CardThemeData(
        color: panel,
        elevation: 0,
        margin: EdgeInsets.zero,
        shape: RoundedRectangleBorder(
          side: BorderSide(color: border),
          borderRadius: BorderRadius.circular(8),
        ),
      ),
      dialogTheme: DialogThemeData(
        backgroundColor: surface,
        elevation: 0,
        shape: RoundedRectangleBorder(
          side: BorderSide(color: border),
          borderRadius: BorderRadius.circular(10),
        ),
      ),
      progressIndicatorTheme: const ProgressIndicatorThemeData(linearMinHeight: 4),
    );
  }

  static OutlineInputBorder _inputBorder(Color color, {double width = 1}) {
    return OutlineInputBorder(
      borderRadius: BorderRadius.circular(6),
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
  static const openProject = SingleActivator(LogicalKeyboardKey.keyO, meta: true);
  static const focusFilter = SingleActivator(LogicalKeyboardKey.keyF, meta: true);
  static const exportCsv = SingleActivator(LogicalKeyboardKey.keyE, meta: true);
  static const pauseResume = SingleActivator(LogicalKeyboardKey.space, meta: true);
  static const closePanel = SingleActivator(LogicalKeyboardKey.escape);
}
