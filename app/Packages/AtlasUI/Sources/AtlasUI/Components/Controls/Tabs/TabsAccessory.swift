import SwiftUI

/// A tab's label with what it carries, such as Activity's dot, hung a gap after its trailing edge and outside its
/// frame, so the label alone is centred in the tab and sizes it: the gaps between the words stay even, and the thumb
/// sits on the tab whole, whether the accessory shows or not.
struct TabsAccessory<Accessory: View>: ViewModifier {
  /// What the tab carries.
  let accessory: Accessory
  /// From the label's trailing edge to the accessory.
  let gap: CGFloat

  /// The label, the accessory over its trailing side.
  func body(content: Content) -> some View {
    content
      .overlay(alignment: .trailing) {
        accessory
          .alignmentGuide(.trailing) { dimensions in
            dimensions[.leading] - gap
          }
      }
  }
}
