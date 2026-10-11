import Cocoa
import FlutterMacOS

@main
class AppDelegate: FlutterAppDelegate {
  private var isObserve: Bool { CommandLine.arguments.contains("--observe-id") || CommandLine.arguments.contains("--files-id") || CommandLine.arguments.contains("--activity-id") || CommandLine.arguments.contains("--internet-ids") || CommandLine.arguments.contains("--vscode-ids") || CommandLine.arguments.contains("--apps-ids") }

  override func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
    // Teacher hides its window while the room stays active. AppKit must not
    // terminate that frontend when its last visible window is ordered out.
    return isObserve
  }

  override func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
    if isObserve {
      // All windows share a bundle. Finder may deliver reopen to an Observe
      // process; route that request to the console through authenticated IPC.
      if let controller = mainFlutterWindow?.contentViewController as? FlutterViewController {
        FlutterMethodChannel(name: "classroom/lifecycle", binaryMessenger: controller.engine.binaryMessenger)
          .invokeMethod("reopenConsole", arguments: nil)
      }
    } else if let window = mainFlutterWindow {
      if window.isMiniaturized { window.deminiaturize(nil) }
      window.makeKeyAndOrderFront(nil)
      sender.activate(ignoringOtherApps: true)
    }
    return false
  }

  override func applicationSupportsSecureRestorableState(_ app: NSApplication) -> Bool {
    return true
  }
}
