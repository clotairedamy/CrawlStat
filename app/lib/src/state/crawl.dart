import 'dart:async';
import 'dart:collection';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../rust/api/crawl.dart' as rust;
import '../rust/api/types.dart';
import 'projects.dart';

/// Rows kept for the live table during a crawl.
///
/// The crawl writes every URL to SQLite as it goes, so this is a *view* of the
/// last few hundred, not a buffer that has to hold everything. A 500,000-URL
/// crawl uses the same memory here as a small one, and the full results are one
/// query away the moment the crawl ends.
const int liveRowLimit = 300;

/// Warnings kept in memory. Every one is also in the crawl log.
const int liveWarningLimit = 100;

/// What the live crawl screen shows.
class LiveCrawl {
  const LiveCrawl({
    this.phase = CrawlPhase.idle,
    this.dbPath,
    this.startUrl,
    this.snapshot,
    this.rows = const [],
    this.warnings = const [],
    this.summary,
    this.audit,
    this.error,
    this.seeded = 0,
    this.resumed = false,
  });

  final CrawlPhase phase;
  final String? dbPath;
  final String? startUrl;

  /// The latest counters. `null` until the first snapshot arrives.
  final ProgressSnapshot? snapshot;

  /// Most recent URLs, newest first.
  final List<CrawledUrlEvent> rows;
  final List<CrawlWarning> warnings;

  final CrawlSummary? summary;
  final CrawlEvent_Audited? audit;

  /// Set when the crawl could not start or died.
  final String? error;

  final int seeded;
  final bool resumed;

  bool get isRunning => phase == CrawlPhase.running || phase == CrawlPhase.paused;
  bool get isFinished =>
      phase == CrawlPhase.finished || phase == CrawlPhase.cancelled || phase == CrawlPhase.failed;

  /// Crawled over discovered, or `null` when there is no limit to measure
  /// against — a spider crawl has no total until it ends.
  double? get fraction {
    final current = snapshot;
    if (current == null || current.discovered == 0) return null;
    final done = current.crawled + current.errors;
    return (done / current.discovered).clamp(0.0, 1.0);
  }

  LiveCrawl copyWith({
    CrawlPhase? phase,
    String? dbPath,
    String? startUrl,
    ProgressSnapshot? snapshot,
    List<CrawledUrlEvent>? rows,
    List<CrawlWarning>? warnings,
    CrawlSummary? summary,
    CrawlEvent_Audited? audit,
    String? error,
    int? seeded,
    bool? resumed,
  }) {
    return LiveCrawl(
      phase: phase ?? this.phase,
      dbPath: dbPath ?? this.dbPath,
      startUrl: startUrl ?? this.startUrl,
      snapshot: snapshot ?? this.snapshot,
      rows: rows ?? this.rows,
      warnings: warnings ?? this.warnings,
      summary: summary ?? this.summary,
      audit: audit ?? this.audit,
      error: error ?? this.error,
      seeded: seeded ?? this.seeded,
      resumed: resumed ?? this.resumed,
    );
  }
}

enum CrawlPhase { idle, starting, running, paused, auditing, finished, cancelled, failed }

/// A warning with the time it arrived, so the panel can order them.
class CrawlWarning {
  const CrawlWarning(this.message, this.url, this.at);

  final String message;
  final String? url;
  final DateTime at;
}

/// Drives one crawl and exposes its progress.
///
/// Riverpod holds this for the app's lifetime, so navigating away from the
/// crawl screen does not cancel the crawl — which is the behaviour a user
/// expects from a tool that takes minutes to finish.
class CrawlController extends Notifier<LiveCrawl> {
  StreamSubscription<CrawlEvent>? _subscription;

  @override
  LiveCrawl build() {
    ref.onDispose(() => _subscription?.cancel());
    return const LiveCrawl();
  }

  /// Start a crawl and begin streaming its progress.
  Future<void> start(CrawlRequest request, String dbPath) async {
    await _subscription?.cancel();
    state = LiveCrawl(
      phase: CrawlPhase.starting,
      dbPath: dbPath,
      startUrl: request.mode == 'list' ? 'a list of URLs' : request.startUrl,
    );
    _listen(rust.startCrawl(request: request, dbPath: dbPath));
    await ref.read(recentProjectsProvider.notifier).remember(dbPath);
  }

  /// Continue an interrupted crawl.
  Future<void> resumeProject(String dbPath) async {
    await _subscription?.cancel();
    state = LiveCrawl(phase: CrawlPhase.starting, dbPath: dbPath, resumed: true);
    _listen(rust.resumeCrawl(dbPath: dbPath));
    await ref.read(recentProjectsProvider.notifier).remember(dbPath);
  }

  void _listen(Stream<CrawlEvent> events) {
    _subscription = events.listen(
      _apply,
      onError: (Object error) {
        state = state.copyWith(phase: CrawlPhase.failed, error: error.toString());
      },
      onDone: () {
        // The engine always sends `Finished` before closing, so reaching here
        // in a running state means the stream died unexpectedly.
        if (!state.isFinished) {
          state = state.copyWith(phase: CrawlPhase.finished);
        }
        ref.invalidate(recentProjectsProvider);
      },
    );
  }

  void _apply(CrawlEvent event) {
    switch (event) {
      case CrawlEvent_Started(:final seeded, :final resumed):
        state = state.copyWith(
          phase: CrawlPhase.running,
          seeded: seeded.toInt(),
          resumed: resumed,
        );
      case CrawlEvent_Url(:final field0):
        // A bounded queue: the newest rows at the front, the oldest dropped.
        final rows = Queue<CrawledUrlEvent>.of(state.rows);
        rows.addFirst(field0);
        while (rows.length > liveRowLimit) {
          rows.removeLast();
        }
        state = state.copyWith(rows: rows.toList(growable: false));
      case CrawlEvent_Progress(:final field0):
        state = state.copyWith(snapshot: field0);
      case CrawlEvent_Warning(:final message, :final url):
        final warnings = [CrawlWarning(message, url, DateTime.now()), ...state.warnings];
        state = state.copyWith(
          warnings: warnings.take(liveWarningLimit).toList(growable: false),
        );
      case CrawlEvent_StateChanged(state: final name):
        state = state.copyWith(phase: _phaseFor(name) ?? state.phase);
      case CrawlEvent_AuditStarted():
        state = state.copyWith(phase: CrawlPhase.auditing);
      case final CrawlEvent_Audited audited:
        state = state.copyWith(audit: audited);
      case CrawlEvent_Finished(:final summary):
        state = state.copyWith(
          phase: _phaseFor(summary.state) ?? CrawlPhase.finished,
          summary: summary,
        );
      case CrawlEvent_Failed(:final message):
        state = state.copyWith(phase: CrawlPhase.failed, error: message);
    }
  }

  /// Map the engine's state names onto the UI's phases.
  CrawlPhase? _phaseFor(String name) {
    switch (name) {
      case 'starting':
        return CrawlPhase.starting;
      case 'running':
        return CrawlPhase.running;
      case 'paused':
        return CrawlPhase.paused;
      case 'finished':
        return CrawlPhase.finished;
      case 'cancelled':
        return CrawlPhase.cancelled;
      case 'failed':
        return CrawlPhase.failed;
      // 'stopping' keeps the current phase: in-flight requests are still
      // finishing, and flipping the UI to "cancelled" early would be a lie.
      default:
        return null;
    }
  }

  void pause() {
    if (rust.pauseCrawl()) {
      state = state.copyWith(phase: CrawlPhase.paused);
    }
  }

  void resume() {
    if (rust.resumePausedCrawl()) {
      state = state.copyWith(phase: CrawlPhase.running);
    }
  }

  void cancel() => rust.cancelCrawl();

  /// Forget a finished crawl, so the screen is ready for the next one.
  void clear() {
    _subscription?.cancel();
    _subscription = null;
    state = const LiveCrawl();
  }
}

final crawlControllerProvider =
    NotifierProvider<CrawlController, LiveCrawl>(CrawlController.new);
