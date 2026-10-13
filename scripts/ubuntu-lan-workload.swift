// Controlled native foreground content for the Ubuntu receiving test.
// No capture, permission requests, input injection or performance assertions.
import Cocoa

let duration = Double(CommandLine.arguments.dropFirst().first ?? "480") ?? 480
let auditPath = CommandLine.arguments.count > 2 ? CommandLine.arguments[2] : nil
let app = NSApplication.shared
app.setActivationPolicy(.regular)
let bounds = NSScreen.screens.first?.visibleFrame
    ?? NSRect(x: 80, y: 80, width: 1000, height: 700)
let window = NSWindow(contentRect: bounds,
                      styleMask: [.titled, .closable], backing: .buffered, defer: false)
window.title = "Classroom → Ubuntu: functional LAN test"
let text = NSTextView(frame: window.contentView!.bounds)
text.autoresizingMask = [.width, .height]
text.font = NSFont.monospacedSystemFont(ofSize: 22, weight: .regular)
text.textColor = .white
text.backgroundColor = .black
text.isEditable = false
window.contentView!.addSubview(text)
window.makeKeyAndOrderFront(nil)
app.activate(ignoringOtherApps: true)
let began = ProcessInfo.processInfo.systemUptime
var lastSecond = -1
var rows: [[String: Any]] = []
let timer = Timer.scheduledTimer(withTimeInterval: 0.2, repeats: true) { _ in
    let elapsed = ProcessInfo.processInfo.systemUptime - began
    if elapsed >= duration { app.terminate(nil); return }
    let tick = Int(elapsed * 5)
    let phase = Int(elapsed / 8) % 3
    text.string = "UBUNTU STUDENT — NHẬN MÀN HÌNH THẬT\n"
        + "Teacher capture / H.264 / UDP / decode / native render\n"
        + "Functional test only — no FPS requirement\n"
        + "Frame \(tick)  ·  phase \(phase)\n\n"
        + (0..<24).map { "\($0 + (phase == 1 ? tick : 0)): fn main() { latest_frame(); } // Tiếng Việt" }.joined(separator: "\n")
    text.backgroundColor = phase == 2
        ? NSColor(calibratedRed: Double(tick % 8) / 12, green: 0.15, blue: 0.35, alpha: 1)
        : .black
    if Int(elapsed) != lastSecond {
        lastSecond = Int(elapsed)
        if !app.isActive {
            window.makeKeyAndOrderFront(nil)
            app.activate(ignoringOtherApps: true)
        }
        rows.append(["elapsed_seconds": elapsed, "tick": tick,
                     "phase": phase, "visible": window.isVisible,
                     "foreground": NSWorkspace.shared.frontmostApplication?.processIdentifier == ProcessInfo.processInfo.processIdentifier])
        if rows.count > 1000 { rows.removeFirst() }
        if let path = auditPath,
           let data = try? JSONSerialization.data(withJSONObject: rows, options: [.prettyPrinted]) {
            try? data.write(to: URL(fileURLWithPath: path), options: .atomic)
        }
    }
}
app.run()
timer.invalidate()
