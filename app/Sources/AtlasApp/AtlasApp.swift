import AppKit
import AtlasCommands
import AtlasService

/// Atlas.app's executable, the app target: AppKit windows, one controller and presenter per window, rendering and
/// launch. It reads its environment once (``AppConfiguration``) and draws with AtlasUI, which sees no domain package.
///
/// `atlas menubar` starts it with the viewer's capability URL on stdin, never in argv, and it quits when stdin
/// closes, so it never outlives the `atlas` that started it. A few flags run one task and exit instead
/// (``LaunchArguments``).
@main
enum AtlasApp {
  /// The longest a launch waits for its secret after the viewer's URL, in seconds: past it the app goes on without one,
  /// and what only the person's app may do says how to get it back.
  static let secretWait: TimeInterval = 2

  /// Starts the launch its arguments ask for: one function each.
  static func main() {
    // A command that exits before reading what it was given must not end
    // the app: writes to its closed pipe fail with an error instead.
    signal(SIGPIPE, SIG_IGN)
    let arguments = CommandLine.arguments
    let executable = URL(fileURLWithPath: arguments[0])
      .resolvingSymlinksInPath()
    let configuration = AppConfiguration.reading(executable: executable)
    let loginItem = OpenAtLogin.live(LoginItem())
    // The one live clock: every window's waits run on it, and tests hand in a manual one.
    let clock = ContinuousClock()
    switch LaunchArguments.mode(arguments) {
    case .notice(let title, let message, let switchBack):
      NoticeAlert.show(title: title, message: message, switchBack: switchBack, commands: configuration.commands)
    case .login(let change):
      LoginCommand.run(change, loginItem: loginItem)
    case .onboarding:
      OnboardingAlone.run(commands: configuration.commands, loginItem: loginItem, clock: clock)
    #if DEBUG
      case .renderOnboarding(let folder, let dark):
        drawPreviews(dark: dark) { appearance in
          OnboardingRender.draw(to: folder, appearance: appearance)
            && DockedRender.draw(to: folder, appearance: appearance)
        }
      case .renderViewer(let folder, let dark):
        drawPreviews(dark: dark) { appearance in
          ViewerRender.draw(to: folder, appearance: appearance)
            && ComponentRender.draw(to: folder + "/components", appearance: appearance)
        }
      case .renderSettings(let folder, let dark):
        drawPreviews(dark: dark) { appearance in SettingsRender.draw(to: folder, appearance: appearance) }
    #else
      case .renderOnboarding:
        needsDebugBuild("--render-onboarding")
      case .renderViewer:
        needsDebugBuild("--render-viewer")
      case .renderSettings:
        needsDebugBuild("--render-settings")
    #endif
    case .renderMenuBar(let folder):
      drawMenuBar(to: folder)
    case .app(let render, let dark):
      app(configuration: configuration, loginItem: loginItem, clock: clock, render: render, dark: dark)
    }
  }

  /// The app itself, once `atlas menubar` has given it the viewer's capability URL and this launch's secret on stdin:
  /// the menu bar item and the windows it opens, built by the container. With `--render PATH`, the panel is drawn instead.
  ///
  /// - Parameters:
  ///   - configuration: What the app took from its environment.
  ///   - loginItem: The app's login item.
  ///   - clock: The one live clock.
  ///   - render: Where `--render` draws the panel, or nil for the live app.
  ///   - dark: Whether that drawing is in the dark appearance.
  static func app(
    configuration: AppConfiguration,
    loginItem: OpenAtLogin,
    clock: any Clock<Duration>,
    render: String?,
    dark: Bool
  ) {
    let input = LaunchInput()
    guard case .line(let line) = input.next() else {
      FileHandle.standardError
        .write(Data("Expected the viewer URL on stdin\n".utf8))
      exit(2)
    }
    // The launch's secret comes next, kept in memory only; a render is given none, and a launch never waits for it
    // longer than ``secretWait``. A line that is not one is the parent's first, passed on once the app listens.
    var next: String?
    if render == nil, case .line(let line) = input.next(within: secretWait) { next = line }
    let secret = next.flatMap(ViewerClient.appSecret(fromLine:))
    guard let service = ViewerClient(line, appSecret: secret) else {
      FileHandle.standardError
        .write(Data("Expected the viewer URL on stdin\n".utf8))
      exit(2)
    }
    let app = NSApplication.shared
    DockIcon.settle(windowOpen: false)
    if let render {
      drawPanel(service: service, input: input, overrides: configuration.renderOverrides, to: render, dark: dark)
    }
    let delegate = AppDelegate(
      container: AppContainer(
        commands: configuration.commands,
        service: service,
        loginItem: loginItem,
        clock: clock
      )
    )
    app.delegate = delegate
    // Unstructured, so the line is handled once the app runs, as every later one is.
    if secret == nil, let next { Task { @MainActor in delegate.received(next) } }
    input.listen(received: { line in delegate.received(line) }, closed: { delegate.parentClosed() })
    withExtendedLifetime(delegate) { app.run() }
  }

  #if DEBUG
    /// `--render-onboarding DIR`, `--render-viewer DIR` or `--render-settings DIR`: draw every screen from the preview
    /// states, which only a debug build carries, in light, or with `--dark` in dark, then exit.
    ///
    /// - Parameters:
    ///   - dark: Whether the screens are drawn in the dark appearance.
    ///   - draw: Draws the screens to their folder in the appearance it is given, returning whether every one was
    ///     drawn.
    /// - Returns: Never; the process exits.
    static func drawPreviews(dark: Bool, _ draw: (NSAppearance?) -> Bool) -> Never {
      DockIcon.settle(windowOpen: false)
      exit(draw(dark ? Appearance.dark : Appearance.light) ? 0 : 4)
    }
  #else
    /// A render flag that draws the preview states, which a release build does not carry: say how to draw them,
    /// then exit.
    ///
    /// - Parameter flag: The flag, `--render-onboarding`, `--render-viewer` or `--render-settings`.
    /// - Returns: Never; the process exits.
    static func needsDebugBuild(_ flag: String) -> Never {
      FileHandle.standardError
        .write(Data("\(flag) needs a debug build: swift run AtlasApp \(flag) DIR\n".utf8))
      exit(2)
    }
  #endif

  /// `--render-menubar DIR`: draw the menu bar item in every state, light and dark, then exit. It needs no preview
  /// state or service, so release builds draw it too.
  ///
  /// - Parameter folder: Where the PNGs go.
  /// - Returns: Never; the process exits.
  static func drawMenuBar(to folder: String) -> Never {
    DockIcon.settle(windowOpen: false)
    exit(MenuBarRender.draw(to: folder) ? 0 : 4)
  }

  /// `--render PATH`: draw the panel from the service's status, then exit.
  ///
  /// - Parameters:
  ///   - service: The viewer service to read.
  ///   - input: The parent's lines, whose close quits.
  ///   - overrides: How the drawing differs from the live panel.
  ///   - path: Where the PNGs go.
  ///   - dark: Whether the panel and its menu bar strip are drawn dark.
  /// - Returns: Never; the process exits.
  static func drawPanel(
    service: ViewerClient,
    input: LaunchInput,
    overrides: PanelRenderOverrides,
    to path: String,
    dark: Bool
  ) -> Never {
    let app = NSApplication.shared
    input.listen(received: { _ in }, closed: { NSApp.terminate(nil) })
    // Unstructured, as the drawing needs the app running below; it exits the process once drawn.
    Task {
      let drawn = await PanelRender.draw(
        service: service,
        overrides: overrides,
        to: path,
        appearance: dark ? Appearance.dark : Appearance.light
      )
      exit(drawn ? 0 : 4)
    }
    app.run()
    exit(0)
  }

}
