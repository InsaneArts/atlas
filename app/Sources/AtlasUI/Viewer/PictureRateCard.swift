import SwiftUI

/// How often the picture refreshes, as a picker on a card, and the note on what pictures are kept.
struct PictureRateCard: View {
  /// How often a picture is read while watching.
  let rate: ViewerPictureRate
  /// The note under the card.
  let note: String
  /// Receives the rate picked.
  let pick: @MainActor (ViewerPictureRate) -> Void

  /// The picker's row, then the note.
  var body: some View {
    CardGroup {
      ControlRow {
        Text("Refresh")
          .font(Typography.font(.body))
          .foregroundStyle(Color.textSecondary)
      } control: {
        Picker("Refresh", selection: Binding(get: { rate }, set: { pick($0) })) {
          ForEach(ViewerPictureRate.allCases, id: \.self) { rate in
            Text(rate.title)
              .tag(rate)
          }
        }
        .labelsHidden()
        .fixedSize()
      }
    }
    Note(text: note)
      .multilineTextAlignment(.leading)
      .padding(.horizontal, Spacing.xs)
  }
}

#if DEBUG
  #Preview("Every 2 seconds") {
    VStack(alignment: .leading, spacing: Spacing.m) {
      PictureRateCard(rate: .everyTwoSeconds, note: PreviewStates.viewerWatching.sidebar.pictureNote) { _ in }
    }
    .frame(width: SampleWidth.narrow)
    .sampleSurface()
  }
#endif
