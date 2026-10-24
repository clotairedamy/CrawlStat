import 'package:crawlstat/src/rust/api/license.dart';
import 'package:crawlstat/src/state/license.dart';
import 'package:flutter_test/flutter_test.dart';

LicenseInfo info({
  String status = 'free',
  String plan = 'free',
  int daysRemaining = 0,
  String? fingerprint,
  List<String> features = const [],
  int? maxUrls = 500,
  String? reason,
}) {
  return LicenseInfo(
    status: status,
    plan: plan,
    daysRemaining: daysRemaining,
    keyFingerprint: fingerprint,
    features: features,
    maxUrlsPerCrawl: maxUrls,
    reason: reason,
  );
}

void main() {
  group('describeLicense', () {
    test('tells a free user what the limit is', () {
      expect(describeLicense(info()), contains('500 URLs'));
      expect(describeLicense(info()), startsWith('Free'));
    });

    test('tells an active user when it renews', () {
      final text = describeLicense(
        info(status: 'active', plan: 'pro', daysRemaining: 14, maxUrls: null),
      );
      expect(text, startsWith('Pro'));
      expect(text, contains('14 days'));
    });

    test('says a day, not days, when there is one left', () {
      expect(
        describeLicense(
          info(status: 'active', plan: 'pro', daysRemaining: 1, maxUrls: null),
        ),
        contains('1 day'),
      );
      expect(
        describeLicense(
          info(status: 'active', plan: 'pro', daysRemaining: 1, maxUrls: null),
        ),
        isNot(contains('1 days')),
      );
    });

    test('explains the grace period without alarming anyone', () {
      final text = describeLicense(
        info(status: 'grace', plan: 'pro', daysRemaining: 9, maxUrls: null),
      );
      // Still Pro, and the wording is about checking in rather than losing
      // anything — because nothing has been lost.
      expect(text, startsWith('Pro'));
      expect(text, contains('offline'));
      expect(text, contains('9 days'));
    });

    test('an expired licence reads as needing a check, not as a loss', () {
      final text = describeLicense(info(status: 'expired'));
      expect(text, contains('re-check'));
    });

    test('an unreadable licence shows the reason it gave', () {
      expect(
        describeLicense(
          info(status: 'invalid', reason: 'the signature does not match'),
        ),
        'the signature does not match',
      );
    });
  });

  group('entitlement', () {
    test('Pro features are recognised by name', () {
      final pro = info(
        status: 'active',
        plan: 'pro',
        maxUrls: null,
        features: const ['unlimited_urls', 'xlsx_export'],
      );
      expect(hasFeature(pro, ProFeatures.unlimitedUrls), isTrue);
      expect(hasFeature(pro, ProFeatures.xlsxExport), isTrue);
      expect(hasFeature(pro, ProFeatures.crawlComparison), isFalse);
    });

    test('the free tier grants nothing', () {
      expect(hasFeature(info(), ProFeatures.unlimitedUrls), isFalse);
      expect(info().features, isEmpty);
    });

    test(
      'a feature this build has never heard of does not break the lookup',
      () {
        final granted = info(
          plan: 'pro',
          features: const ['time_travel'],
          maxUrls: null,
        );
        expect(hasFeature(granted, 'time_travel'), isTrue);
        expect(hasFeature(granted, ProFeatures.unlimitedUrls), isFalse);
      },
    );
  });

  group('the URL limit', () {
    test('is a number on the free tier and absent on Pro', () {
      expect(info().maxUrlsPerCrawl, 500);
      expect(info(plan: 'pro', maxUrls: null).maxUrlsPerCrawl, isNull);
    });
  });
}
