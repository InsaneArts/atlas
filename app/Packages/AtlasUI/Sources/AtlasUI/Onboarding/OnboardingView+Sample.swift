import SwiftUI

#if DEBUG
  extension OnboardingView where DragIcon == AppIcon {
    /// Creates the window's content with a still icon in the permission panel, as previews and renders draw it.
    ///
    /// - Parameters:
    ///   - state: What the window shows.
    ///   - form: The new helper account's form, which the account screen edits.
    ///   - send: Receives what the person does.
    public init(
      state: OnboardingState,
      form: Binding<OnboardingAccountForm>,
      send: @escaping @MainActor (OnboardingEvent) -> Void
    ) {
      self.init(state: state, form: form, dragIcon: AppIcon(size: OnboardingLayout.dragIcon), send: send)
    }
  }
#endif
