import Cocoa
import FlutterMacOS

@main
class AppDelegate: FlutterAppDelegate {
  /// Separate-process child windows quit with their last window; the console only hides.
  private var isObserve: Bool { isChildWindow }

  override func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
    // Teacher hides its window while the room stays active. AppKit must not
    // terminate that frontend when its last visible window is ordered out.
    return isObserve
  }

  /// Any separate Teacher window (Observe, files, apps…), as opposed to the
  /// console itself. Each runs as its own process with its own Dock icon.
  private var isChildWindow: Bool {
    ["--observe-id", "--files-id", "--activity-id", "--internet-ids", "--vscode-ids",
     "--apps-ids", "--terminal-id", "--hardware-control"]
      .contains { CommandLine.arguments.contains($0) }
  }

  /// Puts this process's window above everything, restoring it if minimized.
  private func raiseOwnWindow(_ app: NSApplication) {
    guard let window = mainFlutterWindow else { return }
    if window.isMiniaturized { window.deminiaturize(nil) }
    window.makeKeyAndOrderFront(nil)
    app.activate(ignoringOtherApps: true)
  }

  override func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
    if isChildWindow, let window = mainFlutterWindow, window.isVisible || window.isMiniaturized {
      // The Dock icon of a window raises *that* window. Routing this to the
      // console (below) used to bring the console over it instead.
      raiseOwnWindow(sender)
    } else if isObserve {
      // This process has no window left to show. All windows share a bundle,
      // so Finder may deliver the reopen here; route it to the console
      // through authenticated IPC.
      if let controller = mainFlutterWindow?.contentViewController as? FlutterViewController {
        FlutterMethodChannel(name: "classroom/lifecycle", binaryMessenger: controller.engine.binaryMessenger)
          .invokeMethod("reopenConsole", arguments: nil)
      }
    } else {
      raiseOwnWindow(sender)
    }
    return false
  }

  override func applicationDidBecomeActive(_ notification: Notification) {
    super.applicationDidBecomeActive(notification)
    // Switching to a window's Dock icon or Cmd-Tab entry must put that window
    // on top even when this app's other-process siblings are in front.
    if isChildWindow, let window = mainFlutterWindow, window.isVisible, !window.isKeyWindow {
      window.makeKeyAndOrderFront(nil)
    }
  }

  override func applicationSupportsSecureRestorableState(_ app: NSApplication) -> Bool {
    return true
  }
}
