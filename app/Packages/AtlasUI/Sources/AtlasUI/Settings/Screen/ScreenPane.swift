import SwiftUI

/// The canonical screen cadence control, with a standard popup for accessibility and narrow layouts.
struct ScreenPane: View {
  /// The shared preference values and exact existing choices.
  let state: SettingsGeneralState
  /// Receives only the edited picture field.
  let send: @MainActor (SettingsEvent) -> Void
  /// VoiceOver uses the existing labeled popup instead of illustrated cards.
  @Environment(\.accessibilityVoiceOverEnabled) private var voiceOver
  /// Allows deterministic inspection of the accessible presentation without changing system settings.
  @Environment(\.settingsPicturePopup) private var popupRequested

  /// One selection and one edit path, whichever presentation fits.
  var body: some View {
    VStack(alignment: .leading, spacing: SettingsLayout.titleGap) {
      SettingsSectionHeading(title: "Screen updates")
      GroupedSection(
        [GeneralRow.picture],
        notes: state.pictureOptions.first(where: { $0.id == .paused })?.isEnabled == false
          ? [.text("Open the main window to pause its picture.")] : []
      ) { _ in
        if voiceOver || popupRequested {
          popup
        } else {
          ViewThatFits(in: .horizontal) {
            HStack(alignment: .top, spacing: Spacing.s) {
              ForEach(state.pictureOptions) { option in
                PictureChoiceCard(option: option, selected: state.picture == option.id) {
                  send(.choosePicture(option.id))
                }
              }
            }
            .padding(Spacing.m)
            popup
          }
        }
      }
      .accessibilityLabel("Screen updates")
    }
  }

  /// The standard menu shares its options, selection and writer with the cards.
  private var popup: some View {
    ListRowBody("Screen updates", detail: "How often Atlas's windows show the helper's screen", size: .compact) {
      PopupButton(
        label: "Screen updates",
        options: state.pictureOptions,
        selection: Binding(get: { state.picture }, set: { send(.choosePicture($0)) })
      )
    }
  }
}
