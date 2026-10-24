import AppKit
import AtlasUI

/// The main menu's View, File and Helper items, and the ⋯ menu's, as the window in front answers them. Each item is
/// validated against the same state field its button draws (``ViewerPresenter``), never a rule of its own. While
/// this window holds control, the screen's key monitor sees every key first, so these shortcuts reach the helper
/// instead, all but Control-Shift-Escape.
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

  /// Pauses or resumes the picture.
  ///
  /// - Parameter sender: The menu item.
  func togglePicturePause(_ sender: Any?) { controller.send(.togglePause) }

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

  /// Copies the picture on screen.
  ///
  /// - Parameter sender: The menu item.
  @objc func copy(_ sender: Any?) { controller.readingLoop.copyPicture() }

  /// Saves the picture on screen.
  ///
  /// - Parameter sender: The menu item.
  func savePicture(_ sender: Any?) { controller.readingLoop.savePicture() }

  /// Live, every five seconds, or paused, by the item's tag.
  ///
  /// - Parameter sender: The menu item.
  func choosePicture(_ sender: Any?) {
    guard let choice = ((sender as? NSMenuItem)?.tag).flatMap(ViewerPictureChoice.init(rawValue:)) else { return }
    controller.choosePicture(choice)
  }

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
      item.title = state.top.actions.pause?.isOn == true ? ViewerWords.resumePictureItem : ViewerWords.pausePictureItem
      return true
    case #selector(ViewerMenuActions.choosePicture(_:)):
      item.state = item.tag == menu.picture.rawValue ? .on : .off
      return true
    case #selector(ViewerMenuActions.toggleActualSize(_:)):
      item.state = state.screen.actualSize == nil ? .off : .on
      return menu.hasPicture
    case #selector(ViewerMenuActions.toggleControl(_:)):
      return validateControl(item, state.top.actions.control) && !alertUp
    case #selector(ViewerMenuActions.toggleAgent(_:)):
      return validateAgent(item, state.top.actions.agent) && !alertUp
    case #selector(NSText.copy(_:)), #selector(ViewerMenuActions.savePicture(_:)):
      return menu.hasPicture
    case #selector(HelperMenuActions.askRecover(_:)): return menu.helper.canRecover && !alertUp
    case #selector(HelperMenuActions.askRestart(_:)): return menu.helper.canRestart && !alertUp
    case #selector(HelperMenuActions.askSwitch(_:)): return menu.helper.canSwitch && !alertUp
    case #selector(HelperMenuActions.askLogOut(_:)): return menu.helper.canLogOut && !alertUp
    default: return super.validateMenuItem(item)
    }
  }

  /// Take Control or Give Back, as the white capsule reads; while it reads Recover…, or is not drawn, Take Control
  /// waits.
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
    item.title = control.event == .giveBack ? ViewerWords.giveBack : ViewerWords.takeControl
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
