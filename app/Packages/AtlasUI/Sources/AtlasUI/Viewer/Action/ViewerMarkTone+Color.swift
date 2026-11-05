import SwiftUI

extension ViewerMarkTone {
  /// The mark's colour: blue when it worked, orange when it did not or may not have finished, grey when nothing was
  /// sent.
  var color: Color {
    switch self {
    case .worked: .markWorked
    case .failed, .uncertain: .markFailed
    case .notSent: .markQuiet
    }
  }

  /// The soft glow around the mark, its colour faint.
  var glow: Color {
    color.opacity(ViewerLayout.markGlowOpacity)
  }

  /// Whether its ring is dashed: for an action that did not work or may not have finished.
  var isDashed: Bool {
    self == .failed || self == .uncertain
  }

  /// The colour an outcome's words take in the details beside a large picture.
  var textColor: Color {
    switch self {
    case .worked: .success
    case .failed, .uncertain: .attention
    case .notSent: .markBubbleDim
    }
  }
}
