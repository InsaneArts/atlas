#if DEBUG
  import SwiftUI

  /// One component's samples, as its preview draws them.
  public struct ComponentSampleView: View {
    /// Which component.
    private let sample: ComponentSample

    /// Creates the samples of one component.
    ///
    /// - Parameter sample: Which component.
    public init(_ sample: ComponentSample) {
      self.sample = sample
    }

    /// The chosen component's samples.
    public var body: some View {
      switch sample {
      case .capsuleTabs: CapsuleTabsSamples()
      case .capsuleButtons: CapsuleButtonSamples(.every)
      case .captionStack: CaptionStackSamples()
      case .statusDots: IndicatorSamples(.statusDot)
      case .statusLabel: StatusLabelSamples()
      case .floatingChip: FloatingChipSamples()
      case .alertCard: AlertCardSamples()
      case .emptyMessage: EmptyMessageSamples()
      case .pictureFrame: PictureFrameSamples()
      case .miniScreen: MiniScreenSamples()
      case .paneHeader: PaneHeaderSamples()
      case .searchField: SearchFieldSamples()
      case .groupLabel: GroupLabelSamples()
      case .iconTabs: IconTabsSamples()
      case .formRow: FormRowSamples()
      case .popupButton: PopupButtonSamples()
      case .appIconTiles: BrandSamples(.appIcon)
      case .accountPictureSizes: BrandSamples(.accountPicture)
      case .listRowChoices: ListRowSamples(.choices)
      case .listRowLeads: ListRowSamples(.leads)
      case .listRowTrailing: ListRowSamples(.trailing)
      case .listRowSizes: ListRowSamples(.sizes)
      case .groupedListScrolling: ListRowSamples(.scrolling)
      case .statusLineKinds: StatusLineSamples(.kinds)
      case .progressTrackSteps: IndicatorSamples(.progressTrack)
      case .heroDiscKinds: IndicatorSamples(.heroDisc)
      case .glassChipKinds: GlassChipSamples(.chips)
      case .capsuleButtonCircles: CapsuleButtonSamples(.circles)
      case .actionSlotActions: ActionSlotSamples(isInControl: true)
      case .symbolTileSizes: IndicatorSamples(.symbolTile)
      case .symbolNames: SymbolSamples()
      }
    }
  }
#endif
