import SwiftUI

/// The shake's horizontal offsets, shrinking as the motion settles.
enum ShakeLayout {
  /// 5: the first, furthest swing.
  static let far: CGFloat = 5
  /// 3: the second, nearer swing.
  static let near: CGFloat = 3
  /// 5: the shake's steps.
  static let steps = 5
  /// One fifth of the shake.
  static let step = Motion.Duration.shake.seconds / Double(steps)
}
