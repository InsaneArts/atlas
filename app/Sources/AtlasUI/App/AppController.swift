import AppKit
import AtlasKit

/// Atlas.app once it is set up: the menu bar item, and the setup window
/// when Getting Started… brings it back. The app has no Dock icon unless
/// a window is open.
@MainActor
public final class AppController: NSObject, NSApplicationDelegate {
    let commands: any CommandRunning
    let service: any ViewerService
    private var menuBar: MenuBarController?
    private var panel: PanelModel?
    private var onboarding: OnboardingWindow?
    private var viewer: ViewerWindow?

    public init(commands: any CommandRunning, service: any ViewerService) {
        self.commands = commands
        self.service = service
        super.init()
    }

    public func applicationDidFinishLaunching(_ notification: Notification) {
        // One window of each kind: no Show Tab Bar in the Window menu.
        NSWindow.allowsAutomaticWindowTabbing = false
        NSApp.mainMenu = appMenu(quit: (self, #selector(quitFromMenu(_:))))
        let model = PanelModel(service: service, commands: commands, loginItem: LoginItem())
        model.gettingStarted = { [weak self] in self?.showOnboarding() }
        model.openViewer = { [weak self] in
            self?.menuBar?.closePanel()
            self?.showViewer()
        }
        menuBar = MenuBarController(model: model)
        panel = model
    }

    /// A line from `atlas menubar` on standard input.
    public func received(_ line: String) {
        switch line.trimmingCharacters(in: .whitespacesAndNewlines) {
        case "viewer": showViewer()
        default: break
        }
    }

    /// The viewer window; one at a time, brought forward when already open.
    public func showViewer() {
        if let viewer, viewer.isOpen {
            viewer.show()
            return
        }
        let model = ViewerModel(service: service)
        if let panel { model.begin(with: panel.status, image: panel.image) }
        let window = ViewerWindow(model: model) { [weak self] closed in
            guard let self, viewer === closed else { return }
            viewer = nil
            settlePolicy()
        }
        viewer = window
        window.show()
    }

    /// A Dock icon while any window is open; none otherwise.
    private func settlePolicy() {
        let open = viewer?.isOpen == true || onboarding?.isOpen == true
        NSApp.setActivationPolicy(open ? .regular : .accessory)
    }

    /// Standard input closed: the service is gone, so the app goes too.
    public func parentClosed() {
        NSApp.terminate(nil)
    }

    /// Quit Atlas (⌘Q) in a window closes that window: the menu bar and the
    /// helper's service keep running. Quit Atlas in the panel, logging out
    /// and the service going away all quit.
    @objc func quitFromMenu(_ sender: Any?) {
        if let onboarding, onboarding.isOpen, onboarding.isKey {
            onboarding.close()
        } else if let viewer, viewer.isOpen, viewer.isKey {
            viewer.close()
        } else {
            NSApp.terminate(sender)
        }
    }

    /// The setup window; one at a time, brought forward when already open.
    public func showOnboarding() {
        if let onboarding, onboarding.isOpen {
            onboarding.show()
            return
        }
        let window = OnboardingWindow(model: OnboardingModel(commands: commands)) { [weak self] closed in
            guard let self, onboarding === closed else { return }
            onboarding = nil
            settlePolicy()
        }
        onboarding = window
        window.show()
    }
}
