import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:go_router/go_router.dart';

import 'screens/accessibility_screen.dart';
import 'screens/compare_screen.dart';
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

  /// A crawl's own page, on its Overview tab — the live crawl, while it runs.
  static const project = '/project';

  /// Where the live crawl used to have a screen of its own. Kept as a
  /// redirect to [project] so a saved `CRAWLSTAT_INITIAL_ROUTE` still works.
  static const crawl = '/crawl';
  static const results = '/results';
  static const issues = '/issues';
  static const plan = '/plan';
  static const visualise = '/visualise';
  static const ux = '/reader-flow';
  static const accessibility = '/accessibility';
  static const compare = '/compare';
  static const settings = '/settings';

  /// The tabs of a crawl's page, which all show the open crawl.
  static const inProject = {
    project,
    issues,
    results,
    plan,
    visualise,
    ux,
    accessibility,
  };

  /// The Issues tab with one issue selected.
  static String issue(String id) =>
      Uri(path: issues, queryParameters: {'issue': id}).toString();

  /// Compare opened on two particular crawls, oldest first.
  static String comparing({required String before, required String after}) =>
      Uri(
        path: compare,
        queryParameters: {'before': before, 'after': after},
      ).toString();
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
            redirect: (context, state) => Routes.project,
          ),
          // Everything about one crawl lives on that crawl's page, under a
          // header that says which crawl it is.
          ShellRoute(
            builder: (context, state, child) =>
                ProjectShell(location: state.uri.path, child: child),
            routes: [
              GoRoute(
                path: Routes.project,
                pageBuilder: (context, state) =>
                    const NoTransitionPage(child: ProjectOverview()),
              ),
              GoRoute(
                path: Routes.results,
                pageBuilder: (context, state) => NoTransitionPage(
                  // `?tab=` lets the Issues tab deep-link into a filtered
                  // table.
                  child: ResultsScreen(
                    initialTab: state.uri.queryParameters['tab'],
                  ),
                ),
              ),
              GoRoute(
                path: Routes.issues,
                pageBuilder: (context, state) => NoTransitionPage(
                  child: IssuesScreen(
                    initialIssue: state.uri.queryParameters['issue'],
                  ),
                ),
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
                path: Routes.accessibility,
                pageBuilder: (context, state) =>
                    const NoTransitionPage(child: AccessibilityScreen()),
              ),
            ],
          ),
          GoRoute(
            path: Routes.compare,
            // Reached from a project card, which says which two crawls. The
            // key makes a new pair start a fresh comparison rather than
            // keeping the last one's pickers.
            pageBuilder: (context, state) {
              final before = state.uri.queryParameters['before'];
              final after = state.uri.queryParameters['after'];
              return NoTransitionPage(
                child: CompareScreen(
                  key: ValueKey('$before|$after'),
                  before: before,
                  after: after,
                ),
              );
            },
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
