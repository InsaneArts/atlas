#if DEBUG
  import SwiftUI

  /// An inspector's facts: a group of plain facts, then a group of checks, one passing, one failing with what
  /// to do, and one not checked yet.
  struct FactSamples: View {
    /// The two groups on an inspector's card.
    var body: some View {
      VStack(alignment: .leading, spacing: Spacing.mPlus) {
        FactGroup(String("Helper")) {
          FactRow(String("Account"), value: String("Atlas"))
          FactRow(String("Display"), value: String("1440 × 900"))
        }
        FactGroup(String("Ready to act")) {
          FactRow(
            String("Mouse and keyboard"),
            value: String("Not ready. Recover… starts them again."),
            mark: .notReady
          )
          FactRow(String("Save and Open dialogs"), value: String("Not checked yet"), mark: .unknown)
          FactRow(String("1 more check passes"), mark: .ready)
        }
      }
      .padding(Spacing.mPlus)
      .frame(width: 280)
      .sampleSurface()
    }
  }
#endif
