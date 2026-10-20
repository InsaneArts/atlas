import AppKit
import AtlasKit

/// Atlas.app's executable, the app target: AppKit windows, one controller and presenter per window, rendering and
/// launch. It reads its environment once (``AppConfiguration``) and draws with AtlasUI, which never sees AtlasKit.
///
/// `atlas menubar` starts it with the viewer's capability URL on stdin, never in argv, and it quits when stdin
/// closes, so it never outlives the `atlas` that started it. A few flags run one task and exit instead
/// (``LaunchArguments``).
@main
@MainActor
enum AtlasApp {
  /// Starts the launch its arguments ask for.
  static func main() {
    // A command that exits before reading what it was given must not end
    // the app: writes to its closed pipe fail with an error instead.
    signal(SIGPIPE, SIG_IGN)
    let arguments = CommandLine.arguments
    let executable = URL(fileURLWithPath: arguments[0])
      .resolvingSymlinksInPath()
    let configuration = AppConfiguration(
      environment: ProcessInfo.processInfo.environment,
      executable: executable
    )
    let loginItem = OpenAtLogin.live(LoginItem())
    var render: String?
    var dark = false
    let mode = LaunchArguments.mode(arguments)
    switch mode {
    case .notice(let title, let message, let switchBack):
      NoticeAlert.show(title: title, message: message, switchBack: switchBack, commands: configuration.commands)
    case .login(let change):
      LoginCommand.run(change, loginItem: loginItem)
    case .onboarding:
      onboarding(commands: configuration.commands, loginItem: loginItem)
    #if DEBUG
      case .renderOnboarding(let folder):
        drawPreviews { OnboardingRender.draw(to: folder) }
      case .renderViewer(let folder):
        drawPreviews { ViewerRender.draw(to: folder) }
    #else
      case .renderOnboarding:
        needsDebugBuild("--render-onboarding")
      case .renderViewer:
        needsDebugBuild("--render-viewer")
    #endif
    case .app(let folder, let inDark):
      render = folder
      dark = inDark
    }
    guard let line = readLine(), let service = ViewerClient(line) else {
      FileHandle.standardError
        .write(Data("Expected the viewer URL on stdin\n".utf8))
      exit(2)
    }
    let app = NSApplication.shared
    DockIcon.settle(windowOpen: false)
    if let render {
      drawPanel(service: service, overrides: configuration.renderOverrides, to: render, dark: dark)
    }
    let controller = AppController(commands: configuration.commands, service: service, loginItem: loginItem)
    app.delegate = controller
    listen(received: { line in controller.received(line) }, closed: { controller.parentClosed() })
    withExtendedLifetime(controller) { app.run() }
  }

  #if DEBUG
    /// `--render-onboarding DIR` or `--render-viewer DIR`: draw every screen from the preview states, which only a
    /// debug build carries, then exit.
    ///
    /// - Parameter draw: Draws the screens to their folder, returning whether every one was drawn.
    /// - Returns: Never; the process exits.
    static func drawPreviews(_ draw: () -> Bool) -> Never {
      DockIcon.settle(windowOpen: false)
      exit(draw() ? 0 : 4)
    }
  #else
    /// A render flag that draws the preview states, which a release build does not carry: say how to draw them,
    /// then exit.
    ///
    /// - Parameter flag: The flag, `--render-onboarding` or `--render-viewer`.
    /// - Returns: Never; the process exits.
    static func needsDebugBuild(_ flag: String) -> Never {
      FileHandle.standardError
        .write(Data("\(flag) needs a debug build: swift run AtlasApp \(flag) DIR\n".utf8))
      exit(2)
    }
  #endif

  /// `--render PATH`: draw the panel from the service's status, then exit.
  ///
  /// - Parameters:
  ///   - service: The viewer service to read.
  ///   - overrides: How the drawing differs from the live panel.
  ///   - path: Where the PNGs go.
  ///   - dark: Whether the menu bar strip is drawn dark.
  /// - Returns: Never; the process exits.
  static func drawPanel(service: ViewerClient, overrides: PanelRenderOverrides, to path: String, dark: Bool) -> Never {
    let app = NSApplication.shared
    listen(received: { _ in }, closed: { NSApp.terminate(nil) })
    Task {
      let drawn = await PanelRender.draw(
        service: service,
        overrides: overrides,
        to: path,
        appearance: dark ? .darkAqua : .aqua
      )
      exit(drawn ? 0 : 4)
    }
    app.run()
    exit(0)
  }

  /// `--onboarding`: the setup window on its own (Atlas.app opened before it is set up). It quits when the window
  /// closes.
  ///
  /// - Parameters:
  ///   - commands: The `atlas` the window runs.
  ///   - loginItem: The app's login item.
  /// - Returns: Never; the process exits.
  static func onboarding(commands: Commands, loginItem: OpenAtLogin) -> Never {
    let app = NSApplication.shared
    app.mainMenu = MainMenu.make()
    let controller = OnboardingController(dependencies: .live(commands: commands, loginItem: loginItem))
    let window = OnboardingWindow(controller: controller) { _ in NSApp.terminate(nil) }
    DockIcon.settle(windowOpen: true)
    app.finishLaunching()
    window.show()
    withExtendedLifetime(window) { app.run() }
    exit(0)
  }

  /// Pass each line from the parent on, and quit when it closes stdin, so the app never outlives it.
  ///
  /// - Parameters:
  ///   - received: Called with each line.
  ///   - closed: Called once stdin closes.
  static func listen(
    received: @escaping @MainActor @Sendable (String) -> Void,
    closed: @escaping @MainActor @Sendable () -> Void
  ) {
    Thread.detachNewThread {
      while let line = readLine() {
        Task { @MainActor in received(line) }
      }
      Task { @MainActor in closed() }
    }
  }
}
