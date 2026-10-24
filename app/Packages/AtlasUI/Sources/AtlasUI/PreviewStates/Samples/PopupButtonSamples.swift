#if DEBUG
  import SwiftUI

  /// The popup button's samples: a picture rate with each choice selected, one of them not offered now.
  struct PopupButtonSamples: View {
    /// A picture rate, standing in for the app's own.
    enum Rate: Hashable, Sendable {
      /// As often as the window reads.
      case live
      /// Every five seconds.
      case everyFiveSeconds
      /// No new pictures.
      case paused
    }

    /// The choices, Paused not offered.
    static let options = [
      PopupOption(id: Rate.live, title: "Live"),
      PopupOption(id: Rate.everyFiveSeconds, title: "Every 5 seconds"),
      PopupOption(id: Rate.paused, title: "Paused", isEnabled: false),
    ]

    var body: some View {
      HStack(spacing: Spacing.l) {
        PopupButton(label: String("Picture"), options: Self.options, selection: .constant(.live))
        PopupButton(label: String("Picture"), options: Self.options, selection: .constant(.everyFiveSeconds))
      }
      .sampleSurface()
    }
  }
#endif
