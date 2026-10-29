import SwiftUI

/// A section's heading over its rows, in line with their icons: Needs you in the attention colour, Working and Idle in
/// grey.
struct SidebarSectionHeading: View {
  /// The section it heads.
  let section: ViewerSessionSection

  /// The heading.
  var body: some View {
    Text(section.title)
      .font(Typography.font(.caption, weight: .semibold))
      .foregroundStyle(section.tone == .attention ? Color.textAttention : Color.textTertiary)
      .lineLimit(1)
      .fixedSize()
      .padding(ViewerLayout.headingInset)
      .frame(width: ViewerLayout.pillWidth, alignment: .leading)
      .accessibilityAddTraits(.isHeader)
  }
}

#if DEBUG
  #Preview("Samples") {
    VStack(alignment: .leading, spacing: .zero) {
      ForEach(ViewerSessionSection.allCases, id: \.self) { SidebarSectionHeading(section: $0) }
    }
    .frame(width: ViewerLayout.sidebarWidth)
    .sampleSurface()
  }
#endif
