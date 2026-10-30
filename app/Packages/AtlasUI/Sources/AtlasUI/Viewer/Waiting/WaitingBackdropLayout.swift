import SwiftUI

/// The backdrop's own measures.
enum WaitingBackdropLayout {
  /// 48: how many pixels across the shrunk picture keeps, about one for every 30 points of a wide stage, so
  /// drawn back up it blurs about as much as a 14-point blur.
  static let sampleWidth = 48
  /// 8: bits for each colour of a shrunk pixel.
  static let bitsPerComponent = 8
}
