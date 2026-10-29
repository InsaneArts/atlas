import SwiftUI

/// A step's disc on its moment's line, with its kind's glyph: the content's colour, so the line breaks around it, for
/// a step that worked or was refused before anything was sent; blue while it runs; orange for one that did not work.
/// No halo and no ring. Its colour moves on a crossfade, as the live blue passes to the newest step.
struct ActivityStepDisc: View {
  /// What kind of thing the step did.
  let kind: ViewerActivityKind
  /// How it went.
  let mark: ViewerStepMark

  /// The glyph on its disc.
  var body: some View {
    // Only the colours take the crossfade, so the disc moves with its step as the list moves it.
    Image(systemName: kind.symbol.rawValue)
      .font(Typography.font(.caption, weight: .semibold))
      .animation(ActivityMotion.disc) { glyph in
        glyph.foregroundStyle(ink)
      }
      .frame(width: ViewerLayout.stepDisc, height: ViewerLayout.stepDisc)
      .background(
        Circle()
          .animation(ActivityMotion.disc) { disc in
            disc.foregroundStyle(fill)
          }
      )
      .accessibilityHidden(true)
  }

  /// The disc's paint.
  private var fill: Color {
    switch mark {
    case .quiet, .notSent: .fillContent
    case .live: .accent
    case .failed: .attention
    }
  }

  /// The glyph's paint.
  private var ink: Color {
    switch mark {
    case .quiet, .notSent: .textSecondary
    case .live: .onAccent
    case .failed: .onAttentionStrong
    }
  }
}
