import Cocoa
import FlutterMacOS

class MainFlutterWindow: NSWindow {
  override func awakeFromNib() {
    let flutterViewController = FlutterViewController()
    let windowFrame = self.frame
    self.contentViewController = flutterViewController
    self.setFrame(windowFrame, display: true)

    RegisterGeneratedPlugins(registry: flutterViewController)

    // "Show in Finder" for a finished export, and opening a web link in the
    // browser. Two calls, so they live here rather than pulling in plugins.
    let files = FlutterMethodChannel(
      name: "crawlstat/files",
      binaryMessenger: flutterViewController.engine.binaryMessenger)
    files.setMethodCallHandler { call, result in
      guard let argument = call.arguments as? String else {
        result(FlutterMethodNotImplemented)
        return
      }
      switch call.method {
      case "reveal":
        NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: argument)])
        result(nil)
      case "openUrl":
        guard let url = URL(string: argument),
              url.scheme == "https" || url.scheme == "http" else {
          result(FlutterError(code: "bad_url", message: argument, details: nil))
          return
        }
        NSWorkspace.shared.open(url)
        result(nil)
      default:
        result(FlutterMethodNotImplemented)
      }
    }

    // Let the Flutter canvas paint the whole window, title bar included.
    //
    // The default macOS title bar is a light strip that sits above the app's
    // own background, and against a near-black page it reads as a seam across
    // the top of the window. Drawing under it means the title bar is whatever
    // colour Flutter painted there, in light theme and dark alike. The shell
    // keeps its top 46pt clear so the traffic lights have somewhere to sit.
    self.titlebarAppearsTransparent = true
    self.titleVisibility = .hidden
    self.styleMask.insert(.fullSizeContentView)
    self.isMovableByWindowBackground = true

    super.awakeFromNib()
  }
}
