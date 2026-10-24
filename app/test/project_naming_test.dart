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

  group('recent projects', () {
    test('the first crawl on a fresh install is remembered', () {
      // A fresh install reads an empty, unmodifiable list. Mutating it in place
      // crashed the very first crawl before it could start.
      expect(promoteToFront(const [], '/a.db'), ['/a.db']);
    });

    test('a reopened crawl moves to the front without duplicating', () {
      expect(promoteToFront(['/a.db', '/b.db', '/c.db'], '/b.db'), ['/b.db', '/a.db', '/c.db']);
    });

    test('the list is capped', () {
      final many = [for (var i = 0; i < 30; i++) '/$i.db'];
      final result = promoteToFront(many, '/new.db');
      expect(result, hasLength(recentProjectLimit));
      expect(result.first, '/new.db');
    });

    test('forgetting works on an unmodifiable list', () {
      expect(withoutPath(const [], '/a.db'), isEmpty);
      expect(withoutPath(const ['/a.db', '/b.db'], '/a.db'), ['/b.db']);
    });
  });
}
