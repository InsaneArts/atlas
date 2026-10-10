import AppKit
import UnderstudyKit

/// The popover's content: status, desktop, facts and actions. Every action
/// is an `understudy` command (or the viewer's own control endpoint).
public final class PanelController: NSViewController {
    let viewer: Viewer?
    let commands: Commands?
    let loginItem: LoginItem?
    /// The helper's short name, once `understudy switch --list` reports it.
    public var helper: String? {
        didSet {
            switchButton.isEnabled = helper != nil
            switchButton.detail = helper ?? ""
            if isViewLoaded { fit() }
        }
    }
    /// Render-only: show every action as it looks when it is available.
    var previewing = false

    static let width: CGFloat = 340
    static let inset: CGFloat = 14
    /// The rows reach closer to the edge than the text, as in system menus.
    static let edge: CGFloat = 6
    static var content: CGFloat { width - 2 * inset }

    let dot = DotView()
    let state = label("", size: 12, weight: .medium, color: .secondaryLabelColor)
    let reason = label("", size: 11, color: .secondaryLabelColor)
    public let thumbnail = ThumbnailView()
    let account = label("")
    let display = label("")
    let last = label("")
    let message = label("", size: 11, color: .secondaryLabelColor)
    let spinner = NSProgressIndicator()
    lazy var feedback = NSStackView(views: [spinner, message])
    let open = RowButton("Open Viewer", symbol: "eye")
    let release = RowButton("Release Control", symbol: "hand.raised")
    let switchButton = RowButton("Switch to Helper…", symbol: "person.2")
    let recover = RowButton("Recover", symbol: "arrow.clockwise")
    let atLogin = NSSwitch()
    lazy var loginRow = NSStackView(views: [label("Open at Login"), NSView(), atLogin])
    let quit = RowButton("Quit Understudy", detail: "⌘Q")

    public init(viewer: Viewer?, commands: Commands?, loginItem: LoginItem?) {
        self.viewer = viewer
        self.commands = commands
        self.loginItem = loginItem
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) { nil }

    public override func loadView() {
        let width = Self.content
        // Header: the app, its state in a word and a dot, and why.
        let title = label("Understudy", weight: .semibold)
        let status = NSStackView(views: [dot, state])
        status.spacing = 5
        let top = NSStackView(views: [title, NSView(), status])
        reason.maximumNumberOfLines = 2
        reason.lineBreakMode = .byWordWrapping
        reason.preferredMaxLayoutWidth = width
        let header = NSStackView(views: [top, reason])
        header.orientation = .vertical
        header.alignment = .leading
        header.spacing = 2
        top.widthAnchor.constraint(equalTo: header.widthAnchor).isActive = true

        // Facts: a System Settings style inset group.
        let rows: [NSView] = [
            infoRow("Account", account), Hairline(), infoRow("Display", display), Hairline(),
            infoRow("Last action", last),
        ]
        let facts = NSStackView(views: rows)
        facts.orientation = .vertical
        facts.spacing = 0
        let group = GroupView()
        group.addSubview(facts)
        facts.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            facts.leadingAnchor.constraint(equalTo: group.leadingAnchor),
            facts.trailingAnchor.constraint(equalTo: group.trailingAnchor),
            facts.topAnchor.constraint(equalTo: group.topAnchor),
            facts.bottomAnchor.constraint(equalTo: group.bottomAnchor),
        ])
        for view in rows {
            if view is Hairline {
                view.leadingAnchor.constraint(equalTo: facts.leadingAnchor, constant: 12).isActive =
                    true
                view.trailingAnchor.constraint(equalTo: facts.trailingAnchor, constant: -12)
                    .isActive = true
            } else {
                view.widthAnchor.constraint(equalTo: facts.widthAnchor).isActive = true
            }
        }

        // Actions: Control Center style rows, the viewer first.
        open.press = { [weak self] in self?.openViewer() }
        open.isEmphasized = true
        open.keyEquivalent = "\r"
        release.press = { [weak self] in self?.releaseControl() }
        release.isEmphasized = true
        release.isHidden = true
        switchButton.press = { [weak self] in self?.switchToHelper() }
        // The helper may be known before the panel is first drawn.
        switchButton.isEnabled = helper != nil
        switchButton.detail = helper ?? ""
        recover.press = { [weak self] in self?.recoverHelper() }
        let actions = NSStackView(views: [open, release, switchButton, recover])
        actions.orientation = .vertical
        actions.spacing = 0

        // What the last action said, only while there is something to say.
        spinner.style = .spinning
        spinner.controlSize = .small
        spinner.isDisplayedWhenStopped = false
        spinner.isHidden = true
        message.maximumNumberOfLines = 3
        message.lineBreakMode = .byWordWrapping
        message.preferredMaxLayoutWidth = width - 22
        feedback.spacing = 6
        feedback.alignment = .centerY
        feedback.isHidden = true

        // Footer: the login switch and Quit, as in the system's own menus.
        atLogin.controlSize = .mini
        atLogin.target = self
        atLogin.action = #selector(toggleLogin)
        atLogin.state = loginItem?.enabled == true ? .on : .off
        loginRow.edgeInsets = NSEdgeInsets(top: 0, left: 8, bottom: 0, right: 10)
        loginRow.heightAnchor.constraint(equalToConstant: 26).isActive = true
        loginRow.isHidden = loginItem?.available != true
        quit.keyEquivalent = "q"
        quit.keyEquivalentModifierMask = .command
        quit.press = { NSApp.terminate(nil) }

        let divider = Hairline()
        let stack = NSStackView(views: [
            header, thumbnail, group, actions, feedback, divider, loginRow, quit,
        ])
        stack.orientation = .vertical
        stack.alignment = .centerX
        stack.spacing = 0
        stack.edgeInsets = NSEdgeInsets(
            top: Self.inset, left: Self.edge, bottom: Self.edge, right: Self.edge)
        stack.setCustomSpacing(12, after: header)
        stack.setCustomSpacing(10, after: thumbnail)
        stack.setCustomSpacing(8, after: group)
        stack.setCustomSpacing(6, after: actions)
        stack.setCustomSpacing(8, after: feedback)
        stack.setCustomSpacing(6, after: divider)
        for view in [header, thumbnail, group, feedback, divider] {
            view.translatesAutoresizingMaskIntoConstraints = false
            view.widthAnchor.constraint(equalToConstant: width).isActive = true
        }
        for view in [actions, loginRow, quit] {
            view.translatesAutoresizingMaskIntoConstraints = false
            view.widthAnchor.constraint(equalToConstant: width + 2 * (Self.inset - Self.edge)).isActive =
                true
        }
        for view in [open, release, switchButton, recover] {
            view.widthAnchor.constraint(equalTo: actions.widthAnchor).isActive = true
        }
        thumbnail.heightAnchor.constraint(equalTo: thumbnail.widthAnchor, multiplier: 9.0 / 16.0)
            .isActive = true
        stack.widthAnchor.constraint(equalToConstant: Self.width).isActive = true
        view = stack
        show(Snapshot())
    }

    public func show(_ snapshot: Snapshot) {
        let mode = Mode.of(snapshot.mode)
        dot.color = mode.tone.color
        state.stringValue = mode.label
        reason.stringValue = snapshot.reason
        reason.isHidden = snapshot.reason.isEmpty
        account.stringValue = snapshot.account
        display.stringValue = snapshot.display.isEmpty ? "—" : snapshot.display
        last.stringValue = [snapshot.last, snapshot.when].filter { !$0.isEmpty }
            .joined(separator: " · ")
        last.toolTip = last.stringValue
        // Releasing is offered only while the viewer holds control.
        release.isHidden = !snapshot.controlling
        release.isEnabled = snapshot.controlling
        recover.isEnabled = commands != nil || previewing
        recover.isEmphasized = snapshot.mode == "recovering"
        fit()
    }

    /// The popover follows the content as rows and messages come and go, so
    /// nothing is squeezed to make room.
    func fit() {
        view.layoutSubtreeIfNeeded()
        let size = view.fittingSize
        if preferredContentSize != size { preferredContentSize = size }
    }

    func say(_ text: String, working: Bool = false) {
        message.stringValue = text
        spinner.isHidden = !working
        if working { spinner.startAnimation(nil) } else { spinner.stopAnimation(nil) }
        feedback.isHidden = false
        fit()
    }

    /// Render-only: every action available, as on a set-up Mac.
    func preview() {
        previewing = true
        helper = helper ?? "helper"
        loginRow.isHidden = false
        recover.isEnabled = true
        fit()
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
        release.isEnabled = false
        viewer?.revoke { [weak self] failure in
            self?.say(failure ?? "Control released.")
        }
    }

    func switchToHelper() {
        guard let helper else { return }
        perform(.switchToHelper(helper), working: "Switching…", done: "Switched")
    }

    func recoverHelper() {
        perform(.recover, working: "Recovering the helper…", done: "The helper is ready again")
    }

    @objc func toggleLogin() {
        do {
            try loginItem?.set(atLogin.state == .on)
        } catch {
            atLogin.state = loginItem?.enabled == true ? .on : .off
            say(error.localizedDescription)
        }
    }
}
