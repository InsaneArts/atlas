import AppKit
import AtlasKit
import AtlasUI

/// Atlas.app's executable. `atlas menubar` starts it with the viewer's
/// capability URL on stdin, never in argv, and it quits when stdin closes,
/// so it never outlives the `atlas` that started it. A few flags run one
/// task and exit instead.
@main
@MainActor
enum AtlasApp {
    static let arguments = CommandLine.arguments
    static let commands = Commands.parent(fallback: URL(fileURLWithPath: arguments[0]).resolvingSymlinksInPath())

    static func main() {
        // A command that exits before reading what it was given must not end
        // the app: writes to its closed pipe fail with an error instead.
        signal(SIGPIPE, SIG_IGN)
        if let index = arguments.firstIndex(of: "--notice"), index + 2 < arguments.count {
            notice(title: arguments[index + 1], message: arguments[index + 2])
        }
        if let index = arguments.firstIndex(of: "--login"), index + 1 < arguments.count {
            login(arguments[index + 1])
        }
        if arguments.contains("--onboarding") { onboarding() }
        if let index = arguments.firstIndex(of: "--render-onboarding"), index + 1 < arguments.count {
            NSApplication.shared.setActivationPolicy(.accessory)
            exit(renderOnboarding(to: arguments[index + 1]) ? 0 : 4)
        }
        if let index = arguments.firstIndex(of: "--render-viewer"), index + 1 < arguments.count {
            NSApplication.shared.setActivationPolicy(.accessory)
            exit(renderViewer(to: arguments[index + 1]) ? 0 : 4)
        }
        guard let line = readLine(), let service = ViewerClient(line) else {
            FileHandle.standardError.write(Data("Expected the viewer URL on stdin\n".utf8))
            exit(2)
        }
        let app = NSApplication.shared
        app.setActivationPolicy(.accessory)
        if let index = arguments.firstIndex(of: "--render"), index + 1 < arguments.count {
            render(service: service, to: arguments[index + 1], dark: arguments.contains("--dark"))
        }
        let controller = AppController(commands: commands, service: service)
        app.delegate = controller
        listen(received: { line in controller.received(line) }, closed: { controller.parentClosed() })
        withExtendedLifetime(controller) { app.run() }
    }

    /// `--notice TITLE MESSAGE`: tell the person something in this session
    /// (the helper's, when its daemon asks), wait for OK, then exit.
    static func notice(title: String, message: String) -> Never {
        let app = NSApplication.shared
        app.setActivationPolicy(.accessory)
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = message
        alert.addButton(withTitle: "OK")
        // Finish launching first, so the request to come forward is honoured
        // and the notice has keyboard focus (Return closes it).
        app.finishLaunching()
        app.activate(ignoringOtherApps: true)
        alert.window.level = .floating
        alert.runModal()
        exit(0)
    }

    /// `--login on|off|status`: manage Open at Login for the command line.
    static func login(_ change: String) -> Never {
        let item = LoginItem()
        do {
            switch change {
            case "on": try item.set(true)
            case "off": try item.set(false)
            default: break
            }
            print("{\"ok\": true, \"enabled\": \(item.enabled), \"available\": \(item.available)}")
            exit(0)
        } catch {
            let message = error.localizedDescription.replacingOccurrences(of: "\"", with: "'")
            print("{\"ok\": false, \"error\": \"\(message)\"}")
            exit(1)
        }
    }

    /// `--render PATH`: draw the panel from the service's status, then exit.
    static func render(service: ViewerClient, to path: String, dark: Bool) -> Never {
        let app = NSApplication.shared
        listen(received: { _ in }, closed: { NSApp.terminate(nil) })
        Task { exit(await renderPanel(service: service, to: path, appearance: dark ? .darkAqua : .aqua) ? 0 : 4) }
        app.run()
        exit(0)
    }

    /// `--onboarding`: the setup window on its own (Atlas.app opened before
    /// it is set up). It quits when the window closes.
    static func onboarding() -> Never {
        let app = NSApplication.shared
        app.mainMenu = appMenu()
        let window = OnboardingWindow(model: OnboardingModel(commands: commands)) { _ in NSApp.terminate(nil) }
        app.setActivationPolicy(.regular)
        app.finishLaunching()
        window.show()
        withExtendedLifetime(window) { app.run() }
        exit(0)
    }

    /// Pass each line from the parent on, and quit when it closes stdin,
    /// so the app never outlives it.
    static func listen(
        received: @escaping @MainActor @Sendable (String) -> Void, closed: @escaping @MainActor @Sendable () -> Void
    ) {
        Thread.detachNewThread {
            while let line = readLine() {
                Task { @MainActor in received(line) }
            }
            Task { @MainActor in closed() }
        }
    }
}
