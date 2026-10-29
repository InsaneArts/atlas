import SwiftUI

/// One of the sidebar's rows as a button, on its capsule highlight (``SidebarRowStyle``): selected while what it
/// chooses shows. VoiceOver reads its words as the button's, and says whether it is selected.
struct SidebarRow<Content: View>: View {
  /// Whether what the row chooses shows.
  private let isSelected: Bool
  /// The row's least height.
  private let height: CGFloat
  /// Chooses what the row shows.
  private let choose: @MainActor () -> Void
  /// What the row shows.
  private let content: Content

  /// Creates a row.
  ///
  /// - Parameters:
  ///   - isSelected: Whether what the row chooses shows.
  ///   - height: The row's least height; a session's 44 by default.
  ///   - choose: Chooses what the row shows.
  ///   - content: What the row shows.
  init(
    isSelected: Bool,
    height: CGFloat = ViewerLayout.sidebarRowHeight,
    choose: @escaping @MainActor () -> Void,
    @ViewBuilder content: () -> Content
  ) {
    self.isSelected = isSelected
    self.height = height
    self.choose = choose
    self.content = content()
  }

  /// The content as a button on its highlight.
  var body: some View {
    Button(action: choose) {
      content
    }
    .buttonStyle(SidebarRowStyle(isSelected: isSelected, height: height))
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
        SidebarSessionLabel(row: PreviewStates.sessionRows[0], ground: Color.groundSelected)
      }
      .frame(width: ViewerLayout.sidebarWidth)
      .background(Color.groundRecessed)
    }
  }
#endif
