#if DEBUG
  import SwiftUI

  /// The frame an interactive preview draws in: an appearance switch and its controls above the component, on the
  /// ground every window uses. The switch draws the component in the canvas's own appearance, or in Light or Dark,
  /// so both can be checked without leaving the preview. The component's own `#Preview("Interactive")` holds the
  /// state in `@Previewable @State`, so clicking and toggling in the canvas behave as in the app.
  struct InteractivePreview<Controls: View, Content: View>: View {
    private let controls: Controls
    private let content: Content
    /// The appearance picked, or nil for the canvas's own.
    @State private var appearance: ColorScheme?
    @Environment(\.colorScheme) private var canvasAppearance

    /// Creates the frame.
    ///
    /// - Parameters:
    ///   - controls: Toggles and pickers that change the component's inputs.
    ///   - content: The component to interact with.
    init(@ViewBuilder controls: () -> Controls, @ViewBuilder content: () -> Content) {
      self.controls = controls()
      self.content = content()
    }

    /// The appearance switch and the controls, then the component, all in the appearance picked.
    var body: some View {
      VStack(spacing: Spacing.xl) {
        VStack(alignment: .leading, spacing: Spacing.s) {
          Picker(String("Appearance"), selection: $appearance) {
            Text(verbatim: "Canvas")
              .tag(ColorScheme?.none)
            Text(verbatim: "Light")
              .tag(ColorScheme?.some(.light))
            Text(verbatim: "Dark")
              .tag(ColorScheme?.some(.dark))
          }
          .pickerStyle(.segmented)
          .fixedSize()
          controls
        }
        content
      }
      .sampleSurface()
      .environment(\.colorScheme, appearance ?? canvasAppearance)
    }
  }

  extension InteractivePreview where Controls == EmptyView {
    /// Creates the frame for a component that is interactive on its own, such as a switch.
    ///
    /// - Parameter content: The component to interact with.
    init(@ViewBuilder content: () -> Content) {
      self.init(controls: { EmptyView() }, content: content)
    }
  }
#endif
