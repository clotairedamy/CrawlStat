import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:go_router/go_router.dart';

import 'screens/compare_screen.dart';
import 'screens/crawl_screen.dart';
import 'screens/issues_screen.dart';
import 'screens/plan_screen.dart';
import 'screens/projects_screen.dart';
import 'screens/results_screen.dart';
import 'screens/settings_screen.dart';
import 'screens/ux_screen.dart';
import 'screens/visualise_screen.dart';
import 'screens/shell.dart';

/// Route paths, in one place so navigation calls cannot drift from the routes.
class Routes {
  const Routes._();

  static const projects = '/';
  static const crawl = '/crawl';
  static const results = '/results';
  static const issues = '/issues';
  static const plan = '/plan';
  static const visualise = '/visualise';
  static const ux = '/reader-flow';
  static const compare = '/compare';
  static const settings = '/settings';
}

/// The screen the app opens on.
///
/// Normally Projects. A debug build honours `CRAWLSTAT_INITIAL_ROUTE`, which
/// exists so the marketing screenshots can be regenerated the same way every
/// time instead of by somebody clicking around and cropping — stale
/// screenshots are what happens when refreshing them is a manual chore.
///
/// Release builds ignore it: `kDebugMode` is a compile-time constant, so the
/// lookup is removed entirely from a shipped binary.
String initialLocation() {
  if (!kDebugMode) return Routes.projects;
  final route = Platform.environment['CRAWLSTAT_INITIAL_ROUTE'];
  return (route == null || route.isEmpty) ? Routes.projects : route;
}

GoRouter buildRouter() {
  return GoRouter(
    initialLocation: initialLocation(),
    routes: [
      ShellRoute(
        builder: (context, state, child) =>
            AppShell(location: state.uri.path, child: child),
        routes: [
          GoRoute(
            path: Routes.projects,
            pageBuilder: (context, state) =>
                const NoTransitionPage(child: ProjectsScreen()),
          ),
          GoRoute(
            path: Routes.crawl,
            pageBuilder: (context, state) =>
                const NoTransitionPage(child: CrawlScreen()),
          ),
          GoRoute(
            path: Routes.results,
            pageBuilder: (context, state) => NoTransitionPage(
              // `?tab=` lets the Issues screen deep-link into a filtered table.
              child: ResultsScreen(
                initialTab: state.uri.queryParameters['tab'],
              ),
            ),
          ),
          GoRoute(
            path: Routes.issues,
            pageBuilder: (context, state) =>
                const NoTransitionPage(child: IssuesScreen()),
          ),
          GoRoute(
            path: Routes.plan,
            pageBuilder: (context, state) =>
                const NoTransitionPage(child: PlanScreen()),
          ),
          GoRoute(
            path: Routes.visualise,
            pageBuilder: (context, state) =>
                const NoTransitionPage(child: VisualiseScreen()),
          ),
          GoRoute(
            path: Routes.ux,
            pageBuilder: (context, state) =>
                const NoTransitionPage(child: UxScreen()),
          ),
          GoRoute(
            path: Routes.compare,
            pageBuilder: (context, state) =>
                const NoTransitionPage(child: CompareScreen()),
          ),
          GoRoute(
            path: Routes.settings,
            pageBuilder: (context, state) =>
                const NoTransitionPage(child: SettingsScreen()),
          ),
        ],
      ),
    ],
  );
}
