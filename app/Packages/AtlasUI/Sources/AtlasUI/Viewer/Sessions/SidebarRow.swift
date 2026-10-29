import SwiftUI

/// One of the sessions column's rows as a button, on its inset, rounded highlight (``SidebarRowStyle``): selected
/// while its session's work shows. VoiceOver reads its words as the button's and says whether it is selected.
struct SidebarRow<Content: View>: View {
  /// Whether its session's work is the one shown.
  private let isSelected: Bool
  /// Chooses its session.
  private let choose: @MainActor () -> Void
  /// What the row shows.
  private let content: Content

  /// Creates a row.
  ///
  /// - Parameters:
  ///   - isSelected: Whether its session's work is the one shown.
  ///   - choose: Chooses its session.
  ///   - content: What the row shows.
  init(isSelected: Bool, choose: @escaping @MainActor () -> Void, @ViewBuilder content: () -> Content) {
    self.isSelected = isSelected
    self.choose = choose
    self.content = content()
  }

  /// The content as a button on its highlight.
  var body: some View {
    Button(action: choose) {
      content
    }
    .buttonStyle(SidebarRowStyle(isSelected: isSelected))
    .accessibilityAddTraits(isSelected ? .isSelected : [])
  }
}

#if DEBUG
  #Preview("Interactive") {
    @Previewable @State var isSelected = true
    InteractivePreview {
      Toggle(String("Selected"), isOn: $isSelected)
    } content: {
      SidebarRow(isSelected: isSelected) {
        isSelected.toggle()
      } content: {
        Text(SessionSamples.claudeName)
          .font(Typography.font(.callout, weight: .semibold))
      }
      .frame(width: ViewerLayout.sidebarWidth)
    }
  }
#endif
