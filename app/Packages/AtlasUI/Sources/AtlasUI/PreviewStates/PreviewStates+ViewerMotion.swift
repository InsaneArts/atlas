#if DEBUG
  import SwiftUI

  extension PreviewStates {
    /// The main window's transitions drawn as key frames, each spaced along the move that travels furthest, or a
    /// list's change along its beats: the window's own, then Activity's.
    @MainActor public static var viewerMotion: [ViewerMotionPose] {
      windowMotion + activityMotion
    }

    /// The sidebar hiding, the Screen turning into Activity with the picture shrinking into its corner, Give Back
    /// turning the footer back into Take Control, and Ended opening.
    @MainActor private static var windowMotion: [ViewerMotionPose] {
      [
        ViewerMotionPose(
          "sidebar-collapse",
          from: viewerWorking,
          to: viewer(viewerWorking, applying: .toggleSidebar),
          size: viewerSize,
          lead: .spring(Motion.Springs.slide)
        ),
        ViewerMotionPose(
          "activity-zoom",
          from: viewerWorking,
          to: viewer(viewerWorking, applying: .show(.activity)),
          size: viewerSize,
          lead: .spring(Motion.Springs.zoom)
        ),
        ViewerMotionPose(
          "give-back",
          from: viewerControl,
          to: viewer(viewerControl, applying: .giveBack),
          size: viewerSize,
          lead: .spring(Motion.Springs.state)
        ),
        ViewerMotionPose(
          "ended-open",
          from: viewerSidebarEndedFolded,
          to: viewer(viewerSidebarEndedFolded, applying: .toggleEnded),
          size: viewerSize,
          lead: .spring(Motion.Springs.state)
        ),
      ]
    }

    /// Activity's: the Chrome run opening and folding again as the person asks, and the live session folding into its
    /// pictures and opening out of them again, each in tenths; a new run arriving at the top of the
    /// live session; and the Problems filter chosen, which keeps only the run with the failed press.
    @MainActor private static var activityMotion: [ViewerMotionPose] {
      [
        ViewerMotionPose(
          "run-open",
          from: viewerActivity,
          to: viewerActivityOpened,
          size: viewerSize,
          lead: .spring(Motion.Springs.state),
          steps: ViewerMotionPose.tenths,
          animation: { ActivityMotion.fold(reduceMotion: false) }
        ),
        ViewerMotionPose(
          "run-fold",
          from: viewerActivityOpened,
          to: viewerActivity,
          size: viewerSize,
          lead: .spring(Motion.Springs.state),
          steps: ViewerMotionPose.tenths,
          animation: { ActivityMotion.fold(reduceMotion: false) }
        ),
        ViewerMotionPose(
          "session-fold",
          from: viewerActivity,
          to: viewerActivitySessionFolded,
          size: viewerSize,
          lead: .spring(Motion.Springs.state),
          steps: ViewerMotionPose.tenths,
          animation: { ActivityMotion.fold(reduceMotion: false) }
        ),
        ViewerMotionPose(
          "session-open",
          from: viewerActivitySessionFolded,
          to: viewerActivity,
          size: viewerSize,
          lead: .spring(Motion.Springs.state),
          steps: ViewerMotionPose.tenths,
          animation: { ActivityMotion.fold(reduceMotion: false) }
        ),
        ViewerMotionPose(
          "run-arrive",
          from: viewerActivity,
          to: viewerActivityNewRun,
          size: viewerSize,
          lead: .list(Motion.Springs.state)
        ),
        ViewerMotionPose(
          "filter-problems",
          from: viewerActivity,
          to: viewerActivityProblems,
          size: viewerSize,
          lead: .list(Motion.Springs.state),
          animation: { ActivityMotion.filter(reduceMotion: false) }
        ),
      ]
    }

    /// Activity with its live session folded into one box of its row and its newest pictures, as a folded session
    /// shows them.
    @MainActor static var viewerActivitySessionFolded: ViewerState {
      var state = toggling(viewerActivity) { $0.isOpen }
      guard case .activity(var activity) = state.pane else { return state }
      for day in activity.days.indices {
        for index in activity.days[day].sessions.indices where !activity.days[day].sessions[index].isOpen {
          let session = activity.days[day].sessions[index]
          guard session.strip == nil, let tiles = session.runs.first(where: { $0.strip != nil })?.strip?.tiles else {
            continue
          }
          activity.days[day].sessions[index].strip = ViewerPictureStrip(
            run: session.id,
            tiles: tiles,
            total: tiles.count + Self.foldedMore,
            opensSession: true
          )
        }
      }
      state.pane = .activity(activity)
      return state
    }

    /// 338: how many more pictures the folded live session holds than its strip shows.
    private static let foldedMore = 338

    /// `state` with the first session `pick` chooses opened or folded, as the person would.
    ///
    /// - Parameters:
    ///   - state: The Activity sample.
    ///   - pick: Which session.
    /// - Returns: The sample after.
    @MainActor private static func toggling(
      _ state: ViewerState,
      pick: (ViewerOutlineSession) -> Bool
    ) -> ViewerState {
      guard case .activity(let activity) = state.pane,
        let session = activity.days
          .flatMap(\.sessions)
          .first(where: pick)
      else { return state }
      return viewer(state, applying: .toggleSession(session.id, opens: !session.isOpen))
    }

  }
#endif
