import AppKit
import AtlasKit
import SwiftUI

/// What to do next, as the onboarding's footer: where things stand on the
/// left, the next step in white on the right.
struct Footer: View {
    let model: ViewerModel

    var body: some View {
        VStack(spacing: 0) {
            Rule()
            HStack(spacing: 10) {
                Text(model.hint)
                    .font(.system(size: 12))
                    .foregroundStyle(AtlasStyle.ink3)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .accessibilityIdentifier("hint")
                Spacer(minLength: 12)
                if model.recoveryDeferred {
                    Button("Recover…") { model.recoverNow() }.atlas(.glass)
                }
                Button {
                    model.togglePause()
                } label: {
                    Image(systemName: model.paused ? "play.fill" : "pause.fill")
                }
                .buttonStyle(IconButtonStyle())
                .keyboardShortcut("p", modifiers: [.command, .shift])
                .help(model.paused ? "Resume Picture (⇧⌘P)" : "Pause Picture (⇧⌘P)")
                .accessibilityLabel(model.paused ? "Resume picture" : "Pause picture")
                KeysButton(model: model)
                // While in control the desktop's key monitor answers ⌃⇧⎋
                // first and swallows it; this shortcut takes over only when
                // the desktop does not have the keyboard, or while watching.
                Button {
                    if model.holdingControl {
                        model.releaseControl()
                    } else {
                        Task { await model.takeControl() }
                    }
                } label: {
                    Text(title)
                }
                .atlas()
                .keyboardShortcut(.escape, modifiers: [.control, .shift])
                .disabled(!model.holdingControl && !model.canTakeControl)
                .help(
                    model.holdingControl ? "Give control back (⌃⇧⎋)" : "Use your mouse and keyboard in the helper (⌃⇧⎋)"
                )
            }
            .padding(.horizontal, 22)
            .frame(height: 62)
        }
    }

    private var title: String {
        if model.holdingControl { return "Give Back" }
        return model.acquiring ? "Connecting…" : "Take Control"
    }
}

/// Keys the Mac keeps for itself (⌘⇥, ⌘Space…), sent to the helper from
/// a menu, which leaves the window in front and control where it is.
struct KeysButton: View {
    let model: ViewerModel

    /// One key to offer: its name, the key equivalent shown beside it with
    /// its modifiers, and the key sent to the helper.
    struct Key {
        let name: String
        let shown: String
        let modifiers: NSEvent.ModifierFlags
        let sent: String
    }

    static let groups: [[Key]] = [
        [
            Key(name: "Escape", shown: "\u{1b}", modifiers: [], sent: "escape"),
            Key(name: "Tab", shown: "\t", modifiers: [], sent: "tab"),
            Key(name: "Return", shown: "\r", modifiers: [], sent: "return"),
        ],
        [
            Key(name: "Copy", shown: "c", modifiers: .command, sent: "cmd+c"),
            Key(name: "Paste", shown: "v", modifiers: .command, sent: "cmd+v"),
            Key(name: "Select All", shown: "a", modifiers: .command, sent: "cmd+a"),
            Key(name: "Undo", shown: "z", modifiers: .command, sent: "cmd+z"),
            Key(name: "Redo", shown: "z", modifiers: [.command, .shift], sent: "cmd+shift+z"),
            Key(name: "Save", shown: "s", modifiers: .command, sent: "cmd+s"),
        ],
        [
            Key(name: "Switch App", shown: "\t", modifiers: .command, sent: "cmd+tab"),
            Key(name: "Spotlight", shown: " ", modifiers: .command, sent: "cmd+space"),
            Key(name: "Close Window", shown: "w", modifiers: .command, sent: "cmd+w"),
            Key(name: "Quit App", shown: "q", modifiers: .command, sent: "cmd+q"),
        ],
    ]

    var body: some View {
        Button {
            Self.menu(for: model).popUp(positioning: nil, at: NSEvent.mouseLocation, in: nil)
        } label: {
            Label("Keys", systemImage: "keyboard")
        }
        .atlas(.glass)
        .disabled(!model.holdingControl)
        .help("Send a key the Mac would keep for itself, such as ⌘⇥")
    }

    /// The menu of keys, each item sending its key to `model`.
    static func menu(for model: ViewerModel) -> NSMenu {
        let menu = NSMenu()
        menu.appearance = NSAppearance(named: .darkAqua)
        for (index, group) in groups.enumerated() {
            if index > 0 { menu.addItem(.separator()) }
            for key in group {
                let item = KeyItem(title: key.name) { [weak model] in model?.sendKey(key.sent) }
                item.keyEquivalent = key.shown
                item.keyEquivalentModifierMask = key.modifiers
                menu.addItem(item)
            }
        }
        return menu
    }
}

/// A menu item that runs a closure (NSMenuItem itself is not main-actor-bound).
@MainActor
final class KeyItem: NSMenuItem {
    private let handler: @MainActor () -> Void

    init(title: String, handler: @escaping @MainActor () -> Void) {
        self.handler = handler
        super.init(title: title, action: #selector(fire), keyEquivalent: "")
        target = self
    }

    required init(coder: NSCoder) { fatalError("not used") }

    @objc func fire() { handler() }
}
