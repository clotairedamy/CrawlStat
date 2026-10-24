import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'src/app.dart';
import 'src/rust/api/license.dart' as license;
import 'src/rust/frb_generated.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();

  // Load the engine before the first frame: every screen reads from it, and a
  // half-initialised bridge would surface as a confusing error rather than a
  // clear one.
  await RustLib.init();

  // Read the stored licence before the first frame, so a Pro user never sees
  // the free-tier limit flash up while it loads. A missing or unreadable
  // licence is not an error: it means the free tier.
  await license.loadLicense();

  runApp(const ProviderScope(child: CrawlStatApp()));
}
