import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../rust/api/accessibility.dart' as rust;

/// The WCAG standard the Accessibility screen measures against.
///
/// WCAG 2.1 AA by default: it is what the ADA Title II rule adopts, what
/// courts look to under Title III, and what EN 301 549 requires in the EU.
class AccessibilityStandard extends Notifier<String> {
  @override
  String build() => 'wcag21aa';

  void select(String id) => state = id;
}

final accessibilityStandardProvider =
    NotifierProvider<AccessibilityStandard, String>(AccessibilityStandard.new);

/// The compliance report for a crawl file, against a standard.
final accessibilityReportProvider =
    FutureProvider.family<rust.A11yReport, (String, String)>((ref, key) {
      final (path, standard) = key;
      return rust.accessibilityReport(dbPath: path, standard: standard);
    });

/// Every page one rule fired on, loaded when the user asks to see them.
final accessibilityRulePagesProvider =
    FutureProvider.family<List<rust.A11yPage>, (String, String)>((ref, key) {
      final (path, rule) = key;
      return rust.accessibilityRulePages(dbPath: path, rule: rule, limit: 200);
    });
