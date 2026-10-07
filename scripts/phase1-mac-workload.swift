import Cocoa
let seconds = Double(CommandLine.arguments.dropFirst().first ?? "60") ?? 60
let app = NSApplication.shared
app.setActivationPolicy(.regular)
// NSScreen.main is nil until a key window exists. Use the first attached
// display to size the test window, with a bounded fallback for headless runs.
let screenFrame = NSScreen.screens.first?.visibleFrame
    ?? NSRect(x: 100, y: 100, width: 1280, height: 800)
let window = NSWindow(contentRect: screenFrame,
                      styleMask: [.titled, .closable, .resizable], backing: .buffered, defer: false)
window.title = "Classroom Phase 1 repeatable workload"
let text = NSTextView(frame: window.contentView!.bounds)
text.autoresizingMask = [.width, .height]
text.font = NSFont.monospacedSystemFont(ofSize: 20, weight: .regular)
text.textColor = .white; text.backgroundColor = .black
window.contentView!.addSubview(text)
window.makeKeyAndOrderFront(nil); app.activate(ignoringOtherApps: true)
let audit = CommandLine.arguments.count > 2 ? CommandLine.arguments[2] : nil
var auditRows: [[String: Any]] = []
let start = Date(); var previous = -1; var tick = 0
let timer = Timer.scheduledTimer(withTimeInterval: 0.05, repeats: true) { _ in
    let elapsed = Date().timeIntervalSince(start)
    if elapsed >= seconds {
        if let audit = audit, let data = try? JSONSerialization.data(withJSONObject: auditRows, options: [.prettyPrinted]) {
            try? data.write(to: URL(fileURLWithPath: audit))
        }
        app.terminate(nil); return
    }
    let phase = Int(elapsed / 10) % 5; tick += 1
    // Other applications may activate during the run. Keep
    // the authorized test content foreground, then audit on subsequent ticks.
    if tick % 20 == 0 && !app.isActive {
        window.makeKeyAndOrderFront(nil)
        app.activate(ignoringOtherApps: true)
    }
    if tick % 20 == 0 && auditRows.count < 660 {
        auditRows.append(["elapsed_s": elapsed, "phase": phase,
                          "foreground": NSWorkspace.shared.frontmostApplication?.processIdentifier == ProcessInfo.processInfo.processIdentifier,
                          "visible": window.isVisible, "width": window.frame.width, "height": window.frame.height])
    }
    if phase != previous { text.string = ""; previous = phase }
    switch phase {
    case 0:
        if text.string.isEmpty { text.string = "STATIC: readable code / terminal / Tiếng Việt\n" + String(repeating: "fn main() { println!(\"Classroom 1080p\"); }\n", count: 25) }
    case 1: text.string += "let frame_\(tick) = latest_frame();\n"; text.scrollToEndOfDocument(nil)
    case 2: text.string = (0..<45).map { "\(tick+$0): scrolling source / freshness / RTP H.264" }.joined(separator: "\n")
    case 3: text.string += String(format: "[%.3f] INFO bounded latest-frame slot tick=%d\n", elapsed, tick); text.scrollToEndOfDocument(nil)
    default:
        text.backgroundColor = NSColor(calibratedRed: Double(tick*17%256)/255, green: Double(tick*31%256)/255, blue: Double(tick*7%256)/255, alpha: 1)
        text.string = "MOTION STRESS \(elapsed)\n" + String(repeating: "Moving full-screen color and text ", count: 150)
    }
    if phase != 4 { text.backgroundColor = .black }
}
app.run()
timer.invalidate()
