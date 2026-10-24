import SwiftUI

/// The small mark before a status sentence or a row's word (v3 D4): a spinner while work runs, an orange round "!"
/// for a problem, a green check once done. The spinner waits ``Motion/appear``'s delay before it shows.
struct OutcomeMark: View {
  /// What the mark says.
  let kind: StatusLine.Kind
  /// The mark's side, in points.
  var side = StatusLine.markSide

  /// The spinner, the round "!" or the check.
  var body: some View {
    Group {
      switch kind {
      case .working:
        Spinner(size: side)
          .appearsAfterDelay()
      case .problem:
        Image(systemName: SymbolName.attention.rawValue)
          .font(Typography.font(size: side))
          .foregroundStyle(Self.color(kind))
      case .done:
        Image(systemName: SymbolName.passed.rawValue)
          .font(Typography.font(size: side))
          .foregroundStyle(Self.color(kind))
      }
    }
    .frame(width: side, height: side)
    .accessibilityHidden(true)
  }

  /// The mark's colour: system orange for a problem, system green once done, the accent of the spinner's arc while
  /// work runs.
  ///
  /// - Parameter kind: What the mark says.
  /// - Returns: The colour.
  nonisolated static func color(_ kind: StatusLine.Kind) -> Color {
    switch kind {
    case .working: .accent
    case .problem: .attention
    case .done: .success
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    StatusLineSamples(.marks)
  }
#endif
