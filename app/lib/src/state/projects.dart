import 'dart:convert';
import 'dart:io';

import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../rust/api/project.dart' as rust;
import '../rust/api/types.dart';

/// Where CrawlStat keeps its own files.
///
/// Resolved from the OS conventions directly rather than through a plugin:
/// desktop Dart can do this with `dart:io` alone, which keeps the app free of
/// native plugin dependencies and therefore buildable with just the Flutter
/// SDK.
String appSupportDirectory() {
  final home = Platform.environment['HOME'] ?? Platform.environment['USERPROFILE'] ?? '.';
  if (Platform.isMacOS) {
    return '$home/Library/Application Support/CrawlStat';
  }
  if (Platform.isWindows) {
    final appData = Platform.environment['APPDATA'] ?? '$home\\AppData\\Roaming';
    return '$appData\\CrawlStat';
  }
  // Linux, per the XDG base directory spec.
  final dataHome = Platform.environment['XDG_DATA_HOME'] ?? '$home/.local/share';
  return '$dataHome/crawlstat';
}

/// Where new crawls are saved by default.
String defaultProjectsDirectory() => '${appSupportDirectory()}${Platform.pathSeparator}projects';

/// Where exports go by default.
String defaultExportDirectory() {
  final home = Platform.environment['HOME'] ?? Platform.environment['USERPROFILE'] ?? '.';
  final downloads = Directory('$home${Platform.pathSeparator}Downloads');
  return downloads.existsSync() ? downloads.path : home;
}

/// The list of crawls the user has opened, newest first.
///
/// Stored as a small JSON file next to the crawl databases. Paths are verified
/// on load: a crawl the user has deleted or moved should drop off the list
/// rather than sit there failing to open.
class RecentProjects extends Notifier<List<ProjectInfo>> {
  static const _fileName = 'recent.json';

  @override
  List<ProjectInfo> build() {
    // The first frame must not wait on disk, so the list starts empty and
    // `refresh()` fills it in.
    Future.microtask(refresh);
    return const [];
  }

  File get _file {
    final directory = Directory(appSupportDirectory());
    if (!directory.existsSync()) {
      directory.createSync(recursive: true);
    }
    return File('${directory.path}${Platform.pathSeparator}$_fileName');
  }

  List<String> _readPaths() {
    try {
      if (!_file.existsSync()) return const [];
      final decoded = jsonDecode(_file.readAsStringSync());
      if (decoded is! List) return const [];
      return decoded.whereType<String>().toList();
    } on Object {
      // A corrupt list is not worth an error dialog: start fresh.
      return const [];
    }
  }

  void _writePaths(List<String> paths) {
    try {
      _file.writeAsStringSync(jsonEncode(paths));
    } on Object {
      // Losing the recents list is a cosmetic failure, not a crawl failure.
    }
  }

  /// Re-read every known crawl file and drop the ones that have gone.
  Future<void> refresh() async {
    final paths = _readPaths();
    final found = <ProjectInfo>[];
    final survivors = <String>[];

    for (final path in paths) {
      if (!File(path).existsSync()) continue;
      try {
        found.add(await rust.projectInfo(dbPath: path));
        survivors.add(path);
      } on Object {
        // Unreadable, or written by a newer build. Keep it listed so the user
        // can see it, but with nothing to show.
        survivors.add(path);
      }
    }

    if (survivors.length != paths.length) {
      _writePaths(survivors);
    }
    state = found;
  }

  /// Record a crawl as the most recently used.
  Future<void> remember(String path) async {
    final paths = _readPaths()..removeWhere((existing) => existing == path);
    paths.insert(0, path);
    // Twenty is enough to be useful and short enough to stay scannable.
    _writePaths(paths.take(20).toList());
    await refresh();
  }

  /// Remove a crawl from the list. The file itself is left alone unless
  /// `deleteFile` is set.
  Future<void> forget(String path, {bool deleteFile = false}) async {
    _writePaths(_readPaths()..removeWhere((existing) => existing == path));
    if (deleteFile) {
      for (final suffix in ['', '-wal', '-shm']) {
        final file = File('$path$suffix');
        if (file.existsSync()) {
          try {
            file.deleteSync();
          } on Object {
            // Already gone, or locked by another process.
          }
        }
      }
    }
    await refresh();
  }
}

final recentProjectsProvider =
    NotifierProvider<RecentProjects, List<ProjectInfo>>(RecentProjects.new);

/// The crawl file the UI is currently showing.
///
/// A plain `Notifier` rather than a `StateProvider`, which Riverpod 3 moved to
/// its legacy import.
class OpenProject extends Notifier<String?> {
  @override
  String? build() => null;

  void open(String path) => state = path;

  void close() {
    rust.closeProject();
    state = null;
  }
}

final openProjectProvider = NotifierProvider<OpenProject, String?>(OpenProject.new);

/// Summary of the open project, refreshed on demand.
final projectInfoProvider = FutureProvider.family<ProjectInfo, String>((ref, path) {
  return rust.projectInfo(dbPath: path);
});

/// Build a filename for a new crawl from its start URL.
///
/// `https://example.com/blog` becomes `example-com-20260224-1432.db`, so a
/// directory of crawls sorts by site and then by date.
String suggestedProjectFileName(String startUrl, {DateTime? now}) {
  final at = now ?? DateTime.now();
  var host = startUrl.trim().toLowerCase();
  host = host.replaceFirst(RegExp(r'^[a-z]+://'), '');
  host = host.split('/').first.split(':').first;
  host = host.replaceFirst(RegExp(r'^www\.'), '');
  host = host.replaceAll(RegExp(r'[^a-z0-9.-]'), '');
  host = host.replaceAll('.', '-');
  if (host.isEmpty) host = 'crawl';

  String two(int value) => value.toString().padLeft(2, '0');
  final stamp = '${at.year}${two(at.month)}${two(at.day)}-${two(at.hour)}${two(at.minute)}';
  return '$host-$stamp.db';
}
