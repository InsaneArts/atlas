import AppKit
import AtlasKit
import SwiftUI

/// The popover's content: status, desktop, facts and actions.
struct PanelView: View {
    static let width: CGFloat = 320

    let model: PanelModel

    init(model: PanelModel) {
        self.model = model
    }

    var body: some View {
        let mode = model.mode
        VStack(spacing: 12) {
            header(mode)
            // Logged out, there is no desktop to show: logging in takes its place.
            if model.loggedOut {
                LogInCard { Task { await model.logIn() } }
                    .disabled(model.working)
            } else {
                Desktop(image: model.image, placeholder: model.placeholder)
            }
            if model.recovering {
                RecoveryCard(reason: model.status.overview.reason) { Task { await model.recover() } }
            }
            facts
            HStack(spacing: 8) {
                Button("Open Viewer") { model.openViewer?() }.atlas(model.loggedOut ? .glass : .primary, fill: true)
                    .keyboardShortcut(.defaultAction)
                if model.controlling {
                    Button("Release Control") { Task { await model.releaseControl() } }.atlas(.glass, fill: true)
                } else if !model.loggedOut {
                    Button("Switch to Helper…") { Task { await model.switchToHelper() } }.atlas(.glass, fill: true)
                        .disabled(model.helper == nil || model.working)
                }
            }
            if model.confirmingLogOut {
                ConfirmLogOut(
                    cancel: model.cancelLogOut,
                    logOut: { Task { await model.logOut() } }
                )
                .disabled(model.working || !model.logOutAllowed)
            }
            if let message = model.message {
                HStack(spacing: 8) {
                    if model.working { Spinner(size: 12) }
                    Text(message).font(.system(size: 11.5)).foregroundStyle(AtlasStyle.ink2)
                        .fixedSize(horizontal: false, vertical: true)
                    Spacer(minLength: 0)
                }
                .accessibilityElement(children: .combine)
            }
            if model.forceOffered {
                Button("Log Out Anyway") { Task { await model.logOut(force: true) } }.atlas(.glass, fill: true)
                    .disabled(model.working || !model.forceAllowed)
            }
            Rule()
            VStack(spacing: 1) {
                if model.loginAvailable {
                    HStack {
                        Text("Open at Login").font(.system(size: 13))
                        Spacer()
                        Toggle("Open at Login", isOn: Binding(get: { model.atLogin }, set: { model.setLogin($0) }))
                            .toggleStyle(.switch).controlSize(.mini).labelsHidden().tint(AtlasStyle.blue)
                    }
                    .padding(.horizontal, 8)
                    .frame(height: 30)
                }
                MenuRow(title: "Getting Started…") { model.gettingStarted?() }
                if !model.recovering && !model.loggedOut {
                    MenuRow(title: "Recover") { Task { await model.recover() } }
                        .disabled(model.working)
                }
                if !model.loggedOut {
                    MenuRow(title: "Log Out Helper…", action: model.askToLogOut)
                        .disabled(model.working || !model.logOutAllowed || model.confirmingLogOut)
                }
                MenuRow(title: "Quit Atlas", detail: "⌘Q") { NSApp.terminate(nil) }
                    .keyboardShortcut("q", modifiers: .command)
            }
            .padding(.horizontal, -6)
        }
        .padding(14)
        .frame(width: Self.width)
        .background(AtlasStyle.ground)
        .foregroundStyle(AtlasStyle.ink)
        .environment(\.colorScheme, .dark)
    }

    private func header(_ mode: Mode) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 10) {
                OrbitMark(size: 24)
                Text("Atlas").font(.system(size: 14, weight: .semibold))
                Spacer()
                HStack(spacing: 6) {
                    Circle().fill(mode.tone.color).frame(width: 7, height: 7)
                    Text(mode.label).font(.system(size: 11.5, weight: .medium))
                }
                .padding(.horizontal, 10)
                .frame(height: 24)
                .background(Capsule().fill(Color.white.opacity(0.06)))
                .accessibilityElement(children: .combine)
            }
            let reason = model.status.overview.reason
            if !reason.isEmpty && !model.recovering && !model.loggedOut {
                Text(reason).font(.system(size: 11.5)).foregroundStyle(AtlasStyle.ink2)
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    private var facts: some View {
        let overview = model.status.overview
        return Card {
            FactRow(name: "Helper", value: overview.accountName ?? overview.account ?? "—")
            Rule()
            FactRow(name: "Display", value: Words.size(overview.display) ?? "—")
            Rule()
            FactRow(name: "Last action", value: model.overview.lastAction)
        }
    }
}

/// The helper's desktop, letterboxed on a dark rounded stage.
struct Desktop: View {
    let image: NSImage?
    let placeholder: String

    var body: some View {
        ZStack(alignment: .bottomLeading) {
            RoundedRectangle(cornerRadius: 10).fill(Color.black)
            if let image {
                Image(nsImage: image).resizable().interpolation(.high).aspectRatio(contentMode: .fit)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                HStack(spacing: 5) {
                    Circle().fill(AtlasStyle.green).frame(width: 5, height: 5)
                    Text("Live").font(.system(size: 10, weight: .semibold))
                }
                .padding(.horizontal, 8)
                .frame(height: 20)
                .background(Capsule().fill(Color.black.opacity(0.55)))
                .padding(8)
            } else {
                Text(placeholder).font(.system(size: 11.5)).foregroundStyle(AtlasStyle.ink3)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .padding(16)
            }
        }
        .aspectRatio(16 / 10, contentMode: .fit)
        .clipShape(RoundedRectangle(cornerRadius: 10))
        .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(Color.white.opacity(0.14), lineWidth: 0.5))
        .accessibilityElement()
        .accessibilityLabel(image == nil ? placeholder : "The helper's screen")
    }
}

/// Shown when the last action's outcome is unknown: recover first.
struct RecoveryCard: View {
    let reason: String
    let recover: () -> Void

    var body: some View {
        HStack(spacing: 10) {
            CardText(
                title: "The last action didn't finish",
                text: reason.isEmpty ? "Recover before anything else runs." : reason)
            Spacer(minLength: 6)
            Button("Recover", action: recover).atlas()
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .cardStyle(AtlasStyle.orange)
    }
}

/// Shown while the helper is logged out, in place of its desktop: logging
/// in to it is the way back.
struct LogInCard: View {
    let logIn: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            CardText(title: "The helper is logged out", text: Words.loggedOut)
            Button("Log In…", action: logIn).atlas(fill: true)
        }
        .padding(12)
        .cardStyle()
    }
}

/// Asks before logging the helper out: only the person can log it back in.
struct ConfirmLogOut: View {
    let cancel: () -> Void
    let logOut: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            CardText(
                title: "Log out the helper?",
                text: "Its apps quit, and agents can't use it until you log in to it again.")
            HStack(spacing: 8) {
                Button("Cancel", action: cancel).atlas(.glass, fill: true)
                    .keyboardShortcut(.cancelAction)
                Button("Log Out", action: logOut).atlas(fill: true)
            }
        }
        .padding(12)
        .cardStyle()
    }
}

/// A card's title and what it means, as the panel's cards set them.
struct CardText: View {
    let title: String
    let text: String

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title).font(.system(size: 12.5, weight: .semibold))
            Text(text).font(.system(size: 11.5)).foregroundStyle(AtlasStyle.ink2)
                .fixedSize(horizontal: false, vertical: true)
        }
    }
}

/// A label and its value, on one line.
struct FactRow: View {
    let name: String
    let value: String

    var body: some View {
        HStack(spacing: 12) {
            Text(name).foregroundStyle(AtlasStyle.ink2)
            Spacer(minLength: 8)
            Text(value).lineLimit(1).truncationMode(.tail).help(value)
        }
        .font(.system(size: 12))
        .padding(.horizontal, 12)
        .frame(height: 32)
        .accessibilityElement(children: .combine)
    }
}

/// A row like a menu item: plain text, highlighted under the pointer.
struct MenuRow: View {
    let title: String
    var detail = ""
    let action: () -> Void
    @State private var hovering = false
    @Environment(\.isEnabled) private var enabled

    var body: some View {
        Button(action: action) {
            HStack {
                Text(title).font(.system(size: 13))
                Spacer()
                Text(detail).font(.system(size: 13)).foregroundStyle(AtlasStyle.ink3)
            }
            .padding(.horizontal, 8)
            .frame(height: 28)
            .background(
                RoundedRectangle(cornerRadius: 7).fill(hovering && enabled ? Color.white.opacity(0.08) : .clear)
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .opacity(enabled ? 1 : 0.4)
        .onHover { hovering = $0 }
    }
}
