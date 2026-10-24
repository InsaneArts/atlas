#if DEBUG
  import SwiftUI

  /// The frame an interactive preview draws in: its controls above the component, on the dark ground every window
  /// uses. The component's own `#Preview("Interactive")` holds the state in `@Previewable @State`, so clicking and
  /// toggling in the canvas behave as in the app.
  struct InteractivePreview<Controls: View, Content: View>: View {
    private let controls: Controls
    private let content: Content

    /// Creates the frame.
    ///
    /// - Parameters:
    ///   - controls: Toggles and pickers that change the component's inputs.
    ///   - content: The component under review.
    init(@ViewBuilder controls: () -> Controls, @ViewBuilder content: () -> Content) {
      self.controls = controls()
      self.content = content()
    }

    /// The controls, then the component.
    var body: some View {
      VStack(spacing: Spacing.xl) {
        VStack(alignment: .leading, spacing: Spacing.s) {
          controls
        }
        content
      }
      .sampleSurface()
    }
  }

  extension InteractivePreview where Controls == EmptyView {
    /// Creates the frame for a component that is interactive on its own, such as a switch.
    ///
    /// - Parameter content: The component under review.
    init(@ViewBuilder content: () -> Content) {
      self.init(controls: { EmptyView() }, content: content)
    }
  }
#endif
