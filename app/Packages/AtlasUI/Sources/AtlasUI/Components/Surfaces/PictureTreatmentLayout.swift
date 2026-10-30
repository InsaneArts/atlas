import SwiftUI

/// The treatment's own measures for dimmed and paused pictures.
enum PictureTreatmentLayout {
  /// 0.55: how much colour a dimmed picture keeps.
  static let dimSaturation = 0.55
  /// 0.4: the black over a dimmed picture, which leaves it at 0.6 brightness.
  static let dimShade = 0.4
  /// 0.5: how much colour a paused picture keeps.
  static let pausedSaturation = 0.5
  /// 0.3: the black over a paused picture, which leaves it at 0.7 brightness.
  static let pausedShade = 0.3
}
