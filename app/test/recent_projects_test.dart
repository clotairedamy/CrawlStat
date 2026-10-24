import 'package:crawlstat/src/state/projects.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('promoteToFront', () {
    test('works on an empty list, which is the very first crawl', () {
      // The bug this guards: the stored-paths reader returned `const []` when
      // there was no recents file yet, and the caller mutated it. That is the
      // state of every fresh install, so the first crawl a user ever started
      // failed with "Cannot remove from an unmodifiable list".
      expect(promoteToFront(const [], '/crawls/a.db'), ['/crawls/a.db']);
      expect(promoteToFront(<String>[], '/crawls/a.db'), ['/crawls/a.db']);
    });

    test('puts the newest first', () {
      expect(promoteToFront(['/b.db', '/c.db'], '/a.db'), [
        '/a.db',
        '/b.db',
        '/c.db',
      ]);
    });

    test('moves an existing crawl up rather than duplicating it', () {
      expect(promoteToFront(['/a.db', '/b.db', '/c.db'], '/c.db'), [
        '/c.db',
        '/a.db',
        '/b.db',
      ]);
    });

    test('keeps the list short', () {
      final many = List.generate(30, (index) => '/crawl-$index.db');
      final result = promoteToFront(many, '/new.db', limit: 5);
      expect(result.length, 5);
      expect(result.first, '/new.db');
    });

    test('returns a list the caller can go on to mutate', () {
      // The fix is only complete if the result is growable too.
      final result = promoteToFront(const [], '/a.db');
      expect(() => result.add('/b.db'), returnsNormally);
      expect(
        () => result.removeWhere((path) => path == '/a.db'),
        returnsNormally,
      );
    });

    test('leaves the input alone', () {
      final original = ['/a.db', '/b.db'];
      promoteToFront(original, '/b.db');
      expect(original, ['/a.db', '/b.db']);
    });
  });

  group('withoutPath', () {
    test('works on an empty list', () {
      expect(withoutPath(const [], '/a.db'), isEmpty);
    });

    test('removes every occurrence and leaves the rest in order', () {
      expect(withoutPath(['/a.db', '/b.db', '/a.db'], '/a.db'), ['/b.db']);
      expect(withoutPath(['/a.db', '/b.db'], '/c.db'), ['/a.db', '/b.db']);
    });

    test('returns a growable list', () {
      final result = withoutPath(const ['/a.db'], '/a.db');
      expect(() => result.add('/b.db'), returnsNormally);
    });
  });

  test('the recents limit is sane', () {
    expect(recentProjectLimit, greaterThan(5));
    expect(recentProjectLimit, lessThanOrEqualTo(50));
  });
}
