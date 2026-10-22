#if DEBUG
  import AppKit
  import AtlasUI
  import SwiftUI

  /// The viewer window's design renders. Debug builds only, since the samples are.
  @MainActor
  enum ViewerRender {
    /// Each render's file name, numbered in the order the renders were added, one for each sample the previews list
    /// (``PreviewStates/viewer``), which `RenderTests` checks. Kept apart from the states, which are the main actor's,
    /// so a test can list them as its arguments.
    nonisolated static let files = [
      "01-working",
      "02-idle",
      "03-control",
      "04-needs-you",
      "05-stopped",
      "06-in-use-elsewhere",
      "07-paused",
      "08-details",
      "09-not-ready",
      "10-logged-out",
      "11-connecting",
      "12-switch",
      "13-log-out",
      "14-log-out-anyway",
      "15-restart",
      "16-typing",
      "17-mid-action",
      "18-actual-size-paused",
      "19-actual-size-needs-you",
      "20-activity",
      "21-activity-nothing-yet",
      "22-apps",
      "23-show-on-screen",
      "24-screenshots",
      "25-screenshot-large",
      "26-screenshots-none-yet",
      "27-activity-paused",
      "28-apps-paused",
      "29-show-on-screen-right-edge",
      "30-details-checks",
    ]

    /// The narrow windows the top row is drawn in for checking how it draws in: the least width, then 900 and 1040,
    /// each side of a step (`ViewerLayout.topRowFit`).
    nonisolated static let narrowWidths: [CGFloat] = [860, 900, 1040]

    /// The narrow renders' file names: Working, Stopped and You have control at each of ``narrowWidths``, then
    /// Activity, Apps and Screenshots with Details open at the least width, where their headers have the least room.
    nonisolated static let narrowFiles: [String] =
      narrowWidths.flatMap { width in
        ["working", "stopped", "control"].map { "narrow-\(Int(width))-\($0)" }
      } + ["activity", "apps", "screenshots"].map { "narrow-\(Int(narrowWidths[0]))-\($0)-details" }

    /// The narrow renders, by file name, with their window's width.
    static var narrowRenders: [(file: String, state: ViewerState, width: CGFloat)] {
      let states = [PreviewStates.viewerWorking, PreviewStates.viewerStopped, PreviewStates.viewerControl]
      let tabs = [PreviewStates.viewerActivity, PreviewStates.viewerApps, PreviewStates.viewerShots]
        .map { (withDetails($0), narrowWidths[0]) }
      return zip(narrowFiles, narrowWidths.flatMap { width in states.map { ($0, width) } } + tabs)
        .map { (file: $0, state: $1.0, width: $1.1) }
    }

    /// `state` with the Details column open beside its pane, as the Details sample shows it.
    ///
    /// - Parameter state: A sample.
    /// - Returns: The sample with Details.
    private static func withDetails(_ state: ViewerState) -> ViewerState {
      var shown = state
      shown.details = PreviewStates.viewerDetails.details
      shown.top.actions.details = PreviewStates.viewerDetails.top.actions.details
      return shown
    }

    /// The four tabs' samples, Screen first, which the frame renders draw at every size.
    private static var tabs: [(name: String, state: ViewerState)] {
      [
        ("screen", PreviewStates.viewerWorking), ("activity", PreviewStates.viewerActivity),
        ("apps", PreviewStates.viewerApps), ("screenshots", PreviewStates.viewerShots),
      ]
    }

    /// The sizes the frame renders draw the four tabs at: the boards' window with Details, the narrower steps with
    /// and without it, and two full screens, so every tab can be seen to stand in the same frame. The design check
    /// draws them; the tests draw each state once, at the boards' size or a narrow one.
    private static let frameSizes: [(name: String, size: NSSize, details: Bool, fullScreen: Bool)] = [
      ("1440-details", NSSize(width: 1440, height: 900), true, false),
      ("1040", NSSize(width: 1040, height: 900), false, false),
      ("1040-details", NSSize(width: 1040, height: 900), true, false),
      ("860", NSSize(width: 860, height: 900), false, false),
      ("860-details", NSSize(width: 860, height: 900), true, false),
      ("1280x1369-tall", NSSize(width: 1280, height: 1369), false, false),
      ("1728x1117-fullscreen", NSSize(width: 1728, height: 1117), false, true),
      ("2560x1440-fullscreen", NSSize(width: 2560, height: 1440), false, true),
    ]

    /// The frame renders, by file name, with their window's size.
    static var frameRenders: [(file: String, state: ViewerState, size: NSSize)] {
      frameSizes.flatMap { size in
        tabs.map { tab in
          var state = size.details ? withDetails(tab.state) : tab.state
          state.top.isFullScreen = size.fullScreen
          return (file: "frame-\(size.name)-\(tab.name)", state: state, size: size.size)
        }
      }
    }

    /// Every main window state drawn for checking against the boards, by file name, at the boards' window size.
    static var renders: [(file: String, state: ViewerState)] {
      Array(zip(files, states))
    }

    /// The states, in the order of ``files``.
    private static let states = [
      PreviewStates.viewerWorking,
      PreviewStates.viewerIdle,
      PreviewStates.viewerControl,
      PreviewStates.viewerNeedsYou,
      PreviewStates.viewerStopped,
      PreviewStates.viewerInUseElsewhere,
      PreviewStates.viewerPaused,
      PreviewStates.viewerDetails,
      PreviewStates.viewerNotReady,
      PreviewStates.viewerLoggedOut,
      PreviewStates.viewerConnecting,
      PreviewStates.viewerSwitch,
      PreviewStates.viewerLogOut,
      PreviewStates.viewerLogOutAnyway,
      PreviewStates.viewerRestart,
      PreviewStates.viewerTyping,
      PreviewStates.viewerMidAction,
      PreviewStates.viewerActualSizePaused,
      PreviewStates.viewerActualSizeNeedsYou,
      PreviewStates.viewerActivity,
      PreviewStates.viewerActivityEmpty,
      PreviewStates.viewerApps,
      PreviewStates.viewerShowOnScreen,
      PreviewStates.viewerShots,
      PreviewStates.viewerShotLarge,
      PreviewStates.viewerShotsEmpty,
      PreviewStates.viewerActivityPaused,
      PreviewStates.viewerAppsPaused,
      PreviewStates.viewerShowOnScreenRightEdge,
      PreviewStates.viewerDetailsChecks,
    ]

    /// Draws every main window state to PNGs in `directory`, at Retina scale (``PNG/scale``).
    ///
    /// - Parameters:
    ///   - directory: Where the PNGs go.
    ///   - size: The window's size.
    /// - Returns: Whether every state was drawn.
    static func draw(to directory: String, size: NSSize = PreviewStates.viewerSize) -> Bool {
      let folder = URL(fileURLWithPath: directory)
      let wide = renders.map { draw($0.file, $0.state, to: folder, size: size) }
      let narrow = narrowRenders.map {
        draw($0.file, $0.state, to: folder, size: NSSize(width: $0.width, height: size.height))
      }
      let frames = frameRenders.map { draw($0.file, $0.state, to: folder, size: $0.size) }
      return (wide + narrow + frames).allSatisfy { $0 }
    }

    /// Draws one main window state to `file`.png in `folder`, at Retina scale.
    ///
    /// - Parameters:
    ///   - file: The PNG's name, without its extension.
    ///   - state: The state.
    ///   - folder: Where the PNG goes.
    ///   - size: The window's size.
    /// - Returns: Whether it was drawn.
    static func draw(
      _ file: String,
      _ state: ViewerState,
      to folder: URL,
      size: NSSize = PreviewStates.viewerSize
    ) -> Bool {
      PNG.write(
        ViewerView(state: state) { _ in }
          .environment(\.appIcon, AppIconImage.image),
        size: size,
        to: folder.appendingPathComponent("\(file).png")
      )
    }
  }
#endif
