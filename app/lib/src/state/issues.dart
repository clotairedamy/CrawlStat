import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../rust/api/project.dart' as rust;
import '../rust/api/types.dart';

/// The ranked plan for the open crawl.
final actionPlanProvider = FutureProvider.family<rust.ActionPlan, String>((
  ref,
  path,
) {
  return rust.actionPlan(dbPath: path);
});

/// How many affected pages an issue lists inline before handing over to the
/// Pages tab.
const affectedPreview = 50;

/// The first [affectedPreview] pages an issue touches, best-linked first,
/// each with the engine's reason for flagging it.
final issueUrlsProvider =
    FutureProvider.family<
      List<rust.AffectedPage>,
      ({String path, String issueId})
    >(
      (ref, args) => rust.issueUrls(
        dbPath: args.path,
        issueId: args.issueId,
        limit: affectedPreview,
      ),
    );

/// An issue with what the plan knows about it, when the plan has loaded.
class RankedIssue {
  const RankedIssue(this.issue, {this.plan, this.totalPages});

  final Issue issue;

  /// The plan's view of this issue: impact, reach and effort. `null` until
  /// the plan loads, or for an issue the plan leaves out.
  final rust.PlanItem? plan;

  /// Pages in the crawl, for [share].
  final int? totalPages;

  String get id => issue.id;
  String get severity => issue.severity;
  int get pages => issue.urlCount.toInt();

  /// The share of the site this issue touches, 0..1, or `null` when the
  /// size of the site is not known.
  double? get share {
    final total = totalPages;
    if (total == null || total <= 0) return null;
    return (pages / total).clamp(0.0, 1.0);
  }

  /// 0..100 when the plan has scored it.
  double? get impact => plan?.impact;

  /// One change — a template, a setting — fixes many pages at once.
  bool get isTemplateFix {
    final plan = this.plan;
    return plan != null && plan.effort == 'template' && plan.pages > 1;
  }

  /// How many separate changes the fix takes.
  int? get fixes => plan?.distinctFixes.toInt();
}

int severityRank(String severity) => switch (severity) {
  'error' => 0,
  'warning' => 1,
  'notice' => 2,
  _ => 3,
};

/// The issues in the order to fix them: by severity, then within each
/// severity by the plan's impact score, then by how many pages they touch.
///
/// Severity says how bad one instance is; impact says how much fixing all of
/// them is worth — so a warning on the home page's template outranks one on
/// three orphaned pages, while still never outranking an error.
///
/// [totalPages] sizes the site until the plan, which knows it better, loads.
List<RankedIssue> rankIssues(
  List<Issue> issues, {
  rust.ActionPlan? plan,
  int? totalPages,
}) {
  final byId = {
    for (final item in plan?.items ?? <rust.PlanItem>[]) item.issueId: item,
  };
  final total = plan?.totalPages.toInt() ?? totalPages;
  final ranked = [
    for (final issue in issues)
      RankedIssue(issue, plan: byId[issue.id], totalPages: total),
  ];
  ranked.sort((a, b) {
    final bySeverity = severityRank(a.severity) - severityRank(b.severity);
    if (bySeverity != 0) return bySeverity;
    final byImpact = (b.impact ?? -1).compareTo(a.impact ?? -1);
    if (byImpact != 0) return byImpact;
    final byPages = b.pages.compareTo(a.pages);
    if (byPages != 0) return byPages;
    return a.issue.title.compareTo(b.issue.title);
  });
  return ranked;
}

/// "12%", "<1%" or "100%" — never "0%" for something that is there.
String formatShare(double share) {
  if (share <= 0) return '0%';
  if (share < 0.01) return '<1%';
  if (share >= 0.995 && share < 1) return '99%';
  return '${(share * 100).round()}%';
}
