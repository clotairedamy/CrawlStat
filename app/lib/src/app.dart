import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'router.dart';
import 'state/settings.dart';
import 'theme/scrolling.dart';
import 'theme/theme.dart';

class CrawlStatApp extends ConsumerStatefulWidget {
  const CrawlStatApp({super.key});

  @override
  ConsumerState<CrawlStatApp> createState() => _CrawlStatAppState();
}

class _CrawlStatAppState extends ConsumerState<CrawlStatApp> {
  // Built once: rebuilding a GoRouter would reset the navigation stack on every
  // theme change.
  final _router = buildRouter();

  @override
  Widget build(BuildContext context) {
    final palette = CrawlStatPalette.byName(
      ref.watch(settingsProvider.select((settings) => settings.paletteName)),
    );
    final themeMode = ref.watch(
      settingsProvider.select((settings) => settings.themeMode),
    );

    return MaterialApp.router(
      title: 'CrawlStat',
      debugShowCheckedModeBanner: false,
      theme: CrawlStatTheme.light(palette),
      darkTheme: CrawlStatTheme.dark(palette),
      themeMode: themeMode,
      routerConfig: _router,
      // No scrollbars anywhere: see CrawlStatScrollBehavior for why.
      scrollBehavior: const CrawlStatScrollBehavior(),
    );
  }
}
