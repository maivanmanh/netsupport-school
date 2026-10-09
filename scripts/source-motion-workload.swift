// Native deterministic Student-source workload; no permission requests or input hooks.
import Cocoa
let duration = Double(CommandLine.arguments.dropFirst().first ?? "140") ?? 140
let audit = CommandLine.arguments.count > 2 ? CommandLine.arguments[2] : nil
let app = NSApplication.shared
app.setActivationPolicy(.regular)
let display = NSScreen.screens.first!
let usable = display.visibleFrame
let frame = NSRect(x: usable.minX + 40, y: usable.minY + 40,
                   width: min(1200, usable.width - 80), height: min(700, usable.height - 80))
var windows: [NSWindow] = []
var views: [NSTextView] = []
for index in 0..<2 {
    let window = NSWindow(contentRect: frame, styleMask: [.titled, .closable, .resizable], backing: .buffered, defer: false)
    window.title = "Classroom source measurement \(index + 1)"
    window.isReleasedWhenClosed = false
    let scroll = NSScrollView(frame: window.contentView!.bounds)
    scroll.autoresizingMask = [.width, .height]; scroll.hasVerticalScroller = true
    let text = NSTextView(frame: NSRect(x: 0, y: 0, width: frame.width, height: frame.height))
    text.font = NSFont.monospacedSystemFont(ofSize: 18, weight: .regular)
    text.textColor = .white; text.backgroundColor = .black
    text.isVerticallyResizable = true; text.isEditable = false
    text.minSize = NSSize(width: 0, height: frame.height)
    text.maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
    text.textContainer?.containerSize = NSSize(width: frame.width, height: CGFloat.greatestFiniteMagnitude)
    text.textContainer?.widthTracksTextView = true
    scroll.documentView = text; window.contentView!.addSubview(scroll)
    windows.append(window); views.append(text)
}
let code = (0..<240).map { "\($0): fn render_frame() { latest_frame(); } // Tiếng Việt" }.joined(separator: "\n")
views[0].string = code; views[1].string = "SECOND WINDOW\n" + code
windows[0].makeKeyAndOrderFront(nil); app.activate(ignoringOtherApps: true)
let start = ProcessInfo.processInfo.systemUptime
var previous = -1, tick = 0, lastAudit = -1, active = 0, updates = 0
var nextUpdate = 0.0
var rows: [[String: Any]] = []
let timer = Timer.scheduledTimer(withTimeInterval: 0.015, repeats: true) { _ in
    let elapsed = ProcessInfo.processInfo.systemUptime - start
    if elapsed >= duration {
        if let audit = audit, let data = try? JSONSerialization.data(withJSONObject: rows, options: [.prettyPrinted]) { try? data.write(to: URL(fileURLWithPath: audit)) }
        app.terminate(nil); return
    }
    guard elapsed >= nextUpdate else { return }
    let phase = Int(elapsed / 10) % 7
    tick = Int(elapsed / 0.05) + 1
    nextUpdate = Double(tick) * 0.05
    updates += 1
    if phase != previous {
        active = 0; windows[0].setFrame(frame, display: true)
        windows[0].makeKeyAndOrderFront(nil); app.activate(ignoringOtherApps: true)
        views[0].string = phase == 2 ? code : "STATIC: readable code / terminal / Tiếng Việt\n" + String(repeating: "fn main() { latest_frame(); }\n", count: 25)
        if phase == 2 { views[0].setFrameSize(NSSize(width: frame.width, height: 6000)) }
        views[0].backgroundColor = .black; previous = phase
    }
    switch phase {
    case 1:
        views[0].string += "let frame_\(tick) = latest_frame();\n"; views[0].scrollToEndOfDocument(nil)
    case 2:
        if let scroll = views[0].enclosingScrollView {
            let range = max(1, Int(views[0].bounds.height - scroll.contentView.bounds.height))
            scroll.contentView.scroll(to: NSPoint(x: 0, y: CGFloat(tick * 24 % range)))
            scroll.reflectScrolledClipView(scroll.contentView)
        }
    case 3:
        views[0].string += String(format: "[%.3f] INFO bounded latest-frame tick=%d\n", elapsed, tick); views[0].scrollToEndOfDocument(nil)
    case 4:
        views[0].backgroundColor = NSColor(calibratedRed: Double(tick*17%256)/255, green: Double(tick*31%256)/255, blue: Double(tick*7%256)/255, alpha: 1)
        views[0].string = "MOTION STRESS \(tick)\n" + String(repeating: "Moving color and text ", count: 150)
    case 5:
        windows[0].setFrameOrigin(NSPoint(x: usable.minX + 40 + Double(tick % 20) * 2, y: usable.minY + 40 + Double(tick % 16) * 2))
    case 6:
        if tick % 20 == 0 { active = 1 - active; windows[active].makeKeyAndOrderFront(nil) }
    default: break
    }
    if Int(elapsed) != lastAudit {
        lastAudit = Int(elapsed)
        let foreground = app.isActive && windows[active].isKeyWindow
        rows.append(["unix_ms": Int64(Date().timeIntervalSince1970 * 1000), "elapsed_s": elapsed,
                     "phase": phase, "tick": tick, "updates": updates,
                     "scroll_y": views[0].enclosingScrollView?.contentView.bounds.origin.y ?? 0,
                     "foreground": foreground, "visible": windows[active].isVisible,
                     "screen_width": display.frame.width, "screen_height": display.frame.height,
                     "backing_scale": display.backingScaleFactor, "active_window": active])
        // Record interference before correcting foreground for the next sample.
        if !foreground { windows[active].makeKeyAndOrderFront(nil); app.activate(ignoringOtherApps: true) }
    }
}
app.run()
timer.invalidate()
