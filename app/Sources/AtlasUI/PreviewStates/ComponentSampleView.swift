#if DEBUG
  import SwiftUI

  /// One component's samples, as its preview draws them.
  package struct ComponentSampleView: View {
    /// Which component.
    private let sample: ComponentSample

    /// Creates the samples of one component.
    ///
    /// - Parameter sample: Which component.
    package init(_ sample: ComponentSample) {
      self.sample = sample
    }

    package var body: some View {
      switch sample {
      case .capsuleTabs: CapsuleTabsSamples()
      case .capsuleButtons: CapsuleButtonSamples(.every)
      case .captionStack: CaptionStackSamples()
      case .statusDots: IndicatorSamples(.statusDot)
      case .statusLabel: StatusLabelSamples()
      case .floatingChip: FloatingChipSamples()
      case .orbitAlert: OrbitAlertSamples()
      case .emptyMessage: EmptyMessageSamples()
      case .pictureFrame: PictureFrameSamples()
      case .miniScreen: MiniScreenSamples()
      case .paneHeader: PaneHeaderSamples()
      case .searchField: SearchFieldSamples()
      case .groupLabel: GroupLabelSamples()
      case .iconTabs: IconTabsSamples()
      case .formRow: FormRowSamples()
      case .popupButton: PopupButtonSamples()
      }
    }
  }
#endif
