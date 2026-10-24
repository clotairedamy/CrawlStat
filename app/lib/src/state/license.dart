import 'package:flutter_riverpod/flutter_riverpod.dart';

import '../rust/api/license.dart' as rust;

/// The licence in force, and what it allows.
///
/// The key and the signed token stay in Rust and in the OS keychain; this side
/// only ever sees a status. That is why there is no `licenseKey` field here:
/// the UI cannot leak what it never holds.
class LicenseController extends Notifier<rust.LicenseInfo> {
  @override
  rust.LicenseInfo build() {
    // Synchronous, so the first frame already knows what the user is entitled
    // to — a Pro user must never see the free-tier limit flash up.
    final initial = rust.licenseStatus();
    // Then refresh in the background if one is due. A failure is normal and
    // deliberately silent: the cached token covers a fortnight.
    Future.microtask(_refreshIfDue);
    return initial;
  }

  Future<void> _refreshIfDue() async {
    try {
      final refreshed = await rust.refreshLicenseIfDue();
      state = refreshed;
    } on Object {
      // Nothing to tell the user: they are still licensed.
    }
  }

  /// Activate a key. Throws with the service's own wording on refusal, because
  /// the user is waiting and "something went wrong" is no help.
  Future<void> activate(String licenseKey) async {
    state = await rust.activateLicense(licenseKey: licenseKey);
  }

  Future<void> deactivate() async {
    state = await rust.deactivateLicense();
  }

  /// Check now, from the Settings screen's button.
  Future<void> refreshNow() async {
    state = await rust.refreshLicense();
  }
}

final licenseProvider = NotifierProvider<LicenseController, rust.LicenseInfo>(
  LicenseController.new,
);

/// Whether this install has Pro.
final isProProvider = Provider<bool>(
  (ref) => ref.watch(licenseProvider.select((info) => info.plan == 'pro')),
);

/// URLs allowed per crawl, or `null` for unlimited.
final urlLimitProvider = Provider<int?>(
  (ref) => ref.watch(licenseProvider.select((info) => info.maxUrlsPerCrawl)),
);

/// Whether a Pro feature is available, by its engine feature name.
bool hasFeature(rust.LicenseInfo info, String feature) =>
    info.features.contains(feature);

/// The Pro features, so the UI can gate on them by name rather than by string
/// literals scattered through the screens.
class ProFeatures {
  const ProFeatures._();

  static const unlimitedUrls = 'unlimited_urls';
  static const crawlComparison = 'crawl_comparison';
  static const customExtraction = 'custom_extraction';
  static const scheduledCrawls = 'scheduled_crawls';
  static const xlsxExport = 'xlsx_export';
  static const htmlReport = 'html_report';
  static const byokIntegrations = 'byok_integrations';
}

/// A one-line description of the licence, for the Settings header.
String describeLicense(rust.LicenseInfo info) {
  // A developer entitlement is granted by the engine when CRAWLSTAT_DEV_PRO
  // is set on a debug build. It is said plainly rather than dressed up as a
  // purchase, so nobody mistakes this install for a licensed one.
  if (info.keyFingerprint == 'dev') {
    return 'Pro · developer build (CRAWLSTAT_DEV_PRO)';
  }
  switch (info.status) {
    case 'active':
      return 'Pro · renews in ${info.daysRemaining} '
          'day${info.daysRemaining == 1 ? '' : 's'}';
    case 'grace':
      return 'Pro · offline, ${info.daysRemaining} '
          'day${info.daysRemaining == 1 ? '' : 's'} left before it needs checking';
    case 'expired':
      return 'Your licence needs re-checking — connect and try again';
    case 'invalid':
      return info.reason ?? 'The stored licence could not be read';
    default:
      return 'Free · up to ${info.maxUrlsPerCrawl ?? 0} URLs per crawl';
  }
}
