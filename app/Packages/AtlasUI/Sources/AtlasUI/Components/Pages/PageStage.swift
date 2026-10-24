import SwiftUI

/// Where a window shows one page at a time: a new page, one with a new id, slides in the way the person goes as the
/// last one slides out (``PageSlide``); a change on the same page redraws it in place. Whatever sits outside the stage,
/// such as a footer, stays put while the pages change (v3 S1).
struct PageStage<ID: Hashable, Content: View>: View {
  /// Which page shows: a new id is a new page.
  private let id: ID
  /// Which way the pages move to this one.
  private let direction: PageDirection
  /// The page.
  private let content: Content
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// Creates a stage showing one page.
  ///
  /// - Parameters:
  ///   - id: Which page shows: a new id is a new page.
  ///   - direction: Which way the pages move to this one.
  ///   - content: The page.
  init(id: ID, direction: PageDirection, @ViewBuilder content: () -> Content) {
    self.id = id
    self.direction = direction
    self.content = content()
  }

  /// The page, over the one leaving while it leaves.
  var body: some View {
    ZStack {
      content
        .id(id)
        .transition(PageSlide.pages(reduceMotion: reduceMotion))
    }
    // Set above the pages, so the page leaving slides the new way too.
    .environment(\.pageDirection, direction)
  }
}

#if DEBUG
  #Preview("Interactive") {
    @Previewable @State var page = 0
    @Previewable @State var direction = PageDirection.forward
    InteractivePreview {
      HStack {
        Button(String("Back")) {
          direction = .back
          page -= 1
        }
        Button(String("Next")) {
          direction = .forward
          page += 1
        }
      }
    } content: {
      PageStage(id: page, direction: direction) {
        Text(String("Page \(page)"))
          .font(Typography.font(.title, weight: .semibold))
          .foregroundStyle(Color.textPrimary)
      }
      .frame(width: SampleWidth.column, height: SampleWidth.column / 2)
    }
  }
#endif
