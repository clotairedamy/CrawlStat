import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:intl/intl.dart';

import '../state/settings.dart';

const _files = MethodChannel('crawlstat/files');

final _stamp = DateFormat('yyyy-MM-dd-HHmmss');

/// Write one CSV export into the export folder, then say where it went.
///
/// Every export in the app goes through here, so they all name their files
/// the same way, fail the same way, and offer the same way to find the file
/// afterwards. `write` is handed the full path and returns the row count.
Future<void> exportCsv(
  BuildContext context,
  WidgetRef ref, {
  required String name,
  required Future<int> Function(String file) write,
  required String Function(int rows) done,
}) => _export(
  context,
  ref,
  name: name,
  extension: 'csv',
  write: write,
  done: done,
);

/// Write a finished document — the model's report — into the export folder.
Future<void> exportText(
  BuildContext context,
  WidgetRef ref, {
  required String name,
  required String extension,
  required String contents,
  required String done,
}) => _export(
  context,
  ref,
  name: name,
  extension: extension,
  write: (file) async {
    await File(file).writeAsString(contents);
    return 1;
  },
  done: (_) => done,
);

Future<void> _export(
  BuildContext context,
  WidgetRef ref, {
  required String name,
  required String extension,
  required Future<int> Function(String file) write,
  required String Function(int rows) done,
}) async {
  final messenger = ScaffoldMessenger.of(context);
  final directory = Directory(
    ref.read(settingsProvider).resolvedExportDirectory,
  );
  final file =
      '${directory.path}${Platform.pathSeparator}'
      'crawlstat-$name-${_stamp.format(DateTime.now())}.$extension';

  try {
    if (!directory.existsSync()) directory.createSync(recursive: true);
    final rows = await write(file);
    messenger.showSnackBar(
      SnackBar(
        content: Text(done(rows)),
        action: SnackBarAction(
          label: Platform.isMacOS ? 'Show in Finder' : 'Show file',
          onPressed: () => revealFile(file),
        ),
      ),
    );
  } on Object catch (error) {
    messenger.showSnackBar(
      SnackBar(content: Text('Export failed: ${_describe(error, directory)}')),
    );
  }
}

/// Select a file in the platform's file manager.
Future<void> revealFile(String path) async {
  try {
    if (Platform.isMacOS) {
      await _files.invokeMethod<void>('reveal', path);
    } else if (Platform.isWindows) {
      await Process.run('explorer', ['/select,', path]);
    } else {
      await Process.run('xdg-open', [File(path).parent.path]);
    }
  } on Object {
    // Nothing useful to do: the file is written, and the snackbar named it.
    await Clipboard.setData(ClipboardData(text: path));
  }
}

/// Open a web page in the default browser. Falls back to copying the link,
/// and says which happened.
Future<bool> openLink(String url) async {
  try {
    if (Platform.isMacOS) {
      await _files.invokeMethod<void>('openUrl', url);
    } else if (Platform.isWindows) {
      await Process.run('rundll32', ['url.dll,FileProtocolHandler', url]);
    } else {
      await Process.run('xdg-open', [url]);
    }
    return true;
  } on Object {
    await Clipboard.setData(ClipboardData(text: url));
    return false;
  }
}

/// A sandbox refusal reads as "Operation not permitted, errno = 1", which
/// tells the user nothing about what to do next.
String _describe(Object error, Directory directory) {
  if (error is FileSystemException && error.osError?.errorCode == 1) {
    return 'CrawlStat is not allowed to write to ${directory.path}.';
  }
  return '$error';
}
