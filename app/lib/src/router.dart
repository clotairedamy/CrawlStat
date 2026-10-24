import 'package:go_router/go_router.dart';

import 'screens/crawl_screen.dart';
import 'screens/issues_screen.dart';
import 'screens/projects_screen.dart';
import 'screens/results_screen.dart';
import 'screens/settings_screen.dart';
import 'screens/visualise_screen.dart';
import 'screens/shell.dart';

/// Route paths, in one place so navigation calls cannot drift from the routes.
class Routes {
  const Routes._();

  static const projects = '/';
  static const crawl = '/crawl';
  static const results = '/results';
  static const issues = '/issues';
  static const visualise = '/visualise';
  static const settings = '/settings';
}

GoRouter buildRouter() {
  return GoRouter(
    initialLocation: Routes.projects,
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
            path: Routes.visualise,
            pageBuilder: (context, state) =>
                const NoTransitionPage(child: VisualiseScreen()),
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
