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

    /// Activity's: Notes folding and opening again, and TextEdit's "Show 1 more", as the person asks, each in tenths;
    /// a new step arriving at the top of the live
    /// moment, and a new app's moment over it; the Problems filter chosen, which keeps only the failed click and holds
    /// its moment open.
    @MainActor private static var activityMotion: [ViewerMotionPose] {
      [
        ViewerMotionPose(
          "moment-open",
          from: viewerActivityNotesFolded,
          to: viewer(viewerActivityNotesFolded, applying: .toggleMoment("session:|n1", opens: true)),
          size: viewerSize,
          lead: .spring(Motion.Springs.state),
          steps: ViewerMotionPose.tenths,
          animation: { ActivityMotion.fold(reduceMotion: false) }
        ),
        ViewerMotionPose(
          "moment-fold",
          from: viewerActivity,
          to: viewerActivityNotesFolded,
          size: viewerSize,
          lead: .spring(Motion.Springs.state),
          steps: ViewerMotionPose.tenths,
          animation: { ActivityMotion.fold(reduceMotion: false) }
        ),
        ViewerMotionPose(
          "moment-more",
          from: viewerActivityOpened,
          to: viewerActivityEveryStep,
          size: viewerSize,
          lead: .spring(Motion.Springs.state),
          steps: ViewerMotionPose.tenths,
          animation: { ActivityMotion.fold(reduceMotion: false) }
        ),
        ViewerMotionPose(
          "step-arrive",
          from: viewerActivityBeforeStep,
          to: viewerActivity,
          size: viewerSize,
          lead: .list(Motion.Springs.state)
        ),
        ViewerMotionPose(
          "moment-arrive",
          from: viewerActivity,
          to: viewerActivityNewMoment,
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
