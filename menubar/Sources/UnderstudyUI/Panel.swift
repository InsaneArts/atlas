import AppKit
import UnderstudyKit

/// The popover's content: status, desktop, facts and actions. Every action
/// is an `understudy` command (or the viewer's own control endpoint).
public final class PanelController: NSViewController {
    let viewer: Viewer?
    let commands: Commands?
    let loginItem: LoginItem?
    /// The helper's short name, once `understudy switch --list` reports it.
    public var helper: String? { didSet { switchButton.isEnabled = helper != nil } }

    let dot = DotView()
    let state = label("", size: 12, color: .secondaryLabelColor)
    public let thumbnail = ThumbnailView()
    let account = label("")
    let display = label("")
    let last = label("")
    let when = label("")
    let message = label("", size: 11, color: .secondaryLabelColor)
    let open = NSButton(title: "Open Viewer", target: nil, action: nil)
    let release = NSButton(title: "Release Control", target: nil, action: nil)
    lazy var switchButton = menuButton("Switch to Helper…", target: self, action: #selector(switchToHelper))
    lazy var recover = menuButton("Recover", target: self, action: #selector(recoverHelper))
    lazy var atLogin = NSButton(
        checkboxWithTitle: "Open at Login", target: self, action: #selector(toggleLogin))

    public init(viewer: Viewer?, commands: Commands?, loginItem: LoginItem?) {
        self.viewer = viewer
        self.commands = commands
        self.loginItem = loginItem
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) { nil }

    public override func loadView() {
        let title = label("Understudy")
        title.font = .systemFont(ofSize: 13, weight: .semibold)
        let status = NSStackView(views: [dot, state])
        status.spacing = 6
        let header = NSStackView(views: [title, NSView(), status])
        header.edgeInsets = NSEdgeInsets(top: 0, left: 2, bottom: 0, right: 2)

        let separator = NSBox()
        separator.boxType = .separator
        let facts = NSStackView(views: [row(account, display), separator, row(last, when)])
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
            separator.leadingAnchor.constraint(equalTo: facts.leadingAnchor, constant: 12),
            separator.trailingAnchor.constraint(equalTo: facts.trailingAnchor, constant: -12),
        ])

        for button in [open, release] {
            button.bezelStyle = .push
            button.controlSize = .large
            button.target = self
        }
        open.keyEquivalent = "\r"
        open.action = #selector(openViewer)
        release.action = #selector(releaseControl)
        let actions = NSStackView(views: [open, release])
        actions.orientation = .vertical
        actions.spacing = 6

        switchButton.isEnabled = false
        let more = NSStackView(views: [switchButton, recover])
        more.orientation = .vertical
        more.alignment = .leading
        more.spacing = 4
        more.edgeInsets = NSEdgeInsets(top: 0, left: 8, bottom: 0, right: 8)

        let divider = NSBox()
        divider.boxType = .separator
        atLogin.isHidden = loginItem?.available != true
        atLogin.state = loginItem?.enabled == true ? .on : .off
        let loginRow = NSStackView(views: [atLogin])
        loginRow.edgeInsets = NSEdgeInsets(top: 0, left: 8, bottom: 0, right: 8)
        let quit = menuButton(
            "Quit Understudy", target: NSApp, action: #selector(NSApplication.terminate(_:)))
        quit.keyEquivalent = "q"
        quit.keyEquivalentModifierMask = .command
        let shortcut = label("⌘Q", color: .secondaryLabelColor)
        let quitRow = NSStackView(views: [quit, NSView(), shortcut])
        quitRow.edgeInsets = NSEdgeInsets(top: 0, left: 8, bottom: 0, right: 8)

        message.isHidden = true
        message.maximumNumberOfLines = 3
        let stack = NSStackView(views: [
            header, thumbnail, group, actions, more, message, divider, loginRow, quitRow,
        ])
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 10
        stack.edgeInsets = NSEdgeInsets(top: 12, left: 12, bottom: 10, right: 12)
        for view in [header, thumbnail, group, actions, more, divider, loginRow, quitRow, open, release]
        {
            view.translatesAutoresizingMaskIntoConstraints = false
            view.widthAnchor.constraint(equalToConstant: 316).isActive = true
        }
        message.preferredMaxLayoutWidth = 316
        thumbnail.heightAnchor.constraint(equalTo: thumbnail.widthAnchor, multiplier: 9.0 / 16.0)
            .isActive = true
        stack.widthAnchor.constraint(equalToConstant: 340).isActive = true
        view = stack
        show(Snapshot())
    }

    public func show(_ snapshot: Snapshot) {
        let mode = Mode.of(snapshot.mode)
        dot.color = mode.tone.color
        state.stringValue = mode.label
        account.stringValue = snapshot.account
        display.stringValue = snapshot.display
        last.stringValue = snapshot.last
        when.stringValue = snapshot.when
        release.isEnabled = snapshot.controlling
        recover.isEnabled = commands != nil
    }

    func say(_ text: String) {
        message.stringValue = text
        message.isHidden = false
    }

    func perform(_ action: Action, working: String, done: String) {
        guard let commands else { return }
        say(working)
        commands.run(action.arguments) { [weak self] outcome in
            self?.say(outcome.ok ? done : outcome.message)
        }
    }

    @objc func openViewer() {
        if let url = viewer?.base { NSWorkspace.shared.open(url) }
    }

    @objc func releaseControl() {
        release.isEnabled = false
        viewer?.revoke { [weak self] failure in
            self?.say(failure ?? "Control released. Nothing was resumed.")
        }
    }

    @objc func switchToHelper() {
        guard let helper else { return }
        perform(.switchToHelper(helper), working: "Switching…", done: "Switched")
    }

    @objc func recoverHelper() {
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
