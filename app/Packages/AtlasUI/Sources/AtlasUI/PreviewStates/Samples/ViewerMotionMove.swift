#if DEBUG
  import SwiftUI

  /// One of the main window's transitions the motion lab plays (``ViewerMotionLab``), the window's own and then
  /// Activity's and Apps': each turns the window one way, and played again turns it back, so a second press straight
  /// after the first reverses it mid-way. A list's move shows that list first.
  enum ViewerMotionMove: String, CaseIterable, Identifiable {
    /// The sidebar hiding or showing.
    case sidebar = "Toggle sidebar"
    /// A session moving from Working to Idle, or back.
    case session = "Move a session"
    /// Needs you appearing at the top, or going.
    case needsYou = "Needs you"
    /// Ended opening or folding.
    case ended = "Ended"
    /// A session's badge stepping from acting to waiting to needs you to none.
    case badge = "Badge cycle"
    /// The Screen turning into Activity, the picture shrinking into its corner, or back.
    case library = "Library and Screen"
    /// Activity giving way to Screenshots sideways, or back.
    case lists = "List to list"
    /// Take Control, or Give Back.
    case control = "Take Control"
    /// Stop, or Resume.
    case agent = "Stop and Resume"
    /// The footer's capsules turning into circles, or back.
    case compact = "Compact footer"
    /// Activity's Notes folding, or opening again.
    case moment = "Fold a moment"
    /// TextEdit's "Show 1 more" opening its last step, or the list before it.
    case more = "Show more"
    /// A new step arriving at the top of the live moment, or the list before it.
    case step = "New step"
    /// A new app's moment arriving over the live one, or the list before it.
    case newMoment = "New moment"
    /// Activity's Problems filter chosen, or All again.
    case problems = "Problems filter"
    /// Opie coming to the front of Apps over Safari, or Safari again.
    case front = "Front app"
    /// Notes opening in Apps, or quitting.
    case app = "Open or quit"
    /// Notes moving from Already open to the agents' apps, or back.
    case group = "Move group"

    /// The move's title, which identifies it.
    var id: String { rawValue }

    /// The animation the move runs in, as the view wraps the person's own change: opening or folding a moment, "Show N
    /// more" and choosing a filter; none for the rest, which only the views' own animations move, as when the window reads.
    ///
    /// - Parameters:
    ///   - state: The window as it is.
    ///   - reduceMotion: Whether the Reduce Motion setting is on.
    /// - Returns: The animation, or nil.
    func animation(on state: ViewerState, reduceMotion: Bool) -> Animation? {
      switch self {
      case .moment, .more: ActivityMotion.fold(reduceMotion: reduceMotion)
      case .problems: ActivityMotion.filter(reduceMotion: reduceMotion)
      default: nil
      }
    }

    /// The window after this move from `state`.
    ///
    /// - Parameter state: The window as it is.
    /// - Returns: The window after the move.
    @MainActor func played(on state: ViewerState) -> ViewerState {
      var next = state
      switch self {
      case .sidebar:
        next.sidebar.mode = state.sidebar.mode == .hidden ? .full : .hidden
      case .session:
        next.sidebar.sessions.groups = Self.moved(state.sidebar.sessions.groups)
      case .needsYou:
        next.sidebar.sessions.groups = Self.needingYou(state.sidebar.sessions.groups)
      case .ended:
        next = PreviewStates.viewer(state, applying: .toggleEnded)
      case .badge:
        next.sidebar.sessions.groups = Self.cycled(state.sidebar.sessions.groups)
      case .library:
        next.pane = state.pane.place == .screen ? PreviewStates.viewerActivity.pane : PreviewStates.viewerWorking.pane
        next.mini = next.pane.place == .screen ? nil : PreviewStates.viewerActivity.mini
      case .lists:
        next.pane = state.pane.place == .activity ? PreviewStates.viewerShots.pane : PreviewStates.viewerActivity.pane
        next.mini = PreviewStates.viewerActivity.mini
      case .control:
        let isInControl = state.footer.controls.control?.look == .accent
        let sample = isInControl ? PreviewStates.viewerWorking : PreviewStates.viewerControl
        next.header = sample.header
        next.footer.latest = sample.footer.latest
        next.footer.controls = sample.footer.controls
        next.screen = sample.screen
      case .agent:
        let isStopped = state.footer.controls.agent == .resume(isEnabled: true)
        next.footer.controls = (isStopped ? PreviewStates.viewerWorking : PreviewStates.viewerStopped).footer.controls
      case .compact:
        next.footer.fit = state.footer.fit == .full ? .compact : .full
      case .moment:
        next = PreviewStates.viewer(
          Self.showing(PreviewStates.viewerActivity, on: state),
          applying: .toggleMoment(Self.notes, opens: Self.isFolded(state))
        )
      case .more:
        next = Self.toggled(state, PreviewStates.viewerActivityOpened, PreviewStates.viewerActivityEveryStep)
      case .step:
        next = Self.toggled(state, PreviewStates.viewerActivityBeforeStep, PreviewStates.viewerActivity)
      case .newMoment:
        next = Self.toggled(state, PreviewStates.viewerActivity, PreviewStates.viewerActivityNewMoment)
      case .problems:
        next = Self.toggled(state, PreviewStates.viewerActivity, PreviewStates.viewerActivityProblems)
      case .front:
        next = Self.toggled(state, PreviewStates.viewerAppsSafariFront, PreviewStates.viewerApps)
      case .app:
        next = Self.toggled(state, PreviewStates.viewerAppsBeforeNotes, PreviewStates.viewerApps)
      case .group:
        next = Self.toggled(state, PreviewStates.viewerAppsNotesAlreadyOpen, PreviewStates.viewerApps)
      }
      return next
    }

    /// Notes' moment in the Activity sample.
    private static let notes = "session:|n1"

    /// Whether Activity shows with Notes folded, so the moment move opens it; not while another place shows, where the
    /// move first shows Activity with Notes open.
    ///
    /// - Parameter state: The window as it is.
    /// - Returns: Whether Notes is folded.
    private static func isFolded(_ state: ViewerState) -> Bool {
      guard case .activity(let activity) = state.pane else { return false }
      return activity.items.contains { item in
        guard case .moment(let moment) = item else { return false }
        return moment.id == notes && !moment.isOpen
      }
    }

    /// `state` showing `sample`'s list with its small screen, unless it shows that place already.
    ///
    /// - Parameters:
    ///   - sample: The sample whose list to show.
    ///   - state: The window as it is.
    /// - Returns: The window.
    private static func showing(_ sample: ViewerState, on state: ViewerState) -> ViewerState {
      guard state.pane.place != sample.pane.place else { return state }
      var next = state
      next.pane = sample.pane
      next.mini = sample.mini
      next.sidebar.library = sample.sidebar.library
      return next
    }

    /// `state` showing `second`'s list while it shows `first`'s, else `first`'s, each with its small screen.
    ///
    /// - Parameters:
    ///   - state: The window as it is.
    ///   - first: One end of the move.
    ///   - second: The other.
    /// - Returns: The window.
    private static func toggled(_ state: ViewerState, _ first: ViewerState, _ second: ViewerState) -> ViewerState {
      let sample = state.pane == first.pane ? second : first
      var next = state
      next.pane = sample.pane
      next.mini = sample.mini
      next.sidebar.library = sample.sidebar.library
      return next
    }

    /// The groups with the first working session moved to Idle, or the first idle one back to Working.
    ///
    /// - Parameter groups: The sections as they stand.
    /// - Returns: The sections after the move.
    private static func moved(_ groups: [ViewerSessionGroup]) -> [ViewerSessionGroup] {
      var working = groups.first { $0.section == .working }?.rows ?? []
      var idle = groups.first { $0.section == .idle }?.rows ?? []
      if working.count > 1 {
        var row = working.removeFirst()
        row.badge = nil
        idle.insert(row, at: 0)
      } else if !idle.isEmpty {
        var row = idle.removeFirst()
        row.badge = .acting
        working.insert(row, at: 0)
      }
      return regrouped(groups, working: working, idle: idle)
    }

    /// The groups with the last working session under Needs you, or back under Working.
    ///
    /// - Parameter groups: The sections as they stand.
    /// - Returns: The sections after the move.
    private static func needingYou(_ groups: [ViewerSessionGroup]) -> [ViewerSessionGroup] {
      var working = groups.first { $0.section == .working }?.rows ?? []
      let idle = groups.first { $0.section == .idle }?.rows ?? []
      if var needing = groups.first(where: { $0.section == .needsYou })?.rows.first {
        needing.badge = .acting
        working.append(needing)
        return regrouped(groups, working: working, idle: idle)
      }
      guard var row = working.popLast() else { return groups }
      row.badge = .needsYou
      return [ViewerSessionGroup(.needsYou, rows: [row])] + regrouped(groups, working: working, idle: idle)
    }

    /// The groups with the first session's badge one step on: acting, waiting, needs you, then none.
    ///
    /// - Parameter groups: The sections as they stand.
    /// - Returns: The sections after the step.
    private static func cycled(_ groups: [ViewerSessionGroup]) -> [ViewerSessionGroup] {
      guard var first = groups.first, var row = first.rows.first else { return groups }
      let steps: [SessionBadge?] = [.acting, .waiting, .needsYou, nil]
      let index = steps.firstIndex { $0 == row.badge } ?? steps.count - 1
      row.badge = steps[(index + 1) % steps.count]
      first.rows[0] = row
      return [first] + groups.dropFirst()
    }

    /// `groups` with Working and Idle holding `working` and `idle`, empty sections dropped and the rest kept.
    ///
    /// - Parameters:
    ///   - groups: The sections as they stood.
    ///   - working: Working's rows.
    ///   - idle: Idle's rows.
    /// - Returns: The sections.
    private static func regrouped(
      _ groups: [ViewerSessionGroup],
      working: [ViewerSessionRow],
      idle: [ViewerSessionRow]
    ) -> [ViewerSessionGroup] {
      let rest = groups.filter { ![.needsYou, .working, .idle].contains($0.section) }
      let live = [ViewerSessionGroup(.working, rows: working), ViewerSessionGroup(.idle, rows: idle)]
      return live.filter { !$0.rows.isEmpty } + rest
    }
  }
#endif
