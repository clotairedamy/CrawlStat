import 'package:crawlstat/src/state/projects.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final at = DateTime(2026, 2, 24, 14, 32);

  group('suggestedProjectFileName', () {
    test('names the file after the host and the time', () {
      expect(
        suggestedProjectFileName('https://example.com/blog', now: at),
        'example-com-20260224-1432.db',
      );
    });

    test('drops the scheme, the path, the port and www', () {
      for (final url in [
        'https://www.example.com',
        'http://example.com:8080/deep/path?query=1',
        'example.com/',
        'WWW.EXAMPLE.COM',
      ]) {
        expect(
          suggestedProjectFileName(url, now: at),
          'example-com-20260224-1432.db',
          reason: url,
        );
      }
    });

    test('pads the date so a directory of crawls sorts chronologically', () {
      final early = suggestedProjectFileName(
        'a.test',
        now: DateTime(2026, 1, 2, 3, 4),
      );
      expect(early, 'a-test-20260102-0304.db');
      // String sorting and time ordering agree.
      expect(early.compareTo(suggestedProjectFileName('a.test', now: at)), lessThan(0));
    });

    test('strips anything that would be awkward in a filename', () {
      final name = suggestedProjectFileName('https://exa mple!.com/x', now: at);
      expect(name, isNot(contains(' ')));
      expect(name, isNot(contains('!')));
      expect(name, endsWith('.db'));
    });

    test('an unusable URL still produces a filename', () {
      // The dialog validates URLs, but a filename must never come back empty.
      expect(suggestedProjectFileName('', now: at), 'crawl-20260224-1432.db');
      expect(suggestedProjectFileName('///', now: at), 'crawl-20260224-1432.db');
    });
  });
}
