import AppKit
import AtlasKit
import SwiftUI

/// A stand-in desktop for drawing the viewer: a window on a blue ground.
@MainActor
func sampleDesktop(_ size: NSSize = NSSize(width: 1512, height: 982)) -> NSImage {
    NSImage(size: size, flipped: true) { rect in
        NSGradient(
            starting: NSColor(srgbRed: 0.49, green: 0.59, blue: 0.72, alpha: 1),
            ending: NSColor(srgbRed: 0.2, green: 0.31, blue: 0.44, alpha: 1))?
            .draw(in: rect, angle: 90)
        NSColor.white.withAlphaComponent(0.38).setFill()
        rect.divided(atDistance: rect.height * 0.032, from: .minYEdge).slice.fill()
        let window = NSRect(
            x: rect.width * 0.16, y: rect.height * 0.12, width: rect.width * 0.56, height: rect.height * 0.7)
        NSColor.white.setFill()
        NSBezierPath(roundedRect: window, xRadius: 10, yRadius: 10).fill()
        NSColor(white: 0.965, alpha: 1).setFill()
        NSRect(x: window.minX, y: window.minY + 10, width: window.width, height: window.height * 0.07 - 10).fill()
        for (index, color) in [NSColor.systemRed, .systemYellow, .systemGreen].enumerated() {
            color.setFill()
            NSBezierPath(
                ovalIn: NSRect(x: window.minX + 14 + CGFloat(index) * 18, y: window.minY + 14, width: 11, height: 11)
            )
            .fill()
        }
        let text: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: 22), .foregroundColor: NSColor.black,
        ]
        ("Atlas 0.1.0\n\nAgents test apps in a hidden helper login while you keep working." as NSString)
            .draw(in: window.insetBy(dx: 40, dy: 90), withAttributes: text)
        return true
    }
}

/// Every viewer state, by file name, for checking against the design.
@MainActor
func viewerScreens() -> [(name: String, model: ViewerModel)] {
    func status(_ mode: String, reason: String = "An agent is acting in the helper", ok: Bool = true) -> ViewerStatus {
        let reply: [String: Any] = [
            "ready": true, "canObserve": true,
            "checks": [["name": "Helper session", "ok": true], ["name": "Input service", "ok": ok]],
            "session": ["user": ["name": "Atlas"], "inputService": ["ok": true, "ready": true]],
            "capabilities": ["nativeDialogs": ["state": "unknown", "reason": "Run atlas verify-native to check."]],
            "overview": [
                "mode": mode, "reason": reason, "account": "atlas", "accountName": "Atlas",
                "display": ["width": 1512, "height": 982], "pending": mode == "recovering" ? 1 : 0,
                "activity": [
                    [
                        "operation": "input", "actor": "agent", "ok": mode != "recovering", "actionCount": 2,
                        "ageSeconds": 2,
                    ],
                    ["operation": "run", "actor": "agent", "ok": true, "ageSeconds": 7],
                    ["operation": "input", "actor": "viewer", "ok": true, "actionCount": 5, "ageSeconds": 240],
                ],
            ],
        ]
        let data = (try? JSONSerialization.data(withJSONObject: reply)) ?? Data()
        return (try? JSONDecoder().decode(ViewerStatus.self, from: data)) ?? ViewerStatus()
    }
    let desktop = sampleDesktop()
    return [
        ("01-watching", .still(status("agent"), image: desktop)),
        (
            "02-sidebar",
            .still(status("observing", reason: "Watching the helper"), image: desktop, sidebar: true)
        ),
        (
            "03-sidebar-control",
            .still(
                status("controlling", reason: "You have control"), image: desktop, lease: "lease", sidebar: true)
        ),
        ("04-control", .still(status("controlling", reason: "A viewer has control"), image: desktop, lease: "lease")),
        (
            "05-recovery",
            .still(
                status("recovering", reason: "Atlas can't tell whether the last action worked.", ok: false),
                image: desktop)
        ),
        ("06-paused", .still(status("observing"), image: desktop, paused: true)),
        (
            "07-unavailable",
            .still(
                status("unavailable", reason: "The helper is signed out"), image: nil,
                unavailable: "Sign in to Atlas, then switch back.")
        ),
        ("08-notice", .still(status("observing"), image: desktop, notice: "Control released.")),
    ]
}

/// Draw every viewer state to PNGs in `directory` at Retina scale.
@MainActor
public func renderViewer(to directory: String, size: NSSize = NSSize(width: 1280, height: 820)) -> Bool {
    let folder = URL(fileURLWithPath: directory)
    return viewerScreens().map { screen in
        draw(ViewerView(model: screen.model), size: size, to: folder.appendingPathComponent("\(screen.name).png"))
    }.allSatisfy { $0 }
}
