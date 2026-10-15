import AppKit
import AtlasKit
import Combine
import SwiftUI

/// What the panel shows: the latest status, the helper's desktop, and what
/// the last action said.
@MainActor
public final class PanelModel: ObservableObject {
    @Published public var snapshot = Snapshot()
    @Published public var helper: String?
    @Published public var image: NSImage?
    @Published public var placeholder = "Waiting for the helper’s screen…"
    @Published var message: String?
    @Published var working = false
    @Published var atLogin = false
    @Published var loginAvailable = false

    public init() {}
}

/// Sets the panel's desktop picture (kept apart so callers need not know
/// the panel is drawn with SwiftUI).
@MainActor
public final class PanelThumbnail {
    let model: PanelModel
    init(model: PanelModel) { self.model = model }
    public var image: NSImage? {
        get { model.image }
        set { model.image = newValue }
    }
    public var placeholder: String {
        get { model.placeholder }
        set { model.placeholder = newValue }
    }
}

/// The popover's content: status, desktop, facts and actions. Every action
/// is an `atlas` command (or the viewer's own control endpoint).
public final class PanelController: NSViewController {
    let viewer: Viewer?
    let commands: AtlasKit.Commands?
    let loginItem: LoginItem?
    let model: PanelModel
    public let thumbnail: PanelThumbnail
    /// The setup window this panel opened, while it runs.
    var onboarding: Process?
    private var sizing: AnyCancellable?

    /// The helper's short name, once `atlas switch --list` reports it.
    public var helper: String? {
        get { model.helper }
        set { model.helper = newValue }
    }

    static let width: CGFloat = 320

    public init(viewer: Viewer?, commands: AtlasKit.Commands?, loginItem: LoginItem?) {
        self.viewer = viewer
        self.commands = commands
        self.loginItem = loginItem
        let model = MainActor.assumeIsolated { PanelModel() }
        self.model = model
        thumbnail = MainActor.assumeIsolated { PanelThumbnail(model: model) }
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) { nil }

    public override func loadView() {
        model.atLogin = loginItem?.enabled == true
        model.loginAvailable = loginItem?.available == true
        let host = NSHostingView(rootView: PanelView(model: model, actions: self))
        host.sizingOptions = [.intrinsicContentSize]
        host.appearance = NSAppearance(named: .darkAqua)
        view = host
        // The popover follows the content as rows and messages come and go.
        sizing = model.objectWillChange
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in DispatchQueue.main.async { self?.fit() } }
        fit()
    }

    func fit() {
        view.layoutSubtreeIfNeeded()
        let size = view.fittingSize
        if preferredContentSize != size { preferredContentSize = size }
    }

    public func show(_ snapshot: Snapshot) {
        model.snapshot = snapshot
    }

    func say(_ text: String, working: Bool = false) {
        model.message = text
        model.working = working
    }

    /// Render-only: every action available, as on a set-up Mac.
    func preview() {
        model.helper = model.helper ?? "helper"
        model.loginAvailable = true
    }

    func perform(_ action: Action, working: String, done: String) {
        guard let commands else { return }
        say(working, working: true)
        commands.run(action.arguments) { [weak self] outcome in
            self?.say(outcome.ok ? done : outcome.message)
        }
    }

    func openViewer() {
        if let url = viewer?.base { NSWorkspace.shared.open(url) }
    }

    func releaseControl() {
        say("Releasing control…", working: true)
        viewer?.revoke { [weak self] failure in
            self?.say(failure ?? "Control released.")
        }
    }

    func switchToHelper() {
        guard let helper = model.helper else { return }
        perform(.switchToHelper(helper), working: "Switching…", done: "Switched")
    }

    func recoverHelper() {
        perform(.recover, working: "Recovering the helper…", done: "The helper is ready again")
    }

    /// The setup window, in its own process (it has a Dock icon while
    /// open); one at a time, brought forward when already open.
    func gettingStarted() {
        if let running = onboarding, running.isRunning {
            NSRunningApplication(processIdentifier: running.processIdentifier)?
                .activate(options: [.activateAllWindows])
            return
        }
        guard let executable = Bundle.main.executableURL else { return }
        let process = Process()
        process.executableURL = executable
        process.arguments = ["--onboarding"]
        do {
            try process.run()
            onboarding = process
        } catch {
            say(error.localizedDescription)
        }
    }

    func setLogin(_ on: Bool) {
        do {
            try loginItem?.set(on)
            model.atLogin = loginItem?.enabled == true
        } catch {
            model.atLogin = loginItem?.enabled == true
            say(error.localizedDescription)
        }
    }
}

struct PanelView: View {
    @ObservedObject var model: PanelModel
    let actions: PanelController

    var body: some View {
        let snapshot = model.snapshot
        let mode = Mode.of(snapshot.mode)
        VStack(spacing: 12) {
            header(mode, snapshot)
            Desktop(image: model.image, placeholder: model.placeholder)
            if snapshot.mode == "recovering" {
                RecoveryCard(reason: snapshot.reason) { actions.recoverHelper() }
            }
            facts(snapshot)
            HStack(spacing: 8) {
                Button("Open Viewer") { actions.openViewer() }
                    .buttonStyle(AtlasButtonStyle(emphasis: .primary, fill: true))
                    .keyboardShortcut(.defaultAction)
                if snapshot.controlling {
                    Button("Release Control") { actions.releaseControl() }
                        .buttonStyle(AtlasButtonStyle(emphasis: .glass, fill: true))
                } else {
                    Button("Switch to Helper…") { actions.switchToHelper() }
                        .buttonStyle(AtlasButtonStyle(emphasis: .glass, fill: true))
                        .disabled(model.helper == nil)
                }
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
            Rule()
            VStack(spacing: 1) {
                if model.loginAvailable {
                    HStack {
                        Text("Open at Login").font(.system(size: 13))
                        Spacer()
                        Toggle(
                            "Open at Login",
                            isOn: Binding(get: { model.atLogin }, set: { actions.setLogin($0) })
                        )
                        .toggleStyle(.switch).controlSize(.mini).labelsHidden().tint(AtlasStyle.blue)
                    }
                    .padding(.horizontal, 8)
                    .frame(height: 30)
                }
                MenuRow(title: "Getting Started…") { actions.gettingStarted() }
                if snapshot.mode != "recovering" {
                    MenuRow(title: "Recover") { actions.recoverHelper() }
                }
                MenuRow(title: "Quit Atlas", detail: "⌘Q") { NSApp.terminate(nil) }
                    .keyboardShortcut("q", modifiers: .command)
            }
            .padding(.horizontal, -6)
        }
        .padding(14)
        .frame(width: PanelController.width)
        .background(AtlasStyle.ground)
        .foregroundStyle(AtlasStyle.ink)
        .environment(\.colorScheme, .dark)
    }

    func header(_ mode: Mode, _ snapshot: Snapshot) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 10) {
                AppIcon(size: 30)
                Text("Atlas").font(.system(size: 14, weight: .semibold))
                Spacer()
                HStack(spacing: 6) {
                    Circle().fill(mode.tone.swatch).frame(width: 7, height: 7)
                    Text(mode.label).font(.system(size: 11.5, weight: .medium))
                }
                .padding(.horizontal, 10)
                .frame(height: 24)
                .background(Capsule().fill(Color.white.opacity(0.06)))
                .accessibilityElement(children: .combine)
            }
            if !snapshot.reason.isEmpty && snapshot.mode != "recovering" {
                Text(snapshot.reason).font(.system(size: 11.5)).foregroundStyle(AtlasStyle.ink2)
                    .lineLimit(2)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    func facts(_ snapshot: Snapshot) -> some View {
        let last = [snapshot.last, snapshot.when].filter { !$0.isEmpty }.joined(separator: " · ")
        return Card {
            FactRow(name: "Helper", value: snapshot.account.isEmpty ? "—" : snapshot.account)
            Rule()
            FactRow(name: "Display", value: snapshot.display.isEmpty ? "—" : snapshot.display)
            Rule()
            FactRow(name: "Last action", value: last.isEmpty ? "—" : last)
        }
    }
}

extension Mode.Tone {
    /// The dot's colour: green when ready, blue while working, orange when it
    /// needs the person, quiet grey otherwise.
    var swatch: Color {
        switch self {
        case .ready: return AtlasStyle.green
        case .active: return AtlasStyle.blue
        case .attention: return AtlasStyle.orange
        case .quiet: return AtlasStyle.ink3
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

struct RecoveryCard: View {
    let reason: String
    let recover: () -> Void
    var body: some View {
        HStack(spacing: 10) {
            VStack(alignment: .leading, spacing: 2) {
                Text("The last action didn't finish").font(.system(size: 12.5, weight: .semibold))
                Text(reason.isEmpty ? "Recover before anything else runs." : reason)
                    .font(.system(size: 11.5)).foregroundStyle(AtlasStyle.ink2)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 6)
            Button("Recover", action: recover).buttonStyle(AtlasButtonStyle(emphasis: .primary))
                .controlSize(.small)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .background(RoundedRectangle(cornerRadius: 12).fill(AtlasStyle.orange.opacity(0.10)))
        .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(AtlasStyle.orange.opacity(0.35), lineWidth: 0.5))
    }
}

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
    var body: some View {
        Button(action: action) {
            HStack {
                Text(title).font(.system(size: 13))
                Spacer()
                Text(detail).font(.system(size: 13)).foregroundStyle(AtlasStyle.ink3)
            }
            .padding(.horizontal, 8)
            .frame(height: 28)
            .background(RoundedRectangle(cornerRadius: 7).fill(hovering ? Color.white.opacity(0.08) : .clear))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
    }
}
