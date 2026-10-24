#if DEBUG
  extension PreviewStates {
    /// The main window sample an event leads to, so the Interactive preview answers clicks as the app would.
    ///
    /// - Parameters:
    ///   - state: The sample shown.
    ///   - event: What was clicked.
    /// - Returns: The sample to show next.
    @MainActor public static func viewer(_ state: ViewerState, applying event: ViewerEvent) -> ViewerState {
      switch event {
      case .selectTab(let tab):
        return tabSample(tab)
      case .toggleDetails:
        return state.details == nil ? viewerDetails : viewerIdle
      case .togglePause:
        return state.top.actions.pause?.isOn == true ? viewerWorking : viewerPaused
      case .takeControl:
        return viewerControl
      case .giveBack, .alertConfirm, .alertCancel, .checkAgain:
        return viewerWorking
      case .stopAgent:
        return viewerStopped
      case .resumeAgent:
        return viewerWorking
      case .askRecover:
        return viewerNeedsYou
      case .askSwitch:
        return viewerSwitch
      case .askLogOut:
        return viewerLogOut
      case .askRestart:
        return viewerRestart
      case .showKeys, .showMore, .copyShot, .showShotInFinder, .thumbnailNeeded, .shotColumns, .moveCorner:
        return state
      case .showWindow:
        return viewerShowOnScreen
      case .clearOutline:
        return viewerIdle
      case .searchActivity, .filterActivity, .searchApps, .selectShot, .openShot, .toggleShot, .previousShot,
        .nextShot, .shotAbove, .shotBelow, .clearShotSelection, .closeShot:
        return viewerPane(state, applying: event)
      }
    }

    /// The sample a tab shows.
    ///
    /// - Parameter tab: The tab picked.
    /// - Returns: Its sample.
    @MainActor private static func tabSample(_ tab: ViewerTab) -> ViewerState {
      switch tab {
      case .screen: viewerWorking
      case .activity: viewerActivity
      case .apps: viewerApps
      case .screenshots: viewerShots
      }
    }
  }
#endif
