#if DEBUG
  import SwiftUI

  /// Reports one element's actual geometry to a debug renderer; it changes no layout or transaction.
  struct LayoutFrame: ViewModifier {
    /// The element's stable identity.
    let id: String
    @Environment(\.layoutFrameReport) private var observer

    /// The content with an optional geometry observation.
    func body(content: Content) -> some View {
      content
        .onGeometryChange(for: CGRect.self) {
          $0.frame(in: .global)
        } action: {
          observer?.report(id, $0)
        }
    }
  }
#endif
