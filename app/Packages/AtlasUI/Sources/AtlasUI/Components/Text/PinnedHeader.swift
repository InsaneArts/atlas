import SwiftUI

/// A group's header pinned over the list scrolling under it: its label, small and quiet, on a nearly solid
/// ground so the rows passing under it never show through its words.
struct PinnedHeader: View {
  /// The group's label, such as "Now" or "Today".
  let title: String

  /// The label on its pinned ground.
  var body: some View {
    GroupLabel(text: title)
      .padding(.top, Spacing.sPlus)
      .padding(.bottom, Spacing.xsPlus)
      .padding(.horizontal, Spacing.l)
      .frame(maxWidth: .infinity, alignment: .leading)
      .background(Color.fillPinned)
  }
}

#if DEBUG
  #Preview("Samples") {
    VStack(spacing: .zero) {
      PinnedHeader(title: String("Now"))
      PinnedHeader(title: String("Earlier today"))
    }
    .frame(width: SampleWidth.column)
    .sampleSurface()
  }
#endif
