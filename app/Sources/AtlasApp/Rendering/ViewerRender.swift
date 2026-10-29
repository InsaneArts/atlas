#if DEBUG
  import AppKit
  import AtlasUI
  import SwiftUI

  /// The viewer window's design renders. Debug builds only, since the samples are.
  enum ViewerRender {
    /// Every main window state drawn for visual checks, at the standard window size, one for each sample the
    /// previews list (``PreviewStates/viewer``), which `RenderTests` checks. Each render's name is its
    /// file's, numbered in the order the renders were added.
    static var renders: [PreviewState<ViewerState>] {
      [
        sample("01-working", PreviewStates.viewerWorking),
        sample("02-idle", PreviewStates.viewerIdle),
        sample("03-control", PreviewStates.viewerControl),
        sample("04-needs-you", PreviewStates.viewerNeedsYou),
        sample("05-stopped", PreviewStates.viewerStopped),
        sample("06-in-use-elsewhere", PreviewStates.viewerInUseElsewhere),
        sample("07-paused", PreviewStates.viewerPaused),
        sample("08-details", PreviewStates.viewerDetails),
        sample("09-not-ready", PreviewStates.viewerNotReady),
        sample("10-logged-out", PreviewStates.viewerLoggedOut),
        sample("11-connecting", PreviewStates.viewerConnecting),
        sample("12-switch", PreviewStates.viewerSwitch),
        sample("13-log-out", PreviewStates.viewerLogOut),
        sample("14-log-out-anyway", PreviewStates.viewerLogOutAnyway),
        sample("15-restart", PreviewStates.viewerRestart),
        sample("16-typing", PreviewStates.viewerTyping),
        sample("17-mid-action", PreviewStates.viewerMidAction),
        sample("18-actual-size-paused", PreviewStates.viewerActualSizePaused),
        sample("19-actual-size-needs-you", PreviewStates.viewerActualSizeNeedsYou),
        sample("20-activity", PreviewStates.viewerActivity),
        sample("21-activity-nothing-yet", PreviewStates.viewerActivityEmpty),
        sample("22-apps", PreviewStates.viewerApps),
        sample("23-show-on-screen", PreviewStates.viewerShowOnScreen),
        sample("24-screenshots", PreviewStates.viewerShots),
        sample("25-screenshot-large", PreviewStates.viewerShotLarge),
        sample("26-screenshots-none-yet", PreviewStates.viewerShotsEmpty),
        sample("27-activity-paused", PreviewStates.viewerActivityPaused),
        sample("28-apps-paused", PreviewStates.viewerAppsPaused),
        sample("29-show-on-screen-right-edge", PreviewStates.viewerShowOnScreenRightEdge),
        sample("30-details-checks", PreviewStates.viewerDetailsChecks),
        sample("31-sessions", PreviewStates.viewerSessions),
        sample("32-sessions-menu-open", PreviewStates.viewerSessionsMenuOpen),
        sample("33-activity-by-session", PreviewStates.viewerActivityBySession),
        sample("34-activity-needs-you", PreviewStates.viewerActivityNeedsYou),
        sample("35-activity-one-session", PreviewStates.viewerActivityOneSession),
        sample("36-session-detail", PreviewStates.viewerSessionDetail),
        sample("37-focus-changed", PreviewStates.viewerFocusChanged),
        sample("38-clean-up", PreviewStates.viewerCleanUp),
        sample("39-session-cleaned-up", PreviewStates.viewerSessionCleanedUp),
        sample("40-stopped-jobs-left", PreviewStates.viewerStoppedJobsLeft),
        sample("41-control-command-runs", PreviewStates.viewerControlCommandRuns),
        sample("42-control-job-runs", PreviewStates.viewerControlJobRuns),
        sample("43-activity-one-session-empty", PreviewStates.viewerActivityOneSessionEmpty),
        sample("44-screenshots-one-session", PreviewStates.viewerShotsOneSession),
        sample("45-screenshots-one-session-empty", PreviewStates.viewerShotsOneSessionEmpty),
        sample("46-session-cleaning-up", PreviewStates.viewerSessionCleaningUp),
        sample("47-session-ended", PreviewStates.viewerSessionEnded),
      ]
    }

    /// The render names, in order, for a test to list as its arguments.
    static var names: [String] { renders.map(\.name) }

    /// A render at the standard window size.
    ///
    /// - Parameters:
    ///   - name: Its file's name.
    ///   - state: The state it draws.
    /// - Returns: The render.
    private static func sample(_ name: String, _ state: ViewerState) -> PreviewState<ViewerState> {
      PreviewState(name: name, value: state, size: PreviewStates.viewerSize)
    }

    /// The narrow windows the top row is drawn in for checking how it draws in: the least width, then 900 and 1040,
    /// each side of a step (`ViewerLayout.topRowFit`).
    nonisolated static let narrowWidths: [CGFloat] = [860, 900, 1040]

    /// The narrow renders, each named by its file and drawn at its window's width and the standard height: Working,
    /// Stopped, You have control and the sessions button at each of ``narrowWidths``, then Activity, Apps and
    /// Screenshots with Details open at the least width, where their headers have the least room.
    static var narrowRenders: [PreviewState<ViewerState>] {
      let states = [
        ("working", PreviewStates.viewerWorking), ("stopped", PreviewStates.viewerStopped),
        ("control", PreviewStates.viewerControl), ("sessions", PreviewStates.viewerSessions),
      ]
      let tabs = [
        ("activity", PreviewStates.viewerActivity), ("apps", PreviewStates.viewerApps),
        ("screenshots", PreviewStates.viewerShots),
      ]
      let least = narrowWidths[0]
      return narrowWidths.flatMap { width in
        states.map { narrow("narrow-\(Int(width))-\($0.0)", $0.1, width: width) }
      } + tabs.map { narrow("narrow-\(Int(least))-\($0.0)-details", withDetails($0.1), width: least) }
    }

    /// The narrow render names, in order, for a test to list as its arguments.
    static var narrowNames: [String] { narrowRenders.map(\.name) }

    /// A render in a narrow window, at the standard height.
    ///
    /// - Parameters:
    ///   - name: Its file's name.
    ///   - state: The state it draws.
    ///   - width: The window's width.
    /// - Returns: The render.
    private static func narrow(_ name: String, _ state: ViewerState, width: CGFloat) -> PreviewState<ViewerState> {
      PreviewState(name: name, value: state, size: CGSize(width: width, height: PreviewStates.viewerSize.height))
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

    /// The sizes the frame renders draw the four tabs at: the standard window with Details, the narrower
    /// steps with and without it, and two full screens, so every tab can be seen to stand in the same frame.
    /// The design check draws them; the tests draw each state once, at the standard size or a narrow one.
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

    /// The frame renders, each named by its file and drawn at its window's size.
    static var frameRenders: [PreviewState<ViewerState>] {
      frameSizes.flatMap { size in
        tabs.map { tab in
          var state = size.details ? withDetails(tab.state) : tab.state
          state.top.isFullScreen = size.fullScreen
          return PreviewState(name: "frame-\(size.name)-\(tab.name)", value: state, size: size.size)
        }
      }
    }

    /// Draws every main window state to PNGs in `directory`, at Retina scale (``PNG/scale``): the standard
    /// renders, the narrow ones, then the frames, each at its size.
    ///
    /// - Parameter directory: Where the PNGs go.
    /// - Returns: Whether every state was drawn.
    static func draw(to directory: String) -> Bool {
      let folder = URL(fileURLWithPath: directory)
      return (renders + narrowRenders + frameRenders)
        .map { draw($0.name, $0.value, to: folder, size: $0.size ?? PreviewStates.viewerSize) }
        .allSatisfy { $0 }
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
