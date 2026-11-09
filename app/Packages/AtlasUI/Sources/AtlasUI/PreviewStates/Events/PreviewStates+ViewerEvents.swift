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
      case .show(let place):
        return placeSample(place)
      case .chooseLibrary(let place):
        return placeSample(place == state.pane.place ? .screen : place)
      case .menu(.togglePause):
        return state.footer.controls.pause?.isOn == true ? viewerWorking : viewerPaused
      case .menu(.toggleActualSize):
        return state.screen.actualSize == nil ? viewerActualSizePaused : viewerPaused
      case .menu(.copyPicture), .menu(.savePicture), .menu(.chooseRate), .menu(.sendKey), .menu(.openSettings):
        return state
      case .menu(.checkAgain):
        return viewerWorking
      case .menu(.askRecover):
        return viewerNeedsYou
      case .menu(.askSwitch):
        return viewerSwitch
      case .menu(.askLogOut):
        return viewerLogOut
      case .menu(.askCleanup):
        return viewerCleanUp
      case .menu(.showActivity):
        return viewerActivityOneSession
      case .togglePause:
        return state.footer.controls.pause?.isOn == true ? viewerWorking : viewerPaused
      case .toggleSound:
        var next = state
        next.footer.controls.sound = HelperSoundButton(isMuted: !state.footer.controls.sound.isMuted)
        return next
      case .answerAsk:
        var next = state
        next.screen.ask = nil
        return next
      case .signInDone, .signInCancel:
        var next = state
        next.screen.signingIn = nil
        next.header.tag = nil
        return next
      case .takeControl:
        return viewerControl
      case .giveBack, .alertConfirm, .alertCancel:
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
      case .copyPicture, .showPictureInFinder, .previousPicture, .nextPicture, .thumbnailNeeded, .moveCorner,
        .openSettings, .showDaySessions:
        return state
      case .toggleSidebar, .dismissSidebarOverlay, .toggleEnded, .openSearch, .search, .closeSearch:
        return viewerSidebar(state, applying: event)
      case .chooseSession(.all):
        return viewerActivityBySession
      case .chooseSession(.one):
        return viewerActivityOneSession
      case .showWindow:
        return viewerShowOnScreen
      case .clearOutline:
        return viewerIdle
      case .filterActivity, .toggleSession, .toggleRun, .showRunPictures, .showAllRunPictures, .hideRunPictures,
        .openShot, .openAction, .pictureSide, .closePicture, .openReplay, .replayFrame, .previousFrame, .nextFrame,
        .togglePlay, .closeReplay:
        return viewerPane(state, applying: event)
      }
    }

    /// The sample a place shows.
    ///
    /// - Parameter place: The place picked.
    /// - Returns: Its sample.
    @MainActor private static func placeSample(_ place: ViewerPlace) -> ViewerState {
      switch place {
      case .screen: viewerWorking
      case .activity: viewerActivity
      case .apps: viewerApps
      }
    }
  }
#endif
