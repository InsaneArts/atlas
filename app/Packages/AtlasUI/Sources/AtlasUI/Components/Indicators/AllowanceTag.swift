import SwiftUI

/// An allowance badge drawn as a tag: what let an agent's action act in a listed account. It has no motion of its own;
/// it comes and goes with its row.
struct AllowanceTag: View {
  /// The badge.
  let badge: AllowanceBadge

  /// The badge's words on their capsule, never truncated.
  var body: some View {
    Tag(text: badge.words, tone: tone)
      .fixedSize()
  }

  /// The tag's tone for the badge's.
  private var tone: Tag.Tone {
    switch badge.tone {
    case .accent: .accent
    case .quiet: .quiet
    }
  }
}
