import 'package:crawlstat/src/widgets/common.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('formatBytes', () {
    test('uses the unit a developer expects at each scale', () {
      expect(formatBytes(0), '0 B');
      expect(formatBytes(512), '512 B');
      expect(formatBytes(1024), '1 KB');
      expect(formatBytes(120 * 1024), '120 KB');
      expect(formatBytes(2 * 1024 * 1024), '2.0 MB');
    });

    test('absent sizes render as nothing, not as zero', () {
      // A HEAD request downloads no body; claiming "0 B" would be a lie.
      expect(formatBytes(null), '');
    });
  });

  group('formatMillis', () {
    test('switches units so the number stays short', () {
      expect(formatMillis(80), '80 ms');
      expect(formatMillis(999), '999 ms');
      expect(formatMillis(1500), '1.5 s');
      expect(formatMillis(65000), '1m 05s');
      expect(formatMillis(null), '');
    });
  });

  test('counts are grouped so long numbers stay scannable', () {
    expect(formatCount(999), '999');
    expect(formatCount(1000), '1,000');
    expect(formatCount(128431), '128,431');
  });
}
