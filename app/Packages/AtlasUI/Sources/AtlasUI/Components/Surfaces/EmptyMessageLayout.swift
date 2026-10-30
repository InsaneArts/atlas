import SwiftUI

/// The message's own measures for compact windows and the waiting stage.
enum EmptyMessageLayout {
  /// 30: the orbit's height in the main window; only an empty message draws an orbit this large.
  static let orbit: CGFloat = 30
  /// 26: the orbit's height in a small window.
  static let smallOrbit: CGFloat = 26
  /// 380: the widest the sentence runs in the main window.
  static let width: CGFloat = 380
  /// 360: the widest it runs in a small window.
  static let smallWidth: CGFloat = 360
  /// 340: the widest the line runs on the stage.
  static let stageWidth: CGFloat = 340
  /// 18: under the mark and over the button on the stage.
  static let stageGap: CGFloat = 18
  /// 52: a pane's symbol tile on the stage.
  static let symbolTile: CGFloat = 52
  /// 14: its corner.
  static let symbolTileRadius: CGFloat = 14
}
