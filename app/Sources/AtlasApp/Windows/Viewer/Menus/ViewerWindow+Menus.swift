import AppKit
import AtlasUI

/// The main menu's View, File, Edit and Helper items, the Dock menu's and the ⋯ menu's, as the window in front answers
/// them. Each item is validated against the same state field its button draws (``ViewerPresenter``), never a rule of
/// its own. While this window holds control, the screen's key monitor sees every key first, so these shortcuts reach
/// the helper instead, all but Control-Shift-Escape.
extension ViewerWindow: ViewerMenuActions, HelperMenuActions {
  /// Shows the tab the item's tag names.
  ///
  /// - Parameter sender: The menu item.
  func selectTab(_ sender: Any?) {
    guard let tab = ((sender as? NSMenuItem)?.tag).flatMap(ViewerTab.init(menuTag:)) else { return }
    controller.send(.selectTab(tab))
  }

  /// Shows or hides Details.
  ///
  /// - Parameter sender: The menu item.
  func toggleDetails(_ sender: Any?) { controller.send(.toggleDetails) }

  /// Pauses or resumes the picture, as the pause button does; not while this window holds control, where the button
  /// is gone.
  ///
  /// - Parameter sender: The menu item.
  func togglePicturePause(_ sender: Any?) {
    guard controller.state.top.actions.pause != nil else { return }
    controller.send(.togglePause)
  }

  /// Draws the screen at actual size, or fits it again.
  ///
  /// - Parameter sender: The menu item.
  func toggleActualSize(_ sender: Any?) { controller.toggleActualSize() }

  /// Take Control or Give Back, as the white capsule says now.
  ///
  /// - Parameter sender: The menu item.
  func toggleControl(_ sender: Any?) {
    guard let control = controller.state.top.actions.control, control.isEnabled,
      control.event == .takeControl || control.event == .giveBack
    else { return }
    controller.send(control.event)
  }

  /// Stop Agent or Resume Agent, as the top row offers it now.
  ///
  /// - Parameter sender: The menu item.
  func toggleAgent(_ sender: Any?) {
    switch controller.state.top.actions.agent {
    case .stop: controller.send(.stopAgent)
    case .resume(let isEnabled) where isEnabled: controller.send(.resumeAgent)
    default: return
    }
  }

  /// Copies what the window shows: the screenshot shown large or selected on the Screenshots tab, else the picture.
  ///
  /// - Parameter sender: The menu item.
  @objc func copy(_ sender: Any?) {
    if controller.moreMenu.actsOnShot {
      controller.send(.copyShot)
    } else {
      controller.readingLoop.copyPicture()
    }
  }

  /// Saves what the window shows: the screenshot shown large or selected on the Screenshots tab, else the picture.
  ///
  /// - Parameter sender: The menu item.
  func savePicture(_ sender: Any?) {
    if controller.moreMenu.actsOnShot {
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

  /// Find…: the search field of the tab shown takes the focus.
  ///
  /// - Parameter sender: The menu item.
  func findInTab(_ sender: Any?) { controller.panesModel.find() }

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
    let state = controller.state
    let menu = controller.moreMenu
    let alertUp = state.alert != nil
    switch item.action {
    case #selector(ViewerMenuActions.selectTab(_:)):
      item.state = item.tag == state.top.tabs.selected.menuTag ? .on : .off
      return state.top.tabs.tabs.contains { $0.id.menuTag == item.tag }
    case #selector(ViewerMenuActions.toggleDetails(_:)):
      item.title = state.top.actions.details.isOn ? ViewerWords.hideDetailsItem : ViewerWords.showDetailsItem
      return true
    case #selector(ViewerMenuActions.togglePicturePause(_:)):
      item.title = controller.session.paused ? Words.resumePictureItem : Words.pausePictureItem
      return state.top.actions.pause != nil
    case #selector(ViewerMenuActions.chooseRate(_:)):
      item.state = item.tag == menu.rate.menuTag ? .on : .off
      return true
    case #selector(ViewerMenuActions.toggleActualSize(_:)):
      item.state = menu.actualSize ? .on : .off
      return menu.hasPicture
    case #selector(ViewerMenuActions.toggleControl(_:)):
      return validateControl(item, state.top.actions.control) && !alertUp
    case #selector(ViewerMenuActions.toggleAgent(_:)):
      return validateAgent(item, state.top.actions.agent) && !alertUp
    case #selector(NSText.copy(_:)):
      return menu.actsOnShot || menu.hasPicture
    case #selector(ViewerMenuActions.savePicture(_:)):
      item.title = menu.actsOnShot ? ViewerWords.saveScreenshot : ViewerWords.savePicture
      return menu.actsOnShot || menu.hasPicture
    case #selector(ViewerMenuActions.findInTab(_:)):
      return searchable(state.pane)
    default:
      return validateHelper(item, alertUp: alertUp) ?? super.validateMenuItem(item)
    }
  }

  /// The helper's commands, as the window offers them, with the helper's name in Switch and Log Out once it is known.
  ///
  /// - Parameters:
  ///   - item: The item.
  ///   - alertUp: Whether an alert is over the window.
  /// - Returns: Whether it can be chosen, or nil for an item that is not one of them.
  private func validateHelper(_ item: NSMenuItem, alertUp: Bool) -> Bool? {
    let helper = controller.snapshot.helperMenu
    let name = controller.reading.hasStatus ? ViewerPresenter.account(controller.reading.status) : nil
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

  /// Whether the pane shown has a search field to find in.
  ///
  /// - Parameter pane: The pane shown.
  /// - Returns: Whether Find can focus one.
  private func searchable(_ pane: ViewerPane) -> Bool {
    switch pane {
    case .activity(let activity): activity.isSearchable
    case .apps(let apps): apps.isSearchable
    case .screen, .screenshots: false
    }
  }

  /// Take Control or Give Back Control, as the white capsule reads; while it reads Recover…, or is not drawn, Take
  /// Control waits.
  ///
  /// - Parameters:
  ///   - item: The item.
  ///   - control: The white capsule; nil while it is not drawn.
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

  /// Stop Agent or Resume Agent, as the top row offers it.
  ///
  /// - Parameters:
  ///   - item: The item.
  ///   - agent: The top row's agent button.
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
