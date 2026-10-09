import Cocoa
import FlutterMacOS
import CoreGraphics
import CoreVideo
import CoreImage
import Darwin
import ApplicationServices
import Network

class MainFlutterWindow: NSWindow {
  override func awakeFromNib() {
    self.backgroundColor = NSColor(red: 0.953, green: 0.961, blue: 0.976, alpha: 1)
    let flutterViewController = FlutterViewController()
    let windowFrame = self.frame
    self.contentViewController = flutterViewController
    self.setFrame(windowFrame, display: true)

    RegisterGeneratedPlugins(registry: flutterViewController)
    let textureRegistrar = flutterViewController.registrar(forPlugin: "ClassroomObserveTexture")
    let textureChannel = FlutterMethodChannel(name: "classroom/observe", binaryMessenger: textureRegistrar.messenger)
    var texture: ObserveTexture?
    var textureID: Int64?
    var timer: Timer?
    textureChannel.setMethodCallHandler { call, result in
      switch call.method {
      case "attach":
        guard let args = call.arguments as? [String: Any], let path = args["path"] as? String else {
          result(FlutterError(code: "invalid_path", message: "Observe surface missing", details: nil)); return
        }
        timer?.invalidate()
        // One registration for this engine's lifetime. Unregistering while its
        // raster thread resolves the last Metal frame can invalidate the
        // engine's unretained texture handles. Rebind the map under its lock.
        let next = texture ?? ObserveTexture(path: path)
        next.bind(path: path)
        texture = next
        let id = textureID ?? textureRegistrar.textures.register(next)
        guard id != 0 else {
          result(FlutterError(code: "texture_registration", message: "Observe texture registration failed", details: nil)); return
        }
        textureID = id
        timer = Timer.scheduledTimer(withTimeInterval: 1.0 / 60.0, repeats: true) { _ in
          if next.hasNewFrame() { textureRegistrar.textures.textureFrameAvailable(id) }
        }
        result(id)
      case "activateWindow":
        if let pid = call.arguments as? Int,
           let app = NSRunningApplication(processIdentifier: pid_t(pid)) {
          app.activate(options: [.activateIgnoringOtherApps])
        }
        result(nil)
      case "stats": result(texture?.stats() ?? [:])
      case "detach":
        timer?.invalidate(); timer = nil
        // Keep the single registration/buffer alive until engine teardown.
        // Dart first removes its Texture layer before calling detach.
        texture?.pause()
        result(nil)
      default: result(FlutterMethodNotImplemented)
      }
    }
    let files = FlutterMethodChannel(name: "classroom/files", binaryMessenger: textureRegistrar.messenger)
    files.setMethodCallHandler { call, result in
      let args = call.arguments as? [String: Any] ?? [:]
      switch call.method {
      case "chooseFile":
        ClassroomFiles.chooseFile(directory: args["directory"] as? String ?? "", window: self, result: result)
      case "screenshot":
        ClassroomFiles.screenshot(buffer: texture?.snapshot(), name: args["name"] as? String ?? "Student", window: self, result: result)
      default: result(FlutterMethodNotImplemented)
      }
    }
    let permissions = FlutterMethodChannel(
      name: "classroom/permissions", binaryMessenger: flutterViewController.engine.binaryMessenger)
    permissions.setMethodCallHandler { call, result in
      switch call.method {
      case "restartConsole":
        guard let arguments = call.arguments as? [String] else {
          result(FlutterError(code: "restart_arguments", message: "Console launch arguments are missing.", details: nil)); return
        }
        // Start the exact bundled executable, as existing dependent windows do.
        // LaunchServices/open from this sandbox dropped custom arguments in a
        // native restart test, creating a second helper on the default port.
        guard let executable = Bundle.main.executableURL else {
          result(FlutterError(code: "restart_executable", message: "Console executable is unavailable.", details: nil)); return
        }
        DispatchQueue.global(qos: .utility).async {
          let launcher = Process()
          launcher.executableURL = executable
          launcher.arguments = arguments
          do {
            try launcher.run()
            // Catch immediate launch failures while retaining the old window.
            Thread.sleep(forTimeInterval: 0.5)
            let success = launcher.isRunning
            DispatchQueue.main.async {
              if success {
                result(true)
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.2) { NSApp.terminate(nil) }
              } else {
                result(FlutterError(code: "restart_failed", message: "macOS could not reopen this Console.", details: nil))
              }
            }
          } catch {
            DispatchQueue.main.async { result(FlutterError(code: "restart_failed", message: error.localizedDescription, details: nil)) }
          }
        }
      case "status":
        ClassroomPermissions.helper(["--permission-status"]) { helper in
          result(["screen": CGPreflightScreenCaptureAccess(),
                  "accessibility": AXIsProcessTrusted(),
                  "helperScreen": helper["screen"] ?? NSNull(),
                  "helperError": helper["error"] ?? NSNull(),
                  "appPath": Bundle.main.bundlePath,
                  "helperPath": ClassroomPermissions.helperBundle.path])
        }
      case "requestHelperScreen":
        ClassroomPermissions.helper(["--permission-request-screen"]) { result($0) }
      case "requestAccessibility":
        let options = [kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true]
        result(AXIsProcessTrustedWithOptions(options as CFDictionary))
      case "openAccessibilitySettings":
        result(ClassroomPermissions.openSettings("Privacy_Accessibility"))
      case "openLocalNetworkSettings":
        result(ClassroomPermissions.openSettings("Privacy_LocalNetwork"))
      case "revealApps":
        NSWorkspace.shared.activateFileViewerSelecting([
          Bundle.main.bundleURL, ClassroomPermissions.helperBundle])
        result(nil)
      case "probeNetwork":
        guard let args = call.arguments as? [String: Any],
              let host = args["host"] as? String, let port = args["port"] as? Int,
              ClassroomPermissions.isPrivateIPv4(host), (1...65535).contains(port) else {
          result(FlutterError(code: "invalid_endpoint", message: "Enter a private LAN IPv4 address and TCP port 1–65535.", details: nil)); return
        }
        ClassroomPermissions.probe(host: host, port: port) { parent in
          ClassroomPermissions.helper(["--permission-probe-network", "\(host):\(port)"]) { helper in
            result(["console": parent, "helper": helper["network"] ??
              ["state": "unverified", "detail": helper["error"] ?? "Helper unavailable"]])
          }
        }
      case "screenRecordingGranted":
        result(CGPreflightScreenCaptureAccess())
      case "openScreenRecordingSettings":
        // The OS owns consent. Request the first-use dialog, then navigate to
        // the pane; a denied/revoked permission must be changed by the owner.
        if !CGPreflightScreenCaptureAccess() { _ = CGRequestScreenCaptureAccess() }
        let address: String
        if #available(macOS 13.0, *) {
          address = "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_ScreenCapture"
        } else {
          address = "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
        }
        let opened = NSWorkspace.shared.open(URL(string: address)!)
          || NSWorkspace.shared.open(URL(string: "x-apple.systempreferences:com.apple.preference.security")!)
        if opened { result(true) }
        else { result(FlutterError(code: "settings_unavailable", message: "Open System Settings > Privacy & Security > Screen Recording.", details: nil)) }
      default:
        result(FlutterMethodNotImplemented)
      }
    }

    super.awakeFromNib()
    self.orderOut(nil)
  }
}

// The only video IPC is a fixed-size native map, protected with a nonblocking
// flock. No frame queue, file writes per frame, or Dart byte arrays.
final class ObserveTexture: NSObject, FlutterTexture {
  private var path: String
  private var active = true
  private let lock = NSLock()
  private var fd: Int32 = -1
  private var map: UnsafeMutableRawPointer?
  private let capacity = 32 + 1920 * 1080 * 4
  private var announced: UInt64 = 0
  private var copied: UInt64 = 0
  private var copies: UInt64 = 0
  private var width = 0
  private var height = 0
  private var copiedAt: UInt64 = 0
  private var buffer: CVPixelBuffer?
  private var pool: CVPixelBufferPool?
  private var poolWidth = 0, poolHeight = 0
  private var poolMisses: UInt64 = 0
  private var lastConnectError = ""
  private var sourceSequence: UInt64 = 0
  private var frameNotifications: UInt64 = 0
  private var copyAttempts: UInt64 = 0
  private var copyFailures: UInt64 = 0
  private var lockMisses: UInt64 = 0
  init(path: String) { self.path = path }
  deinit { if let map = map { munmap(map, capacity) }; if fd >= 0 { Darwin.close(fd) } }
  private func connect() -> Bool {
    if map != nil { return true }
    fd = Darwin.open(path, O_RDONLY)
    if fd < 0 {
      lastConnectError = "open errno=\(errno)"
      return false
    }
    var st = stat()
    if fstat(fd, &st) != 0 {
      lastConnectError = "fstat errno=\(errno)"
      Darwin.close(fd); fd = -1; return false
    }
    if st.st_size != capacity {
      lastConnectError = "size=\(st.st_size) expected=\(capacity)"
      Darwin.close(fd); fd = -1; return false
    }
    let pointer = mmap(nil, capacity, PROT_READ, MAP_SHARED, fd, 0)
    if pointer == MAP_FAILED {
      lastConnectError = "mmap errno=\(errno)"
      Darwin.close(fd); fd = -1; return false
    }
    map = pointer; return true
  }
  func bind(path: String) {
    lock.lock(); defer { lock.unlock() }
    active = true
    if self.path != path {
      if let map = map { munmap(map, capacity) }
      if fd >= 0 { Darwin.close(fd) }
      map = nil; fd = -1; self.path = path
      copied = 0; sourceSequence = 0; width = 0; height = 0
      lastConnectError = ""
    }
    _ = connect()
  }
  func pause() { lock.lock(); active = false; lock.unlock() }
  func hasNewFrame() -> Bool {
    lock.lock(); defer { lock.unlock() }
    guard active, connect(), let map = map else { return false }
    guard flock(fd, LOCK_SH | LOCK_NB) == 0 else { lockMisses += 1; return false }
    defer { flock(fd, LOCK_UN) }
    let sequence = map.load(fromByteOffset: 8, as: UInt64.self)
    sourceSequence = sequence
    if sequence == 0 || sequence == copied { return false }
    announced = sequence
    frameNotifications += 1
    return true
  }
  func copyPixelBuffer() -> Unmanaged<CVPixelBuffer>? {
    lock.lock(); defer { lock.unlock() }
    guard active else { return nil }
    copyAttempts += 1
    guard connect(), let map = map else {
      copyFailures += 1
      return buffer.map { Unmanaged.passRetained($0) }
    }
    guard flock(fd, LOCK_SH | LOCK_NB) == 0 else {
      lockMisses += 1
      return buffer.map { Unmanaged.passRetained($0) }
    }
    defer { flock(fd, LOCK_UN) }
    let w = Int(map.load(as: UInt32.self)), h = Int(map.load(fromByteOffset: 4, as: UInt32.self))
    let sequence = map.load(fromByteOffset: 8, as: UInt64.self)
    sourceSequence = sequence
    guard w > 0, h > 0, w <= 1920, h <= 1080, sequence > 0 else { copyFailures += 1; return nil }
    if sequence != copied {
      // Reuse IOSurfaces through Core Video, never overwrite a buffer owned by
      // Flutter. Bound allocation to four surfaces; keep latest state on pressure
      // instead of allocating indefinitely or blocking the raster thread.
      if pool == nil || poolWidth != w || poolHeight != h {
        let attrs: [String: Any] = [
          kCVPixelBufferWidthKey as String: w, kCVPixelBufferHeightKey as String: h,
          kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA,
          kCVPixelBufferIOSurfacePropertiesKey as String: [:],
          kCVPixelBufferMetalCompatibilityKey as String: true]
        var nextPool: CVPixelBufferPool?
        guard CVPixelBufferPoolCreate(kCFAllocatorDefault, nil, attrs as CFDictionary, &nextPool) == kCVReturnSuccess else { copyFailures += 1; return nil }
        pool = nextPool; poolWidth = w; poolHeight = h
      }
      var next: CVPixelBuffer?
      let limit = [kCVPixelBufferPoolAllocationThresholdKey as String: 4] as CFDictionary
      guard CVPixelBufferPoolCreatePixelBufferWithAuxAttributes(kCFAllocatorDefault, pool!, limit, &next) == kCVReturnSuccess, let next = next else {
        poolMisses += 1
        return buffer.map { Unmanaged.passRetained($0) }
      }
      CVPixelBufferLockBaseAddress(next, [])
      if let dest = CVPixelBufferGetBaseAddress(next) {
        let stride = CVPixelBufferGetBytesPerRow(next)
        for row in 0..<h { memcpy(dest.advanced(by: row * stride), map.advanced(by: 32 + row * w * 4), w * 4) }
      } else {
        copyFailures += 1
        CVPixelBufferUnlockBaseAddress(next, [])
        return nil
      }
      CVPixelBufferUnlockBaseAddress(next, [])
      buffer = next; copied = sequence; copies += 1; width = w; height = h
      copiedAt = UInt64(Date().timeIntervalSince1970 * 1_000_000)
    }
    return buffer.map { Unmanaged.passRetained($0) }
  }
  func snapshot() -> CVPixelBuffer? {
    lock.lock(); defer { lock.unlock() }
    return active && copied > 0 ? buffer : nil
  }
  func stats() -> [String: Any] {
    lock.lock(); defer { lock.unlock() }
    if connect(), let map = map, flock(fd, LOCK_SH | LOCK_NB) == 0 {
      sourceSequence = map.load(fromByteOffset: 8, as: UInt64.self)
      flock(fd, LOCK_UN)
    }
    return ["width": width, "height": height, "sequence": copied, "source_sequence": sourceSequence, "native_frames": copies, "frame_notifications": frameNotifications, "copy_attempts": copyAttempts, "copy_failures": copyFailures, "lock_misses": lockMisses, "last_copy_unix_us": copiedAt, "connected": map != nil, "connect_error": lastConnectError, "pool_misses": poolMisses, "active": active]
  }
}

// User-selected files stay under Powerbox/security-scoped access. Only a staged
// copy in the app container is passed to Rust; video pixels never pass through Dart.
private enum ClassroomFiles {
  static func chooseFile(directory: String, window: NSWindow, result: @escaping FlutterResult) {
    let outbox = URL(fileURLWithPath: directory).appendingPathComponent("outbox", isDirectory: true)
    guard outbox.standardizedFileURL.path.hasPrefix(NSHomeDirectory() + "/") else {
      result(FlutterError(code: "invalid_outbox", message: "Outgoing folder must be inside the app container", details: nil)); return
    }
    let panel = NSOpenPanel()
    panel.title = "Send a file to Students"
    panel.prompt = "Choose file"
    panel.canChooseDirectories = false; panel.canChooseFiles = true
    panel.allowsMultipleSelection = false
    panel.beginSheetModal(for: window) { response in
      guard response == .OK, let source = panel.url else { result(nil); return }
      let access = source.startAccessingSecurityScopedResource()
      DispatchQueue.global(qos: .utility).async {
        defer { if access { source.stopAccessingSecurityScopedResource() } }
        let staged = outbox.appendingPathComponent(UUID().uuidString)
        do {
          let values = try source.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey])
          guard values.isRegularFile == true, let size = values.fileSize, size <= 4 * 1024 * 1024 * 1024 else {
            throw NSError(domain: "Classroom", code: 1, userInfo: [NSLocalizedDescriptionKey: "Choose a regular file up to 4 GiB"])
          }
          try FileManager.default.createDirectory(at: outbox, withIntermediateDirectories: true)
          try FileManager.default.copyItem(at: source, to: staged)
          DispatchQueue.main.async { result(["path": staged.path, "name": source.lastPathComponent, "size": size]) }
        } catch {
          try? FileManager.default.removeItem(at: staged)
          DispatchQueue.main.async { result(FlutterError(code: "file_prepare", message: error.localizedDescription, details: nil)) }
        }
      }
    }
  }
  static func desktop(window: NSWindow, completion: @escaping (URL?) -> Void) {
    guard let account = getpwuid(getuid()), let home = account.pointee.pw_dir else {
      completion(nil); return
    }
    let actualHome = String(cString: home)
    let expected = URL(fileURLWithPath: actualHome).appendingPathComponent("Desktop", isDirectory: true)
    if let data = UserDefaults.standard.data(forKey: "classroom.desktop.bookmark") {
      var stale = false
      if let url = try? URL(resolvingBookmarkData: data, options: .withSecurityScope, relativeTo: nil, bookmarkDataIsStale: &stale),
         !stale, url.standardizedFileURL == expected.standardizedFileURL, url.startAccessingSecurityScopedResource() {
        completion(url); return
      }
    }
    let panel = NSOpenPanel()
    panel.title = "Allow Classroom to save screenshots to Desktop"
    panel.message = "Select your Desktop once. Screenshots will always be saved there."
    panel.directoryURL = expected; panel.prompt = "Allow Desktop"
    panel.canChooseFiles = false; panel.canChooseDirectories = true
    panel.allowsMultipleSelection = false
    panel.beginSheetModal(for: window) { response in
      guard response == .OK, let url = panel.url,
            url.standardizedFileURL == expected.standardizedFileURL else { completion(nil); return }
      do {
        let data = try url.bookmarkData(options: .withSecurityScope, includingResourceValuesForKeys: nil, relativeTo: nil)
        UserDefaults.standard.set(data, forKey: "classroom.desktop.bookmark")
        _ = url.startAccessingSecurityScopedResource()
        completion(url)
      } catch { completion(nil) }
    }
  }
  static func screenshot(buffer: CVPixelBuffer?, name: String, window: NSWindow, result: @escaping FlutterResult) {
    guard let buffer = buffer else { result(FlutterError(code: "no_frame", message: "Wait for a live Student frame", details: nil)); return }
    desktop(window: window) { folder in
      guard let folder = folder else { result(FlutterError(code: "desktop_permission", message: "Select Desktop to allow screenshot saving", details: nil)); return }
      let clean = String(name.unicodeScalars.map { CharacterSet.alphanumerics.contains($0) || "-_".unicodeScalars.contains($0) ? String($0) : "_" }.joined().prefix(80))
      let date = DateFormatter(); date.locale = Locale(identifier: "en_US_POSIX"); date.dateFormat = "yyyy-MM-dd_HH-mm-ss-SSS"
      let filename = "Classroom_\(clean)_\(date.string(from: Date()))"
      DispatchQueue.global(qos: .utility).async {
        defer { folder.stopAccessingSecurityScopedResource() }
        do {
          let image = CIImage(cvPixelBuffer: buffer)
          guard let cg = CIContext().createCGImage(image, from: image.extent),
                let png = NSBitmapImageRep(cgImage: cg).representation(using: .png, properties: [:]) else {
            throw NSError(domain: "Classroom", code: 2, userInfo: [NSLocalizedDescriptionKey: "Could not create PNG"])
          }
          var suffix = 1
          var destination: URL
          while true {
            let extra = suffix == 1 ? "" : "-\(suffix)"
            destination = folder.appendingPathComponent("\(filename)\(extra).png")
            let descriptor = Darwin.open(
              destination.path,
              O_WRONLY | O_CREAT | O_EXCL,
              mode_t(S_IRUSR | S_IWUSR)
            )
            if descriptor >= 0 {
              let handle = FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
              do {
                handle.write(png)
                try handle.synchronize()
                try handle.close()
              } catch {
                try? handle.close()
                try? FileManager.default.removeItem(at: destination)
                throw error
              }
              break
            }
            guard errno == EEXIST else {
              throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno))
            }
            suffix += 1
          }
          let savedPath = destination.path
          DispatchQueue.main.async { result(savedPath) }
        } catch { DispatchQueue.main.async { result(FlutterError(code: "screenshot_save", message: error.localizedDescription, details: nil)) } }
      }
    }
  }
}


// Permission probes are bounded, user initiated, and run from the identities
// that actually need access. No private databases, policy reset or grant writes.
private enum ClassroomPermissions {
  static var helperBundle: URL {
    Bundle.main.bundleURL.appendingPathComponent("Contents/Helpers/Classroom Agent.app")
  }
  static func openSettings(_ pane: String) -> Bool {
    let prefix: String
    if #available(macOS 13.0, *) {
      prefix = "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?"
    } else {
      prefix = "x-apple.systempreferences:com.apple.preference.security?"
    }
    return NSWorkspace.shared.open(URL(string: prefix + pane)!)
  }
  static func isPrivateIPv4(_ host: String) -> Bool {
    let parts = host.split(separator: ".", omittingEmptySubsequences: false)
    guard parts.count == 4,
          parts.allSatisfy({ !$0.isEmpty && $0.allSatisfy({ $0.isASCII && $0.isNumber }) }),
          let a = Int(parts[0]), let b = Int(parts[1]),
          parts.allSatisfy({ (0...255).contains(Int($0) ?? -1) }) else { return false }
    return a == 10 || (a == 172 && (16...31).contains(b)) || (a == 192 && b == 168)
  }
  static func helper(_ args: [String], completion: @escaping ([String: Any]) -> Void) {
    DispatchQueue.global(qos: .utility).async {
      let process = Process(), output = Pipe(), errors = Pipe()
      process.executableURL = helperBundle.appendingPathComponent("Contents/MacOS/Classroom")
      process.arguments = args
      process.standardOutput = output; process.standardError = errors
      do {
        try process.run()
        let deadline = Date().addingTimeInterval(6)
        while process.isRunning && Date() < deadline { Thread.sleep(forTimeInterval: 0.05) }
        if process.isRunning {
          process.terminate()
          DispatchQueue.main.async { completion(["error": "Permission check timed out. Finish the macOS dialog, then refresh."]) }
          return
        }
        let data = output.fileHandleForReading.readDataToEndOfFile()
        let parsed = try JSONSerialization.jsonObject(with: data) as? [String: Any]
        let value = parsed ?? ["error": "Bundled helper does not support permission checks. A paired update is needed."]
        DispatchQueue.main.async { completion(value) }
      } catch {
        DispatchQueue.main.async { completion(["error": error.localizedDescription]) }
      }
    }
  }
  static func probe(host: String, port: Int, completion: @escaping ([String: Any]) -> Void) {
    let connection = NWConnection(host: NWEndpoint.Host(host),
                                  port: NWEndpoint.Port(rawValue: UInt16(port))!, using: .tcp)
    let queue = DispatchQueue(label: "classroom.permission-probe")
    var finished = false
    func finish(_ state: String, _ detail: String) {
      guard !finished else { return }; finished = true
      connection.stateUpdateHandler = nil; connection.cancel()
      DispatchQueue.main.async { completion(["state": state, "detail": detail]) }
    }
    connection.stateUpdateHandler = { state in
      switch state {
      case .ready: finish("connected", "TCP connection succeeded")
      case .failed(let error): finish("unverified", error.localizedDescription)
      default: break // .waiting can include an unanswered system consent prompt.
      }
    }
    connection.start(queue: queue)
    queue.asyncAfter(deadline: .now() + 4) {
      finish("unverified", "No connection yet. Check Allow, address, service and firewall, then retry.")
    }
  }
}
