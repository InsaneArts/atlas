#if DEBUG
  import SwiftUI

  extension PreviewStates {
    /// The main window's transitions drawn as key frames, each spaced along the move that travels furthest, or a
    /// list's change along its beats: the window's own, then Activity's and Apps'.
    @MainActor public static var viewerMotion: [ViewerMotionPose] {
      windowMotion + activityMotion + appsMotion
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
          "library-zoom",
          from: viewerWorking,
          to: viewer(viewerWorking, applying: .chooseLibrary(.activity)),
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

    /// Activity's: the Chrome run opening and folding again as the person asks, each in tenths; a new run arriving at
    /// the top of the live session; and the Problems filter chosen, which keeps only the run with the failed press.
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

    /// Apps': Opie coming to the front over Safari, Notes opening and quitting, and Notes moving from Already open to
    /// the agents' apps once the helper learns an agent opened it.
    @MainActor private static var appsMotion: [ViewerMotionPose] {
      [
        ViewerMotionPose(
          "apps-front",
          from: viewerAppsSafariFront,
          to: viewerApps,
          size: viewerSize,
          lead: .spring(Motion.Springs.state)
        ),
        ViewerMotionPose(
          "apps-open",
          from: viewerAppsBeforeNotes,
          to: viewerApps,
          size: viewerSize,
          lead: .list(Motion.Springs.state)
        ),
        ViewerMotionPose(
          "apps-quit",
          from: viewerApps,
          to: viewerAppsBeforeNotes,
          size: viewerSize,
          lead: .list(Motion.Springs.state)
        ),
        ViewerMotionPose(
          "apps-move",
          from: viewerAppsNotesAlreadyOpen,
          to: viewerApps,
          size: viewerSize,
          lead: .list(Motion.Springs.state)
        ),
      ]
    }
  }
#endif
