import AppKit
import AtlasUI

/// The main menu's View, Session, File, Edit and Helper items and the Dock menu's, as the window in front answers them.
/// Each item is validated against the same state its button, row or menu item draws (``ViewerPresenter``), never a rule
/// of its own. While this window holds control, the screen's key monitor sees every key first, so these shortcuts reach
/// the helper instead, all but Control-Shift-Escape.
extension ViewerWindow: ViewerMenuActions, HelperMenuActions {
  /// Shows the place the item's tag names.
  ///
  /// - Parameter sender: The menu item.
  func showPlace(_ sender: Any?) {
    guard let place = ((sender as? NSMenuItem)?.tag).flatMap(ViewerPlace.init(menuTag:)) else { return }
    controller.send(.show(place))
  }

  /// Shows or hides the sidebar.
  ///
  /// - Parameter sender: The menu item.
  func toggleSidebar(_ sender: Any?) { controller.send(.toggleSidebar) }

  /// Pauses or resumes the picture, as the pause button does; not while this window holds control, where the button
  /// is gone.
  ///
  /// - Parameter sender: The menu item.
  func togglePicturePause(_ sender: Any?) {
    guard ViewerPresenter.pause(controller.snapshot) != nil else { return }
    controller.send(.togglePause)
  }

  /// Draws the screen at actual size, or fits it again.
  ///
  /// - Parameter sender: The menu item.
  func toggleActualSize(_ sender: Any?) { controller.toggleActualSize() }

  /// Take Control or Give Back, as the main capsule says now.
  ///
  /// - Parameter sender: The menu item.
  func toggleControl(_ sender: Any?) {
    controller.toggleControl(present: present)
  }

  /// Stop Agent or Resume Agent, as the footer offers it now.
  ///
  /// - Parameter sender: The menu item.
  func toggleAgent(_ sender: Any?) {
    switch ViewerPresenter.controls(controller.snapshot).agent {
    case .stop: controller.send(.stopAgent)
    case .resume(let isEnabled) where isEnabled: controller.send(.resumeAgent)
    default: return
    }
  }

  /// Copies what the window shows: the screenshot shown large or selected in the Screenshots list, else the picture.
  ///
  /// - Parameter sender: The menu item.
  @objc func copy(_ sender: Any?) {
    if controller.pictureMenu.actsOnShot {
      controller.send(.copyShot)
    } else {
      controller.readingLoop.copyPicture()
    }
  }

  /// Saves what the window shows: the screenshot shown large or selected in the Screenshots list, else the picture.
  ///
  /// - Parameter sender: The menu item.
  func savePicture(_ sender: Any?) {
    if controller.pictureMenu.actsOnShot {
      controller.panesModel.saveShot()
    } else {
      controller.readingLoop.savePicture()
    }
  }

  /// Live or every five seconds, by the item's tag: how often pictures arrive, or will once the picture resumes.
  ///
  /// - Parameter sender: The menu item.
  func chooseRate(_ sender: Any?) {
    guard let rate = ((sender as? NSMenuItem)?.tag).flatMap(ViewerPictureRate.init(menuTag:)) else { return }
    controller.readingLoop.setRate(rate)
  }

  /// Find…: the sidebar's search takes the focus, on the Screen and on every list.
  ///
  /// - Parameter sender: The menu item.
  func findInTab(_ sender: Any?) { controller.find() }

  /// Sends the key the item's tag names to the helper, while this window holds control.
  ///
  /// - Parameter sender: The menu item.
  func sendKey(_ sender: Any?) {
    guard let tag = (sender as? NSMenuItem)?.tag else { return }
    controller.sendKey(at: tag)
  }

  /// Reads the helper's status now.
  ///
  /// - Parameter sender: The menu item.
  func checkAgain(_ sender: Any?) { controller.send(.menu(.checkAgain)) }

  /// Opens Settings on Helper.
  ///
  /// - Parameter sender: The menu item.
  func openHelperSettings(_ sender: Any?) { controller.send(.menu(.openSettings)) }

  /// Shows every session's work.
  ///
  /// - Parameter sender: The menu item.
  func chooseAllSessions(_ sender: Any?) { controller.chooseAllSessions() }

  /// Chooses the session over the one chosen.
  ///
  /// - Parameter sender: The menu item.
  func choosePreviousSession(_ sender: Any?) { controller.chooseSession(by: -1) }

  /// Chooses the session under the one chosen.
  ///
  /// - Parameter sender: The menu item.
  func chooseNextSession(_ sender: Any?) { controller.chooseSession(by: 1) }

  /// Lists every session that ended under Ended, or folds them again.
  ///
  /// - Parameter sender: The menu item.
  func toggleEnded(_ sender: Any?) { controller.send(.toggleEnded) }

  /// Asks before cleaning up the chosen session's work.
  ///
  /// - Parameter sender: The menu item.
  func askCleanupChosen(_ sender: Any?) { controller.askCleanupChosen() }

  /// Asks before recovering the helper.
  ///
  /// - Parameter sender: The menu item.
  func askRecover(_ sender: Any?) { controller.send(.askRecover) }

  /// Asks before restarting the helper.
  ///
  /// - Parameter sender: The menu item.
  func askRestart(_ sender: Any?) { controller.send(.askRestart) }

  /// Asks before switching to the helper.
  ///
  /// - Parameter sender: The menu item.
  func askSwitch(_ sender: Any?) { controller.send(.askSwitch) }

  /// Asks before logging the helper out.
  ///
  /// - Parameter sender: The menu item.
  func askLogOut(_ sender: Any?) { controller.send(.askLogOut) }

  /// Whether an item can be chosen now, with its title and tick set from the same state the window draws. The
  /// window's own items (Close, Minimize, Enter Full Screen) are the window's to validate, as for any window.
  ///
  /// - Parameter item: The item.
  /// - Returns: Whether it can be chosen.
  override func validateMenuItem(_ item: NSMenuItem) -> Bool {
    let snapshot = controller.snapshot
    switch item.action {
    case #selector(ViewerMenuActions.showPlace(_:)):
      item.state = item.tag == snapshot.session.place.menuTag ? .on : .off
      return ViewerPlace(menuTag: item.tag) != nil
    case #selector(ViewerMenuActions.toggleSidebar(_:)):
      item.title = ViewerPresenter.sidebarShows(snapshot) ? ViewerWords.hideSidebarItem : ViewerWords.showSidebarItem
      return true
    case #selector(ViewerMenuActions.sendKey(_:)):
      return snapshot.holdingControl && HelperKey.key(at: item.tag) != nil
    case #selector(ViewerMenuActions.togglePicturePause(_:)):
      item.title = snapshot.session.paused ? Words.resumePictureItem : Words.pausePictureItem
      return ViewerPresenter.pause(snapshot) != nil
    case #selector(ViewerMenuActions.chooseRate(_:)):
      item.state = item.tag == snapshot.rate.menuTag ? .on : .off
      return true
    case #selector(ViewerMenuActions.toggleActualSize(_:)):
      let menu = ViewerPresenter.pictureMenu(snapshot)
      item.state = menu.actualSize ? .on : .off
      return menu.hasPicture
    case #selector(ViewerMenuActions.toggleControl(_:)):
      return validateControl(item, ViewerPresenter.controls(snapshot).control) && ViewerPresenter.alert(snapshot) == nil
    case #selector(ViewerMenuActions.toggleAgent(_:)):
      return validateAgent(item, ViewerPresenter.controls(snapshot).agent) && ViewerPresenter.alert(snapshot) == nil
    case #selector(NSText.copy(_:)):
      let menu = ViewerPresenter.pictureMenu(snapshot)
      return menu.actsOnShot || menu.hasPicture
    case #selector(ViewerMenuActions.savePicture(_:)):
      let menu = ViewerPresenter.pictureMenu(snapshot)
      item.title = menu.actsOnShot ? ViewerWords.saveScreenshot : ViewerWords.savePicture
      return menu.actsOnShot || menu.hasPicture
    case #selector(ViewerMenuActions.findInTab(_:)):
      // Find opens the sidebar's search, which every place has.
      return true
    case #selector(ViewerMenuActions.checkAgain(_:)):
      return !snapshot.session.checkingAgain
    case #selector(ViewerMenuActions.openHelperSettings(_:)):
      return true
    default:
      return validateSession(item, snapshot: snapshot) ?? validateHelper(item, snapshot: snapshot)
        ?? super.validateMenuItem(item)
    }
  }

  /// The Session menu's items, from the sidebar the window draws: All Sessions while one session is chosen, Previous
  /// and Next while there is a row to step to, Show Ended Sessions ticked while Ended is open, and Clean Up while the
  /// chosen session's own menu offers it and no alert is up.
  ///
  /// - Parameters:
  ///   - item: The item.
  ///   - snapshot: What the window held when validation began.
  /// - Returns: Whether it can be chosen, or nil for an item that is not one of them.
  private func validateSession(_ item: NSMenuItem, snapshot: ViewerSnapshot) -> Bool? {
    switch item.action {
    case #selector(ViewerMenuActions.chooseAllSessions(_:)):
      return snapshot.shownSessions != .all
    case #selector(ViewerMenuActions.choosePreviousSession(_:)):
      return steps(-1, snapshot)
    case #selector(ViewerMenuActions.chooseNextSession(_:)):
      return steps(1, snapshot)
    case #selector(ViewerMenuActions.toggleEnded(_:)):
      item.state = snapshot.session.endedShown ? .on : .off
      return true
    case #selector(ViewerMenuActions.askCleanupChosen(_:)):
      let cleanup = ViewerPresenter.chosenCleanup(snapshot)
      return cleanup?.isEnabled == true && ViewerPresenter.alert(snapshot) == nil
    default:
      return nil
    }
  }

  /// Whether Previous Session or Next Session has a row to step to from the session shown, in the sidebar's order.
  ///
  /// - Parameters:
  ///   - offset: How many rows down; negative goes up.
  ///   - snapshot: What the window held when validation began.
  /// - Returns: Whether it steps.
  private func steps(_ offset: Int, _ snapshot: ViewerSnapshot) -> Bool {
    let sessions = ViewerPresenter.sessions(snapshot)
    return sessions.choice(offset, from: snapshot.shownSessions) != snapshot.shownSessions
  }

  /// The helper's commands, as the window offers them, with the helper's name in Switch and Log Out once it is known.
  ///
  /// - Parameters:
  ///   - item: The item.
  ///   - snapshot: What the window held when validation began.
  /// - Returns: Whether it can be chosen, or nil for an item that is not one of them.
  private func validateHelper(_ item: NSMenuItem, snapshot: ViewerSnapshot) -> Bool? {
    let helper = snapshot.helperMenu
    let name = snapshot.reading.hasStatus ? ViewerPresenter.account(snapshot.reading.status) : nil
    let alertUp = ViewerPresenter.alert(snapshot) != nil
    switch item.action {
    case #selector(HelperMenuActions.askRecover(_:)): return helper.canRecover && !alertUp
    case #selector(HelperMenuActions.askRestart(_:)): return helper.canRestart && !alertUp
    case #selector(HelperMenuActions.askSwitch(_:)):
      item.title = MenuWords.switchTo(name)
      return helper.canSwitch && !alertUp
    case #selector(HelperMenuActions.askLogOut(_:)):
      item.title = MenuWords.logOut(name)
      return helper.canLogOut && !alertUp
    default: return nil
    }
  }

  /// Take Control or Give Back Control, as the main capsule reads; while it reads Recover…, or is not drawn, Take
  /// Control waits.
  ///
  /// - Parameters:
  ///   - item: The item.
  ///   - control: The main capsule; nil while it is not drawn.
  /// - Returns: Whether it can be chosen.
  private func validateControl(_ item: NSMenuItem, _ control: ViewerControlButton?) -> Bool {
    guard let control else {
      item.title = ViewerWords.takeControl
      return false
    }
    let toggles = control.event == .takeControl || control.event == .giveBack
    item.title = control.event == .giveBack ? ViewerWords.giveBackItem : ViewerWords.takeControl
    return toggles && control.isEnabled
  }

  /// Stop Agent or Resume Agent, as the footer offers it.
  ///
  /// - Parameters:
  ///   - item: The item.
  ///   - agent: The footer's agent button.
  /// - Returns: Whether it can be chosen.
  private func validateAgent(_ item: NSMenuItem, _ agent: ViewerAgentButton?) -> Bool {
    switch agent {
    case .stop:
      item.title = ViewerWords.stopAgentItem
      return true
    case .resume(let isEnabled):
      item.title = ViewerWords.resumeAgentItem
      return isEnabled
    case nil:
      item.title = ViewerWords.stopAgentItem
      return false
    }
  }
}
