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
        sample("31-sessions", PreviewStates.viewerSessions),
        sample("32-sessions-one-ended", PreviewStates.viewerSessionsEnded),
        sample("33-activity-by-session", PreviewStates.viewerActivityBySession),
        sample("34-activity-needs-you", PreviewStates.viewerActivityNeedsYou),
        sample("35-activity-one-session", PreviewStates.viewerActivityOneSession),
        sample("37-focus-changed", PreviewStates.viewerFocusChanged),
        sample("38-clean-up", PreviewStates.viewerCleanUp),
        sample("40-stopped-jobs-left", PreviewStates.viewerStoppedJobsLeft),
        sample("41-control-command-runs", PreviewStates.viewerControlCommandRuns),
        sample("42-control-job-runs", PreviewStates.viewerControlJobRuns),
        sample("43-activity-one-session-empty", PreviewStates.viewerActivityOneSessionEmpty),
        sample("44-screenshots-one-session", PreviewStates.viewerShotsOneSession),
        sample("45-screenshots-one-session-empty", PreviewStates.viewerShotsOneSessionEmpty),
        sample("48-sidebar-by-state", PreviewStates.viewerSidebarByState),
        sample("49-sidebar-ended-folded", PreviewStates.viewerSidebarEndedFolded),
        sample("50-sidebar-search-open", PreviewStates.viewerSidebarSearch),
        sample("51-sidebar-hidden", PreviewStates.viewerSidebarHidden),
        sample("53-pip-top-left", inCorner(.topLeading, PreviewStates.viewerActivity)),
        sample("54-pip-top-right-screenshots", inCorner(.topTrailing, PreviewStates.viewerShots)),
        sample("55-pip-bottom-left-apps-paused", inCorner(.bottomLeading, PreviewStates.viewerAppsPaused)),
        sample("56-helper-needs-setup", PreviewStates.viewerHelperNeedsSetup),
        sample("57-session-chosen", PreviewStates.viewerSessionChosen),
        sample("58-cleaning-up-row", PreviewStates.viewerSessionCleaningUp),
        sample("59-cleaned-up-footer", PreviewStates.viewerSessionCleanedUp),
        sample("60-session-ended-chosen", PreviewStates.viewerSessionEnded),
        sample("61-sidebar-many-sessions", PreviewStates.viewerSidebarMany),
        sample("62-activity-moment-opened", PreviewStates.viewerActivityOpened),
        sample("63-activity-problems", PreviewStates.viewerActivityProblems),
        sample("64-activity-days-before", PreviewStates.viewerActivityDays),
        sample("65-activity-many-moments", PreviewStates.viewerActivityMany),
        sample("66-apps-one-session", PreviewStates.viewerAppsOneSession),
        sample("67-apps-one-session-empty", PreviewStates.viewerAppsOneSessionEmpty),
        sample("68-apps-copy-monogram", PreviewStates.viewerAppsCopy),
        sample("69-apps-hidden", PreviewStates.viewerAppsHidden),
        sample("70-apps-many", PreviewStates.viewerAppsMany),
      ]
    }

    /// `state` with the small screen kept to `corner`, as the person can drag it.
    ///
    /// - Parameters:
    ///   - corner: The corner.
    ///   - state: A sample with the small screen.
    /// - Returns: The sample.
    private static func inCorner(_ corner: MiniCorner, _ state: ViewerState) -> ViewerState {
      var moved = state
      moved.miniCorner = corner
      return moved
    }

    /// The render names, in order, for a test to list as its arguments.
    static var names: [String] { renders.map(\.name) }

    /// The lists drawn as they end, each scrolled to its very end under the small screen, and the Screenshots grid's
    /// own cases: many screenshots, a dark one selected, and every shape, at the standard size and in narrow windows.
    static var listRenders: [PreviewState<ViewerState>] {
      let narrow = shotWidths.flatMap { width in
        [("screenshots", PreviewStates.viewerShots), ("screenshots-mixed", PreviewStates.viewerShotsMixed)]
          .map { name, state in
            let size = CGSize(width: width, height: PreviewStates.viewerSize.height)
            return PreviewState(
              name: "list-narrow-\(Int(width))-\(name)",
              value: PreviewStates.viewer(state, fittedTo: size),
              size: size
            )
          }
      }
      return [
        sample("list-end-activity-many", PreviewStates.viewerActivityMany),
        sample("list-end-apps-many", PreviewStates.viewerAppsMany),
        sample("list-end-screenshots-many", PreviewStates.viewerShotsMany),
        sample("list-end-screenshots-many-top-right", inCorner(.topTrailing, PreviewStates.viewerShotsMany)),
        sample("list-screenshots-many", PreviewStates.viewerShotsMany),
        sample("list-screenshots-dark-selected", PreviewStates.viewerShotsDarkSelected),
        sample("list-screenshots-mixed", PreviewStates.viewerShotsMixed),
      ] + narrow
    }

    /// The list render names, in order, for a test to list as its arguments.
    static var listNames: [String] { listRenders.map(\.name) }

    /// The widths the Screenshots grid is drawn narrow at: one column, and two.
    nonisolated static let shotWidths: [CGFloat] = [480, 760]

    /// Whether a list render shows its list scrolled to its very end.
    ///
    /// - Parameter name: The render's name.
    /// - Returns: Whether it does.
    static func startsAtEnd(_ name: String) -> Bool {
      name.hasPrefix("list-end-")
    }

    /// A render at the standard window size.
    ///
    /// - Parameters:
    ///   - name: Its file's name.
    ///   - state: The state it draws.
    /// - Returns: The render.
    private static func sample(_ name: String, _ state: ViewerState) -> PreviewState<ViewerState> {
      PreviewState(name: name, value: state, size: PreviewStates.viewerSize)
    }

    /// The narrow windows the content is drawn in for checking how it gives way: the least the renders draw, then
    /// widths with the sidebar hidden under 688 as the card's header and footer give way, and the sidebar whole from
    /// 700.
    nonisolated static let narrowWidths: [CGFloat] = [320, 400, 480, 600, 700, 860]

    /// The short windows the content is drawn in for checking how the Library folds into a row: the least the renders
    /// draw, one between, and the height it folds under.
    nonisolated static let shortHeights: [CGFloat] = [220, 300, 420]

    /// The narrow renders, each named by its file and drawn at its window's width and the standard height, as a window
    /// opened at that width fits them: Working, You have control, Stopped, Activity, the sidebar hidden, and the sidebar
    /// over the card where the width leaves no room for it beside the card, at each of ``narrowWidths``; then Working
    /// whole and the sidebar over the card at each of ``shortHeights``, each with a few sessions and with many; and the
    /// Apps list at each of ``appsWidths``.
    static var narrowRenders: [PreviewState<ViewerState>] {
      let states = [
        ("working", PreviewStates.viewerWorking), ("control", PreviewStates.viewerControl),
        ("stopped", PreviewStates.viewerStopped), ("activity", PreviewStates.viewerActivity),
        ("sidebar-hidden", PreviewStates.viewerSidebarHidden), ("overlay", PreviewStates.viewerSidebarOverlay),
      ]
      let narrow = narrowWidths.flatMap { width in
        states.compactMap { state -> PreviewState<ViewerState>? in
          let size = CGSize(width: width, height: PreviewStates.viewerSize.height)
          let fitted = PreviewStates.viewer(state.1, fittedTo: size)
          // The sidebar shows over the card only where the width leaves no room for it beside the card.
          guard fitted.sidebar.overlay == state.1.sidebar.overlay else { return nil }
          return PreviewState(name: "narrow-\(Int(width))-\(state.0)", value: fitted, size: size)
        }
      }
      let apps = appsWidths.map { width in
        let size = CGSize(width: width, height: PreviewStates.viewerSize.height)
        return PreviewState(
          name: "narrow-\(Int(width))-apps",
          value: PreviewStates.viewer(PreviewStates.viewerApps, fittedTo: size),
          size: size
        )
      }
      let short = shortHeights.flatMap { height in
        [
          ("working", PreviewStates.viewerWorking, Self.shortWidths.whole),
          ("overlay", PreviewStates.viewerSidebarOverlay, Self.shortWidths.hidden),
          ("many", PreviewStates.viewerSidebarMany, Self.shortWidths.whole),
          ("many-overlay", Self.overlaid(PreviewStates.viewerSidebarMany), Self.shortWidths.hidden),
        ]
        .map { name, state, width -> PreviewState<ViewerState> in
          let size = CGSize(width: width, height: height)
          let file = "short-\(Int(width))x\(Int(height))-\(name)"
          return PreviewState(name: file, value: PreviewStates.viewer(state, fittedTo: size), size: size)
        }
      }
      return narrow + apps + short
    }

    /// The widths the Apps list is drawn narrow at: one column of cards, and two.
    nonisolated static let appsWidths: [CGFloat] = [320, 600]

    /// `state` with the sidebar shown over the card, as the toggle shows it in a window too narrow for it beside the card.
    ///
    /// - Parameter state: A sample.
    /// - Returns: The sample, its sidebar over the card.
    private static func overlaid(_ state: ViewerState) -> ViewerState {
      var over = state
      over.sidebar.overlay = true
      return over
    }

    /// The widths the short renders are drawn at: one with the whole sidebar, and one too narrow for it.
    private static let shortWidths: (whole: CGFloat, hidden: CGFloat) = (1040, 400)

    /// The narrow render names, in order, for a test to list as its arguments.
    static var narrowNames: [String] { narrowRenders.map(\.name) }

    /// The four places' samples, the Screen first, and the sidebar hidden, which the frame renders draw at every size.
    private static var tabs: [(name: String, state: ViewerState)] {
      [
        ("screen", PreviewStates.viewerWorking), ("activity", PreviewStates.viewerActivity),
        ("screenshots", PreviewStates.viewerShots), ("apps", PreviewStates.viewerApps),
        ("sidebar-hidden", PreviewStates.viewerSidebarHidden),
      ]
    }

    /// The sizes the frame renders draw the four places at: the window's first size, the standard one, the narrower
    /// steps, a tall one and two full screens, so every place can be seen to stand in the same card. The design check
    /// draws them; the tests draw each state once, at the standard size or a narrow one.
    private static let frameSizes: [(name: String, size: NSSize, fullScreen: Bool)] = [
      ("1200x800", NSSize(width: 1200, height: 800), false),
      ("1440", NSSize(width: 1440, height: 900), false),
      ("1040", NSSize(width: 1040, height: 900), false),
      ("860", NSSize(width: 860, height: 900), false),
      ("1280x1369-tall", NSSize(width: 1280, height: 1369), false),
      ("1728x1117-fullscreen", NSSize(width: 1728, height: 1117), true),
      ("2560x1440-fullscreen", NSSize(width: 2560, height: 1440), true),
    ]

    /// The frame renders, each named by its file and drawn at its window's size.
    static var frameRenders: [PreviewState<ViewerState>] {
      frameSizes.flatMap { size in
        tabs.map { tab in
          var state = tab.state
          state.isFullScreen = size.fullScreen
          return PreviewState(name: "frame-\(size.name)-\(tab.name)", value: state, size: size.size)
        }
      }
    }

    /// Draws every main window state to PNGs in `directory`, at Retina scale (``PNG/scale``): the standard
    /// renders, the narrow ones, the frames and the lists, each at its size.
    ///
    /// - Parameters:
    ///   - directory: Where the PNGs go.
    ///   - appearance: The appearance to draw them in, light or dark.
    /// - Returns: Whether every state was drawn.
    static func draw(to directory: String, appearance: NSAppearance?) -> Bool {
      let folder = URL(fileURLWithPath: directory)
      return (renders + narrowRenders + frameRenders + listRenders)
        .map {
          draw($0.name, $0.value, to: folder, size: $0.size ?? PreviewStates.viewerSize, appearance: appearance)
        }
        .allSatisfy { $0 }
    }

    /// Draws one main window state to `file`.png in `folder`, at Retina scale.
    ///
    /// - Parameters:
    ///   - file: The PNG's name, without its extension.
    ///   - state: The state.
    ///   - folder: Where the PNG goes.
    ///   - size: The window's size.
    ///   - appearance: The appearance to draw it in, light or dark.
    /// - Returns: Whether it was drawn.
    static func draw(
      _ file: String,
      _ state: ViewerState,
      to folder: URL,
      size: NSSize = PreviewStates.viewerSize,
      appearance: NSAppearance?
    ) -> Bool {
      if startsAtEnd(file) {
        return drawAtEnd(file, state, to: folder, size: size, appearance: appearance)
      }
      return PNG.write(
        ViewerView(state: state) { _ in }
          .environment(\.appIcon, AppIconImage.image),
        size: size,
        to: folder.appendingPathComponent("\(file).png"),
        appearance: appearance
      )
    }

    /// Draws one main window state to `file`.png in `folder` with the card's list scrolled to its very end, as the
    /// person scrolls it: the list is scrolled to its end, the run loop turned so it builds the rows it now shows, and
    /// again, as a lazy list grows while it builds, until it stands.
    ///
    /// - Parameters:
    ///   - file: The PNG's name, without its extension.
    ///   - state: The state.
    ///   - folder: Where the PNG goes.
    ///   - size: The window's size.
    ///   - appearance: The appearance to draw it in, light or dark.
    /// - Returns: Whether it was drawn.
    private static func drawAtEnd(
      _ file: String,
      _ state: ViewerState,
      to folder: URL,
      size: NSSize,
      appearance: NSAppearance?
    ) -> Bool {
      guard let appearance else { return false }
      let host = PNG.host(
        ViewerView(state: state) { _ in }
          .environment(\.appIcon, AppIconImage.image)
      )
      // In a window of its own, offscreen, as a list scrolls and builds its rows only inside one.
      let window = NSWindow(
        contentRect: NSRect(origin: .zero, size: size),
        styleMask: [.borderless],
        backing: .buffered,
        defer: false
      )
      window.isReleasedWhenClosed = false
      window.appearance = appearance
      window.contentView = host
      host.frame = NSRect(origin: .zero, size: size)
      host.layoutSubtreeIfNeeded()
      defer { window.contentView = nil }
      for _ in 0..<endPasses {
        for list in lists(in: host) {
          let end = max(list.documentView?.frame.height ?? .zero, list.contentView.bounds.height)
          list.contentView.scroll(to: NSPoint(x: .zero, y: end - list.contentView.bounds.height))
          list.reflectScrolledClipView(list.contentView)
        }
        let wait = Date(timeIntervalSinceNow: endPassWait)
        let loop = RunLoop.main
        loop.run(until: wait)
        host.layoutSubtreeIfNeeded()
      }
      guard let data = PNG.data(of: host, appearance: appearance) else { return false }
      return (try? data.write(to: folder.appendingPathComponent("\(file).png"))) != nil
    }

    /// The lists that scroll down in `view`: every scroll view under it taller inside than it shows.
    ///
    /// - Parameter view: A view.
    /// - Returns: The scroll views.
    private static func lists(in view: NSView) -> [NSScrollView] {
      let own: [NSScrollView] =
        if let list = view as? NSScrollView, let document = list.documentView,
          document.frame.height > list.contentView.bounds.height
        {
          [list]
        } else {
          []
        }
      return own + view.subviews.flatMap { lists(in: $0) }
    }

    /// 6: how many times a list is scrolled to its end before it is drawn, as a lazy list grows while it builds rows.
    private static let endPasses = 6
    /// 0.03 seconds: how long the run loop turns after each scroll, for the list to build what it shows.
    private static let endPassWait: TimeInterval = 0.03
  }
#endif
