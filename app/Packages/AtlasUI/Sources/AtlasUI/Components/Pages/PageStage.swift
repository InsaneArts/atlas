import SwiftUI

/// Where a window shows one page at a time: a new page, one with a new id, fades in over the last as the last fades
/// out, starting on the frame it is asked for, and nothing slides; a change on the same page redraws it in place.
/// Whatever sits outside the stage, such as a footer, stays put while the pages change. Only opacity changes, so
/// Reduce Motion keeps the same fade.
struct PageStage<ID: Hashable, Content: View>: View {
  /// Which page shows: a new id is a new page.
  private let id: ID
  /// The page.
  private let content: Content

  /// Creates a stage showing one page.
  ///
  /// - Parameters:
  ///   - id: Which page shows: a new id is a new page.
  ///   - content: The page.
  init(id: ID, @ViewBuilder content: () -> Content) {
    self.id = id
    self.content = content()
  }

  /// The page, over the one leaving while it fades.
  var body: some View {
    ZStack {
      content
        .id(id)
        .transition(Self.pages)
    }
  }

  /// How one page gives way to the next, each side carrying its own animation, since a window's state changes
  /// without one: the new page fades in on ``Motion/pageIn`` as the last fades out on ``Motion/pageOut``.
  static var pages: AnyTransition {
    .asymmetric(
      insertion: .opacity.animation(Motion.pageIn),
      removal: .opacity.animation(Motion.pageOut)
    )
  }
}

#if DEBUG
  #Preview("Interactive") {
    @Previewable @State var page = 0
    InteractivePreview {
      HStack {
        Button(String("Back")) { page -= 1 }
        Button(String("Next")) { page += 1 }
      }
    } content: {
      PageStage(id: page) {
        Text(String("Page \(page)"))
          .font(Typography.font(.title, weight: .semibold))
          .foregroundStyle(Color.textPrimary)
      }
      .frame(width: SampleWidth.column, height: SampleWidth.column / 2)
    }
  }
#endif
