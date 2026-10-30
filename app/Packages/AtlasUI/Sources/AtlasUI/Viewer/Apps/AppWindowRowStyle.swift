import SwiftUI

/// A window's row on its card as a button: a true capsule, filled for the front window of the app in front and clear
/// for the rest, a step brighter under the pointer, where "Show" shows at its end, and brighter again while held. A
/// press shows at once and eases back; nothing moves or resizes.
struct AppWindowRowStyle: ButtonStyle {
  /// Whether it is the front window of the app in front.
  let isFront: Bool
  /// Whether "Show" shows at its end under the pointer: for a window, not for "N more".
  var offersShow = true
  @Environment(\.hoverPaused) private var hoverPaused

  /// The row on its capsule.
  func makeBody(configuration: Configuration) -> some View {
    AppWindowRowCapsule(
      configuration: configuration,
      isFront: isFront,
      offersShow: offersShow,
      hoverPaused: hoverPaused
    )
  }
}
