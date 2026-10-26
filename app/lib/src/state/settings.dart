import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../rust/api.dart' as rust;
import '../rust/api/types.dart';
import 'projects.dart';

/// The user's crawl defaults and app preferences.
///
/// Stored as JSON beside the crawl databases. Deliberately *not* in the OS
/// keychain — none of this is secret, and a plain file is one less native
/// dependency. Licence keys and API tokens do go in the keychain, in Phase 4.
/// Ollama's own default, repeated here so the first frame has a value without
/// a call across the bridge.
const String defaultOllamaEndpoint = 'http://127.0.0.1:11434';
const String defaultOllamaModel = 'qwen3:8b';

class Settings {
  const Settings({
    required this.userAgent,
    this.themeMode = ThemeMode.system,
    this.concurrency = 5,
    this.crawlDelayMs = 0,
    this.requestTimeoutMs = 20000,
    this.respectRobots = true,
    this.crawlSubdomains = false,
    this.checkExternalLinks = true,
    this.followNofollow = false,
    this.crawlImages = true,
    this.crawlCss = false,
    this.crawlJavascript = false,
    this.crawlDocuments = true,
    this.storeHtml = false,
    this.runAudits = true,
    this.projectsDirectory,
    this.exportDirectory,
    this.sidebarCollapsed,
    this.ollamaEndpoint = defaultOllamaEndpoint,
    this.ollamaModel = defaultOllamaModel,
    this.paletteName = 'ember',
    this.defaultPersona = 'task_oriented',
    this.deepThreshold = 4,
    this.crowdedThreshold = 120,
    this.projectsView = 'grid',
    this.reportPreparedBy = '',
    this.reportAccent = '',
  });

  final String userAgent;
  final ThemeMode themeMode;
  final int concurrency;
  final int crawlDelayMs;
  final int requestTimeoutMs;
  final bool respectRobots;
  final bool crawlSubdomains;
  final bool checkExternalLinks;
  final bool followNofollow;
  final bool crawlImages;
  final bool crawlCss;
  final bool crawlJavascript;
  final bool crawlDocuments;
  final bool storeHtml;
  final bool runAudits;

  /// `null` means the platform default.
  final String? projectsDirectory;
  final String? exportDirectory;

  /// The navigation shown as icons only. `null` until the user chooses, so
  /// the window's width can decide in the meantime.
  final bool? sidebarCollapsed;

  /// Where the local language model listens.
  ///
  /// Loopback by default, which is the whole point: the summary feature only
  /// keeps its privacy promise while this address stays on this machine. The
  /// field is editable because some people run Ollama on a box under their
  /// desk — and the Settings screen says plainly what that means.
  final String ollamaEndpoint;
  final String ollamaModel;

  /// Which set of meaningful colours to use: `ember`, `gold`, `graphite` or
  /// `mono`. Only severity and the accent change; the surfaces do not.
  final String paletteName;

  /// Which visitor the Journeys screen starts on.
  final String defaultPersona;

  /// Clicks from home past which a page counts as buried. Three is the
  /// number most IA guidance lands on; a documentation set may want more.
  final int deepThreshold;

  /// Links on one page past which choosing becomes the task.
  final int crowdedThreshold;

  /// How the Projects screen lays out crawls: `grid` (one card per crawl) or
  /// `list` (one row per site, with its history).
  final String projectsView;

  /// "Prepared by" on exported reports, and their brand colour as `#rrggbb`
  /// (blank for CrawlStat's own). Remembered because an agency sends the
  /// same name and colour on every report.
  final String reportPreparedBy;
  final String reportAccent;

  String get resolvedProjectsDirectory =>
      projectsDirectory ?? defaultProjectsDirectory();
  String get resolvedExportDirectory =>
      exportDirectory ?? defaultExportDirectory();

  Settings copyWith({
    String? userAgent,
    ThemeMode? themeMode,
    int? concurrency,
    int? crawlDelayMs,
    int? requestTimeoutMs,
    bool? respectRobots,
    bool? crawlSubdomains,
    bool? checkExternalLinks,
    bool? followNofollow,
    bool? crawlImages,
    bool? crawlCss,
    bool? crawlJavascript,
    bool? crawlDocuments,
    bool? storeHtml,
    bool? runAudits,
    String? projectsDirectory,
    String? exportDirectory,
    bool? sidebarCollapsed,
    String? ollamaEndpoint,
    String? ollamaModel,
    String? paletteName,
    String? defaultPersona,
    int? deepThreshold,
    int? crowdedThreshold,
    String? projectsView,
    String? reportPreparedBy,
    String? reportAccent,
  }) {
    return Settings(
      userAgent: userAgent ?? this.userAgent,
      themeMode: themeMode ?? this.themeMode,
      concurrency: concurrency ?? this.concurrency,
      crawlDelayMs: crawlDelayMs ?? this.crawlDelayMs,
      requestTimeoutMs: requestTimeoutMs ?? this.requestTimeoutMs,
      respectRobots: respectRobots ?? this.respectRobots,
      crawlSubdomains: crawlSubdomains ?? this.crawlSubdomains,
      checkExternalLinks: checkExternalLinks ?? this.checkExternalLinks,
      followNofollow: followNofollow ?? this.followNofollow,
      crawlImages: crawlImages ?? this.crawlImages,
      crawlCss: crawlCss ?? this.crawlCss,
      crawlJavascript: crawlJavascript ?? this.crawlJavascript,
      crawlDocuments: crawlDocuments ?? this.crawlDocuments,
      storeHtml: storeHtml ?? this.storeHtml,
      runAudits: runAudits ?? this.runAudits,
      projectsDirectory: projectsDirectory ?? this.projectsDirectory,
      exportDirectory: exportDirectory ?? this.exportDirectory,
      sidebarCollapsed: sidebarCollapsed ?? this.sidebarCollapsed,
      ollamaEndpoint: ollamaEndpoint ?? this.ollamaEndpoint,
      ollamaModel: ollamaModel ?? this.ollamaModel,
      paletteName: paletteName ?? this.paletteName,
      defaultPersona: defaultPersona ?? this.defaultPersona,
      deepThreshold: deepThreshold ?? this.deepThreshold,
      crowdedThreshold: crowdedThreshold ?? this.crowdedThreshold,
      projectsView: projectsView ?? this.projectsView,
      reportPreparedBy: reportPreparedBy ?? this.reportPreparedBy,
      reportAccent: reportAccent ?? this.reportAccent,
    );
  }

  Map<String, Object?> toJson() => {
    'userAgent': userAgent,
    'ollamaEndpoint': ollamaEndpoint,
    'ollamaModel': ollamaModel,
    'paletteName': paletteName,
    'defaultPersona': defaultPersona,
    'deepThreshold': deepThreshold,
    'crowdedThreshold': crowdedThreshold,
    'projectsView': projectsView,
    'reportPreparedBy': reportPreparedBy,
    'reportAccent': reportAccent,
    'themeMode': themeMode.name,
    'concurrency': concurrency,
    'crawlDelayMs': crawlDelayMs,
    'requestTimeoutMs': requestTimeoutMs,
    'respectRobots': respectRobots,
    'crawlSubdomains': crawlSubdomains,
    'checkExternalLinks': checkExternalLinks,
    'followNofollow': followNofollow,
    'crawlImages': crawlImages,
    'crawlCss': crawlCss,
    'crawlJavascript': crawlJavascript,
    'crawlDocuments': crawlDocuments,
    'storeHtml': storeHtml,
    'runAudits': runAudits,
    'projectsDirectory': projectsDirectory,
    'exportDirectory': exportDirectory,
    'sidebarCollapsed': sidebarCollapsed,
  };

  /// Read settings, falling back to the engine's defaults for anything absent.
  ///
  /// Unknown and missing keys are tolerated on purpose: a settings file written
  /// by an older build must still open, and one written by a newer build must
  /// not crash this one.
  static Settings fromJson(
    Map<String, Object?> json, {
    required String fallbackUserAgent,
  }) {
    T? pick<T>(String key) => json[key] is T ? json[key] as T : null;
    final defaults = Settings(userAgent: fallbackUserAgent);

    return Settings(
      userAgent: pick<String>('userAgent') ?? fallbackUserAgent,
      ollamaEndpoint: pick<String>('ollamaEndpoint') ?? defaults.ollamaEndpoint,
      ollamaModel: pick<String>('ollamaModel') ?? defaults.ollamaModel,
      paletteName: pick<String>('paletteName') ?? defaults.paletteName,
      defaultPersona: pick<String>('defaultPersona') ?? defaults.defaultPersona,
      deepThreshold: pick<int>('deepThreshold') ?? defaults.deepThreshold,
      crowdedThreshold:
          pick<int>('crowdedThreshold') ?? defaults.crowdedThreshold,
      projectsView: pick<String>('projectsView') ?? defaults.projectsView,
      reportPreparedBy:
          pick<String>('reportPreparedBy') ?? defaults.reportPreparedBy,
      reportAccent: pick<String>('reportAccent') ?? defaults.reportAccent,
      themeMode: ThemeMode.values.firstWhere(
        (mode) => mode.name == pick<String>('themeMode'),
        orElse: () => ThemeMode.system,
      ),
      concurrency: pick<int>('concurrency') ?? defaults.concurrency,
      crawlDelayMs: pick<int>('crawlDelayMs') ?? defaults.crawlDelayMs,
      requestTimeoutMs:
          pick<int>('requestTimeoutMs') ?? defaults.requestTimeoutMs,
      respectRobots: pick<bool>('respectRobots') ?? defaults.respectRobots,
      crawlSubdomains:
          pick<bool>('crawlSubdomains') ?? defaults.crawlSubdomains,
      checkExternalLinks:
          pick<bool>('checkExternalLinks') ?? defaults.checkExternalLinks,
      followNofollow: pick<bool>('followNofollow') ?? defaults.followNofollow,
      crawlImages: pick<bool>('crawlImages') ?? defaults.crawlImages,
      crawlCss: pick<bool>('crawlCss') ?? defaults.crawlCss,
      crawlJavascript:
          pick<bool>('crawlJavascript') ?? defaults.crawlJavascript,
      crawlDocuments: pick<bool>('crawlDocuments') ?? defaults.crawlDocuments,
      storeHtml: pick<bool>('storeHtml') ?? defaults.storeHtml,
      runAudits: pick<bool>('runAudits') ?? defaults.runAudits,
      projectsDirectory: pick<String>('projectsDirectory'),
      exportDirectory: pick<String>('exportDirectory'),
      sidebarCollapsed: pick<bool>('sidebarCollapsed'),
    );
  }

  /// A crawl request carrying these defaults.
  CrawlRequest toRequest({
    required String startUrl,
    String mode = 'spider',
    List<String> listUrls = const [],
    int? maxUrls,
    int? maxDepth,
    List<String> includePatterns = const [],
    List<String> excludePatterns = const [],
    bool renderJavascript = false,
  }) {
    return CrawlRequest(
      startUrl: startUrl,
      mode: mode,
      listUrls: listUrls,
      maxUrls: maxUrls,
      maxDepth: maxDepth,
      concurrency: concurrency,
      crawlDelayMs: crawlDelayMs,
      requestTimeoutMs: requestTimeoutMs,
      respectRobots: respectRobots,
      userAgent: userAgent,
      crawlSubdomains: crawlSubdomains,
      checkExternalLinks: checkExternalLinks,
      followNofollow: followNofollow,
      includePatterns: includePatterns,
      excludePatterns: excludePatterns,
      crawlImages: crawlImages,
      crawlCss: crawlCss,
      crawlJavascript: crawlJavascript,
      crawlDocuments: crawlDocuments,
      storeHtml: storeHtml,
      renderJavascript: renderJavascript,
      runAudits: runAudits,
    );
  }
}

class SettingsController extends Notifier<Settings> {
  static const _fileName = 'settings.json';

  @override
  Settings build() {
    final fallback = rust.defaultUserAgent();
    try {
      if (_file.existsSync()) {
        final decoded = jsonDecode(_file.readAsStringSync());
        if (decoded is Map<String, Object?>) {
          return Settings.fromJson(decoded, fallbackUserAgent: fallback);
        }
      }
    } on Object {
      // A corrupt settings file should not stop the app opening.
    }
    return Settings(userAgent: fallback);
  }

  File get _file {
    final directory = Directory(appSupportDirectory());
    if (!directory.existsSync()) {
      directory.createSync(recursive: true);
    }
    return File('${directory.path}${Platform.pathSeparator}$_fileName');
  }

  void update(Settings Function(Settings) change) {
    state = change(state);
    try {
      _file.writeAsStringSync(
        const JsonEncoder.withIndent('  ').convert(state.toJson()),
      );
    } on Object {
      // Settings that fail to persist still apply for this session.
    }
  }

  void resetUserAgent() =>
      update((s) => s.copyWith(userAgent: rust.defaultUserAgent()));

  void setThemeMode(ThemeMode mode) =>
      update((s) => s.copyWith(themeMode: mode));

  void setSidebarCollapsed(bool collapsed) =>
      update((s) => s.copyWith(sidebarCollapsed: collapsed));
}

final settingsProvider = NotifierProvider<SettingsController, Settings>(
  SettingsController.new,
);

/// URLs per crawl allowed without a licence. Phase 4 raises this on activation.
final freeTierLimitProvider = Provider<int>((ref) => rust.freeTierMaxUrls());

/// The engine version, for the Settings screen's About section.
final engineVersionProvider = Provider<String>((ref) => rust.engineVersion());
