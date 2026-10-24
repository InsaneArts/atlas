import SwiftUI

/// An alert in the widget's look: Atlas's icon, a bold title, what it means, and full-width capsules stacked under
/// it. It is modal: Return presses the default button and Escape the way out. Its frosted ground is drawn opaque
/// under Reduce Transparency. The window lays it over its scrim.
struct OrbitAlert<ID: Hashable & Sendable>: View {
  /// What the alert says and offers.
  let state: OrbitAlertState<ID>
  /// Receives the button the person presses.
  let send: @MainActor (ID) -> Void
  @Environment(\.accessibilityReduceTransparency) private var reduceTransparency

  var body: some View {
    VStack(spacing: Spacing.m) {
      AppIcon(size: IconSize.hero)
        .padding(.bottom, Spacing.s)
      Text(state.title)
        .font(Typography.font(.headline, weight: .bold))
        .accessibilityAddTraits(.isHeader)
      Text(state.message)
        .font(Typography.font(.callout))
        .foregroundStyle(Color.textAlert)
      VStack(spacing: Spacing.m) {
        ForEach(state.buttons) { button in
          Button(button.title, role: button.role.buttonRole) { send(button.id) }
            .buttonStyle(.capsule(button.role.look, size: .small, fill: true))
            .keyboardShortcut(button.role.shortcut)
        }
      }
      .disabled(state.isBusy)
      .padding(.top, Layout.buttonsTop)
    }
    // Escape for an alert whose way out takes Return: a key of its own, which works without a focused view, as
    // `onExitCommand` alone does not unless Full Keyboard Access is on.
    .background {
      if state.escapeNeedsKey {
        Button("") { if let escape = state.escape { send(escape) } }
          .keyboardShortcut(.cancelAction)
          .frame(width: .zero, height: .zero)
          .opacity(.zero)
          .accessibilityHidden(true)
      }
    }
    .multilineTextAlignment(.center)
    .fixedSize(horizontal: false, vertical: true)
    .padding(.top, Layout.topInset)
    .padding([.horizontal, .bottom], Spacing.xxl)
    .frame(width: Layout.width)
    // The shadow falls from the ground's shape alone, so it is drawn from a path, never blurred from the content.
    .background {
      ground
        .elevation(.alert)
    }
    .overlay(
      RoundedRectangle(cornerRadius: Radius.alert)
        .strokeBorder(Color.strokeStrong, lineWidth: Stroke.hairline)
    )
    .onExitCommand { if let escape = state.escape { send(escape) } }
    .accessibilityElement(children: .contain)
    .accessibilityAddTraits(.isModal)
  }

  /// The frosted ground, or the opaque one under Reduce Transparency.
  @ViewBuilder private var ground: some View {
    let shape = RoundedRectangle(cornerRadius: Radius.alert)
    if reduceTransparency {
      shape
        .fill(Color.fillAlertSolid)
    } else {
      shape
        .fill(Color.fillAlert)
        .background(.ultraThinMaterial, in: shape)
    }
  }
}

/// At file scope because `OrbitAlert` is generic: the board's measures (mac.py `alert`).
private enum Layout {
  /// 280: the alert's width.
  static let width: CGFloat = 280
  /// 22: above the icon, more than the 16 at the sides and bottom so the icon has room.
  static let topInset: CGFloat = 22
  /// 10: between the message and the buttons, past the 8 between the other parts.
  static let buttonsTop: CGFloat = 10
}

#if DEBUG
  #Preview("Samples") {
    OrbitAlertSamples()
  }

  #Preview("Interactive") {
    @Previewable @State var pressed = String("Nothing pressed yet")
    @Previewable @State var isBusy = false
    InteractivePreview {
      Toggle(String("Busy"), isOn: $isBusy)
      Text(pressed)
    } content: {
      OrbitAlert(state: OrbitAlertSamples.logOut(isBusy: isBusy)) { id in
        pressed = id
      }
    }
  }
#endif
