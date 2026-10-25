import 'package:flutter/widgets.dart';
import 'package:lucide_icons_flutter/lucide_icons.dart';

/// Every icon in CrawlStat, named for what it means rather than what it draws.
///
/// The set is [Lucide](https://lucide.dev): thin, even-weight line icons on a
/// 24px grid. Material's own icons are a mix of filled and outlined shapes at
/// different optical weights, which next to a soft, low-contrast interface
/// reads as heavy and slightly mismatched — the icons were the last thing in
/// the app still shouting.
///
/// Going through one file rather than scattering `LucideIcons.foo` through the
/// screens means a name here says what the icon is *for*: swapping the set
/// again is one file, and nobody has to remember whether "discovered" was a
/// compass or a globe.
///
/// Lucide ships as a pure-Dart font package — no native code, nothing to build
/// — so it does not break the rule about keeping this app free of plugins.
class AppIcon {
  const AppIcon._();

  // Navigation.
  static const IconData projects = LucideIcons.layoutDashboard;
  static const IconData crawl = LucideIcons.radar;
  static const IconData results = LucideIcons.table2;
  static const IconData issues = LucideIcons.listChecks;
  static const IconData visualise = LucideIcons.network;
  static const IconData settings = LucideIcons.settings;
  static const IconData collapse = LucideIcons.chevronsLeft;
  static const IconData expand = LucideIcons.chevronsRight;
  static const IconData signOut = LucideIcons.logOut;

  // Actions.
  static const IconData add = LucideIcons.plus;
  static const IconData remove = LucideIcons.minus;
  static const IconData refresh = LucideIcons.refreshCw;
  static const IconData play = LucideIcons.play;
  static const IconData pause = LucideIcons.pause;
  static const IconData stop = LucideIcons.square;
  static const IconData download = LucideIcons.download;
  static const IconData search = LucideIcons.search;
  static const IconData filter = LucideIcons.filter;
  static const IconData clearFilter = LucideIcons.filterX;
  static const IconData columns = LucideIcons.columns;
  static const IconData copy = LucideIcons.copy;
  static const IconData close = LucideIcons.x;
  static const IconData more = LucideIcons.moreVertical;
  static const IconData open = LucideIcons.externalLink;

  // Crawl concepts.
  static const IconData folder = LucideIcons.folder;
  static const IconData folderOpen = LucideIcons.folderOpen;
  static const IconData discovered = LucideIcons.compass;
  static const IconData queued = LucideIcons.clock;
  static const IconData link = LucideIcons.link;
  static const IconData brokenLink = LucideIcons.unlink;
  static const IconData server = LucideIcons.server;
  static const IconData web = LucideIcons.globe;
  static const IconData sitemap = LucideIcons.map;
  static const IconData structure = LucideIcons.gitBranch;
  static const IconData waiting = LucideIcons.hourglass;
  static const IconData noImage = LucideIcons.imageOff;
  static const IconData report = LucideIcons.fileText;
  static const IconData indexable = LucideIcons.badgeCheck;
  static const IconData pro = LucideIcons.sparkles;
  static const IconData priority = LucideIcons.target;
  static const IconData quickWin = LucideIcons.zap;
  static const IconData save = LucideIcons.save;
  static const IconData localModel = LucideIcons.cpu;

  // Severity. These three carry meaning, so they keep their familiar shapes:
  // a triangle warns, a circle informs, a tick confirms.
  static const IconData error = LucideIcons.alertCircle;
  static const IconData warning = LucideIcons.alertTriangle;
  static const IconData ok = LucideIcons.checkCircle2;

  // Sorting.
  static const IconData sortAscending = LucideIcons.arrowUp;
  static const IconData sortDescending = LucideIcons.arrowDown;
  static const IconData chevronUp = LucideIcons.chevronUp;
  static const IconData chevronDown = LucideIcons.chevronDown;
  static const IconData chevronRight = LucideIcons.chevronRight;
}
