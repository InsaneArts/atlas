import AtlasUI

/// The commands the window's menus offer: the stage's, a session's and the helper line's. Each does what the main
/// menu's item for it does, with its guards.
extension ViewerController {
  /// A command chosen from one of the window's menus.
  ///
  /// - Parameter command: The command.
  func perform(_ command: ViewerCommand) {
    // Unstructured, as `send` is the view's synchronous callback: each async command runs in a task of its own.
    switch command {
    case .copyPicture: readingLoop.copyPicture()
    case .savePicture: readingLoop.savePicture()
    case .toggleActualSize: toggleActualSize()
    case .togglePause: if ViewerPresenter.pause(snapshot) != nil { togglePause() }
    case .chooseRate(let index):
      guard ViewerPictureRate.allCases.indices.contains(index) else { return }
      readingLoop.setRate(ViewerPictureRate.allCases[index])
    case .checkAgain: Task { await checkAgain() }
    case .openSettings: dependencies.openSettings()
    case .askRecover: ask(.recover)
    case .askSwitch: ask(.switchToHelper)
    case .askLogOut: ask(.logOut)
    case .askCleanup(let key): Task { await askCleanup(key) }
    case .sendKey(let index): sendKey(at: index)
    case .showActivity(let key): show(.activity, of: key)
    case .showScreenshots(let key): show(.screenshots, of: key)
    }
  }
}
