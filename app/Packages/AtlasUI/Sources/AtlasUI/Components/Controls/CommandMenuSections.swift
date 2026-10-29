import SwiftUI

/// A menu's sections, each under its heading when it has one, a separator between them; each item drawn by the
/// caller. Items may share a title, as two facts can, so each is told apart by its place.
struct CommandMenuSections<Entry: View>: View {
  /// The sections, in order.
  let sections: [ViewerMenuSection]
  /// Draws one item.
  @ViewBuilder let entry: (ViewerMenuItem) -> Entry

  /// The sections, apart.
  var body: some View {
    ForEach(Array(sections.enumerated()), id: \.offset) { index, section in
      if index > .zero {
        Divider()
      }
      if let title = section.title {
        Section(title) {
          items(section)
        }
      } else {
        items(section)
      }
    }
  }

  /// A section's items, each by its place.
  ///
  /// - Parameter section: The section.
  /// - Returns: Its items.
  private func items(_ section: ViewerMenuSection) -> some View {
    ForEach(Array(section.items.enumerated()), id: \.offset) { _, item in
      entry(item)
    }
  }
}
